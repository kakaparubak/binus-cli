use base64::Engine;
use futures_util::{SinkExt, StreamExt};
#[cfg(not(windows))]
use rustix::fs::{self as rfs, AtFlags, Mode, OFlags};
use serde::Serialize;
use serde_json::{Value, json};
#[cfg(all(windows, test))]
use std::fs;
#[cfg(not(windows))]
use std::path::Path;
use std::path::PathBuf;
#[cfg(not(windows))]
use std::process::{Child, Command, Stdio};
use std::{collections::HashMap, time::Duration};
#[cfg(not(windows))]
use std::{
    fs,
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd, IntoRawFd},
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

mod browser;
mod credential_record;
#[cfg(windows)]
use browser::Browser;
#[cfg(any(windows, test))]
pub(crate) use credential_record::valid_credential_record;
pub use credential_record::{CredentialRecord, RoleContext};
#[cfg(any(windows, test))]
mod windows_store;
#[cfg(windows)]
pub use windows_store::{
    load_preferred_record, load_record, promote_verified_candidate, remove_record_file,
    remove_spike_artifacts, store_record,
};
pub mod cli;
pub mod courses;
pub mod forums;
pub mod session_details;
pub mod sessions;
#[cfg(any(windows, test))]
pub mod windows_lock;

const CAPTURE_EXPRESSION: &str = "(() => {if(location.origin!=='https://lms.binus.ac.id')return null;const chunks=window['webpackJsonpbinus-lms-web'];if(!chunks)return null;if(!window.__binusResearchRequire)chunks.push([[98768],{98768:function(m,e,r){window.__binusResearchRequire=r}},[[98768]]]);const r=window.__binusResearchRequire;if(!r||!r.c)return null;const cached=r.c[34];if(!cached||cached.l!==true)return null;const api=cached.exports&&cached.exports.b;if(!api||typeof api.getState!=='function')return null;const state=api.getState();const auth=state&&state.auth;const a=auth&&auth.res;if(!a||!a.token||!a.refreshToken)return null;return {accessToken:a.token,refreshToken:a.refreshToken,activeRole:a.activeRole,activeRoleOrganizationId:a.activeRoleOrganizationId,account:a.email||a.originalLoginEmail||''};})()";

#[derive(Serialize)]
pub struct Status {
    pub status: &'static str,
    pub stored: bool,
    pub live_verified: bool,
    pub blocker: Option<&'static str>,
}

pub fn sanitized_status(
    stored: bool,
    live_verified: bool,
    blocker: Option<&'static str>,
) -> Status {
    Status {
        status: if live_verified {
            "live_verified"
        } else if stored {
            "stored_unverified"
        } else {
            "not_logged_in"
        },
        stored,
        live_verified,
        blocker,
    }
}

pub fn trusted_exchange_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str() == Some("apim-bm7-prod-web.azure-api.net")
            && u.port().is_none()
            && u.path() == "/func-bm7-identity-prod/AccessAndRefreshToken/AzureAD"
    })
}

pub fn parse_pair(body: &str) -> Result<(String, String), &'static str> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| "invalid_capture")?;
    let token = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .filter(|s| {
                s.len() >= 16 && s.len() <= 32_768 && s.bytes().all(|b| b.is_ascii_graphic())
            })
            .map(str::to_owned)
            .ok_or("invalid_capture")
    };
    Ok((token("accessToken")?, token("refreshToken")?))
}

#[cfg(not(windows))]
fn secure_parent(path: &Path, create: bool) -> Result<std::os::fd::OwnedFd, &'static str> {
    let parent = path.parent().ok_or("storage_error")?;
    if !parent.is_absolute()
        || parent.file_name().and_then(|n| n.to_str()) != Some("binus-auth-spike")
    {
        return Err("storage_error");
    }
    let mut fd = rfs::open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| "storage_error")?;
    for component in parent.components().skip(1) {
        let std::path::Component::Normal(part) = component else {
            return Err("storage_error");
        };
        let part = part.to_str().ok_or("storage_error")?;
        let next = match rfs::openat(
            &fd,
            part,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(next) => next,
            Err(e) if create && e == rustix::io::Errno::NOENT => {
                match rfs::mkdirat(&fd, part, Mode::from_raw_mode(0o700)) {
                    Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                    Err(_) => return Err("storage_error"),
                }
                rfs::openat(
                    &fd,
                    part,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|_| "storage_error")?
            }
            Err(e) if !create && e == rustix::io::Errno::NOENT => return Err("not_logged_in"),
            Err(_) => return Err("storage_error"),
        };
        let meta = rfs::fstat(&next).map_err(|_| "storage_error")?;
        let uid = rustix::process::geteuid().as_raw();
        let sticky_tmp = meta.st_mode & 0o1000 != 0 && meta.st_mode & 0o002 != 0;
        if meta.st_mode & 0o170000 != 0o040000
            || ((meta.st_uid != uid && meta.st_uid != 0) && !sticky_tmp)
            || (meta.st_mode & 0o022 != 0 && !sticky_tmp)
        {
            return Err("storage_error");
        }
        fd = next;
    }
    Ok(fd)
}

#[cfg(not(windows))]
pub fn store_record(path: &Path, record: &CredentialRecord) -> Result<(), &'static str> {
    let _lock = storage_lock(path, true)?;
    store_record_unlocked(path, record)
}

#[cfg(not(windows))]
fn store_record_unlocked(path: &Path, record: &CredentialRecord) -> Result<(), &'static str> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("storage_error")?;
    let dir = secure_parent(path, true)?;
    rfs::fchmod(&dir, Mode::from_raw_mode(0o700)).map_err(|_| "storage_error")?;
    match rfs::statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(meta)
            if meta.st_mode & 0o170000 == 0o100000
                && meta.st_uid == rustix::process::geteuid().as_raw()
                && meta.st_mode & 0o077 == 0 => {}
        Ok(_) => return Err("storage_error"),
        Err(e) if e == rustix::io::Errno::NOENT => {}
        Err(_) => return Err("storage_error"),
    }
    let temp_prefix = name.strip_suffix(".json").unwrap_or(name);
    let tmp = format!("{temp_prefix}.tmp-{}", std::process::id());
    let fd = rfs::openat(
        &dir,
        tmp.as_str(),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .map_err(|_| "storage_error")?;
    let mut file = unsafe { std::fs::File::from_raw_fd(fd.into_raw_fd()) };
    let bytes = serde_json::to_vec(record).map_err(|_| "storage_error")?;
    let outcome = (|| {
        file.write_all(&bytes).map_err(|_| "storage_error")?;
        file.sync_all().map_err(|_| "storage_error")?;
        rfs::renameat(&dir, tmp.as_str(), &dir, name).map_err(|_| "storage_error")?;
        Ok(())
    })();
    if outcome.is_err() {
        let _ = rfs::unlinkat(&dir, tmp.as_str(), AtFlags::empty());
    }
    outcome
}

#[cfg(not(windows))]
pub fn load_record(path: &Path) -> Result<CredentialRecord, &'static str> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("storage_error")?;
    let dir = secure_parent(path, false)?;
    validate_private_app_dir(&dir)?;
    let fd = rfs::openat(
        &dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| {
        if e == rustix::io::Errno::NOENT {
            "not_logged_in"
        } else {
            "storage_error"
        }
    })?;
    let meta = rfs::fstat(&fd).map_err(|_| "storage_error")?;
    if meta.st_mode & 0o170000 != 0o100000
        || meta.st_mode & 0o077 != 0
        || meta.st_uid != rustix::process::geteuid().as_raw()
    {
        return Err("storage_error");
    }
    let file = unsafe { std::fs::File::from_raw_fd(fd.into_raw_fd()) };
    let mut bytes = Vec::new();
    file.take(1_000_001)
        .read_to_end(&mut bytes)
        .map_err(|_| "storage_error")?;
    if bytes.len() > 1_000_000 {
        return Err("storage_error");
    }
    let record: CredentialRecord = serde_json::from_slice(&bytes).map_err(|_| "storage_error")?;
    if !valid_token(&record.access_token)
        || !valid_token(&record.refresh_token)
        || [
            record.role.academic_career.as_str(),
            record.role.institution.as_str(),
            record.role.name.as_str(),
            record.role.role_id.as_str(),
            record.role.organization_id.as_str(),
        ]
        .iter()
        .any(|s| s.is_empty() || s.len() > 256)
    {
        return Err("storage_error");
    }
    Ok(record)
}

#[cfg(not(windows))]
pub fn remove_spike_artifacts(path: &Path) -> Result<bool, &'static str> {
    if path.file_name().and_then(|n| n.to_str()) != Some("credentials.json") {
        return Err("storage_error");
    }
    let dir = match secure_parent(path, false) {
        Ok(fd) => fd,
        Err("not_logged_in") => return Ok(false),
        Err(_) => return Err("storage_error"),
    };
    let _lock = storage_lock(path, false)?;
    validate_private_app_dir(&dir)?;
    let primary = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("storage_error")?;
    let mut names = vec![primary.to_owned(), "credentials.captured.json".to_owned()];
    let proc_dir = PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()));
    for entry in fs::read_dir(proc_dir).map_err(|_| "storage_error")? {
        let entry = entry.map_err(|_| "storage_error")?;
        if let Some(name) = entry
            .file_name()
            .to_str()
            .filter(|n| recognized_temp_name(n))
        {
            names.push(name.to_owned());
        }
    }
    names.sort();
    names.dedup();
    let mut removed = false;
    let mut failed = false;
    for name in names {
        let meta = match rfs::statat(&dir, name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
            Ok(meta) => meta,
            Err(e) if e == rustix::io::Errno::NOENT => continue,
            Err(_) => {
                failed = true;
                continue;
            }
        };
        if meta.st_mode & 0o170000 != 0o100000
            || meta.st_uid != rustix::process::geteuid().as_raw()
            || meta.st_mode & 0o077 != 0
        {
            failed = true;
            continue;
        }
        match rfs::unlinkat(&dir, name.as_str(), AtFlags::empty()) {
            Ok(()) => removed = true,
            Err(_) => failed = true,
        }
    }
    if failed {
        Err("storage_error")
    } else {
        Ok(removed)
    }
}

#[cfg(not(windows))]
pub fn remove_record_file(path: &Path) -> Result<bool, &'static str> {
    let _lock = storage_lock(path, false)?;
    remove_record_file_unlocked(path)
}

#[cfg(not(windows))]
fn remove_record_file_unlocked(path: &Path) -> Result<bool, &'static str> {
    if !matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("credentials.json" | "credentials.captured.json")
    ) {
        return Err("storage_error");
    }
    let dir = match secure_parent(path, false) {
        Ok(fd) => fd,
        Err("not_logged_in") => return Ok(false),
        Err(_) => return Err("storage_error"),
    };
    validate_private_app_dir(&dir)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("storage_error")?;
    let meta = match rfs::statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(meta) => meta,
        Err(e) if e == rustix::io::Errno::NOENT => return Ok(false),
        Err(_) => return Err("storage_error"),
    };
    if meta.st_mode & 0o170000 != 0o100000
        || meta.st_uid != rustix::process::geteuid().as_raw()
        || meta.st_mode & 0o077 != 0
    {
        return Err("storage_error");
    }
    rfs::unlinkat(&dir, name, AtFlags::empty()).map_err(|_| "storage_error")?;
    Ok(true)
}

#[cfg(not(windows))]
fn validate_private_app_dir(dir: &std::os::fd::OwnedFd) -> Result<(), &'static str> {
    let meta = rfs::fstat(dir).map_err(|_| "storage_error")?;
    if meta.st_mode & 0o170000 != 0o040000
        || meta.st_uid != rustix::process::geteuid().as_raw()
        || meta.st_mode & 0o077 != 0
    {
        return Err("storage_error");
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn promote_verified_candidate(
    primary: &Path,
    candidate: &Path,
    verified: &CredentialRecord,
) -> Result<bool, &'static str> {
    let _lock = storage_lock(primary, true)?;
    let record = load_record(candidate)?;
    if serde_json::to_vec(&record).ok() != serde_json::to_vec(verified).ok() {
        return Err("candidate_changed");
    }
    store_record_unlocked(primary, verified)?;
    remove_record_file_unlocked(candidate)?;
    Ok(true)
}

#[cfg(not(windows))]
fn storage_lock(path: &Path, create: bool) -> Result<std::os::fd::OwnedFd, &'static str> {
    let dir = secure_parent(path, create)?;
    let meta = rfs::fstat(&dir).map_err(|_| "storage_error")?;
    if meta.st_uid != rustix::process::geteuid().as_raw() || meta.st_mode & 0o170000 != 0o040000 {
        return Err("storage_error");
    }
    rfs::fchmod(&dir, Mode::from_raw_mode(0o700)).map_err(|_| "storage_error")?;
    validate_private_app_dir(&dir)?;
    let fd = rfs::openat(
        &dir,
        ".auth-spike.lock",
        OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .map_err(|_| "storage_error")?;
    let meta = rfs::fstat(&fd).map_err(|_| "storage_error")?;
    if meta.st_mode & 0o170000 != 0o100000
        || meta.st_uid != rustix::process::geteuid().as_raw()
        || meta.st_mode & 0o077 != 0
    {
        return Err("storage_error");
    }
    rfs::flock(&fd, rustix::fs::FlockOperation::LockExclusive).map_err(|_| "storage_error")?;
    Ok(fd)
}

#[cfg(not(windows))]
pub fn load_preferred_record(primary: &Path) -> Result<(CredentialRecord, bool), &'static str> {
    let candidate = primary.with_file_name("credentials.captured.json");
    match load_record(&candidate) {
        Ok(record) => Ok((record, true)),
        Err("not_logged_in") => load_record(primary).map(|record| (record, false)),
        Err(error) => Err(error),
    }
}

#[cfg(not(windows))]
fn recognized_temp_name(name: &str) -> bool {
    [
        "credentials.tmp-",
        "credentials.captured.tmp-",
        "credentials.json.tmp-",
        "credentials.captured.json.tmp-",
    ]
    .iter()
    .any(|prefix| {
        name.strip_prefix(prefix)
            .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
    })
}

pub fn academic_period_request(
    record: &CredentialRecord,
) -> Result<reqwest::Request, &'static str> {
    reqwest::Client::new()
        .get("https://apim-bm7-prod-web.azure-api.net/func-bm7-course-prod/AcademicPeriod/Student")
        .bearer_auth(&record.access_token)
        .header("rOId", &record.role.organization_id)
        .header("academicCareer", &record.role.academic_career)
        .header("institution", &record.role.institution)
        .header("roleName", &record.role.name)
        .header("roleId", &record.role.role_id)
        .build()
        .map_err(|_| "request_failed")
}

#[cfg(not(windows))]
struct Browser(Child);
#[cfg(not(windows))]
impl Browser {
    fn try_wait(&mut self) -> Result<Option<()>, &'static str> {
        self.0
            .try_wait()
            .map(|status| status.map(|_| ()))
            .map_err(|_| "browser_launch_failed")
    }
}
#[cfg(not(windows))]
impl Drop for Browser {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(pid) = rustix::process::Pid::from_raw(self.0.id() as i32) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::TERM);
            std::thread::sleep(Duration::from_millis(100));
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Cdp {
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    id: u64,
    session: Option<String>,
    guard_enable: HashMap<u64, String>,
    guard_remaining: HashMap<String, u8>,
    pending_acks: HashMap<u64, (Option<String>, &'static str)>,
    paused_count: u64,
    blocked_count: u64,
}
impl Cdp {
    async fn send(&mut self, method: &str, params: Value) -> Result<Value, &'static str> {
        self.id += 1;
        let id = self.id;
        let mut command = json!({"id":id,"method":method,"params":params});
        let expected_session = self.session.clone();
        if let Some(session) = &self.session {
            command["sessionId"] = json!(session);
        }
        self.ws
            .send(Message::Text(command.to_string().into()))
            .await
            .map_err(|_| "browser_protocol_error")?;
        loop {
            let message = self
                .ws
                .next()
                .await
                .ok_or("browser_protocol_error")?
                .map_err(|_| "browser_protocol_error")?;
            if let Message::Text(text) = message {
                let value: Value =
                    serde_json::from_str(&text).map_err(|_| "browser_protocol_error")?;
                if let Some(session) = value["id"]
                    .as_u64()
                    .and_then(|event_id| self.guard_enable.remove(&event_id))
                {
                    if value.get("error").is_some()
                        || value.get("sessionId").and_then(Value::as_str) != Some(session.as_str())
                    {
                        return Err("browser_protocol_error");
                    }
                    let remaining = self
                        .guard_remaining
                        .get_mut(&session)
                        .ok_or("browser_protocol_error")?;
                    *remaining -= 1;
                    if *remaining == 0 {
                        self.guard_remaining.remove(&session);
                        self.id += 1;
                        if self.pending_acks.len() >= 256 {
                            return Err("browser_protocol_error");
                        }
                        let resume = json!({"id":self.id,"method":"Runtime.runIfWaitingForDebugger","params":{},"sessionId":session});
                        self.pending_acks
                            .insert(self.id, (Some(session.clone()), "resume"));
                        self.ws
                            .send(Message::Text(resume.to_string().into()))
                            .await
                            .map_err(|_| "browser_protocol_error")?;
                    }
                    continue;
                }
                if let Some(ack_id) = value["id"]
                    .as_u64()
                    .filter(|ack_id| self.pending_acks.contains_key(ack_id))
                {
                    let (session, kind) = self
                        .pending_acks
                        .remove(&ack_id)
                        .ok_or("browser_protocol_error")?;
                    if value.get("sessionId").and_then(Value::as_str) != session.as_deref() {
                        return Err("browser_protocol_error");
                    }
                    if value.get("error").is_some() && !benign_ack_error(kind, &value["error"]) {
                        return Err("browser_protocol_error");
                    }
                    continue;
                }
                if value["id"].as_u64() == Some(id) {
                    if value.get("sessionId").and_then(Value::as_str) != expected_session.as_deref()
                    {
                        return Err("browser_protocol_error");
                    }
                    return if let Some(error) = value.get("error") {
                        if method == "Runtime.evaluate" && recognized_navigation_error(error) {
                            Err("evaluation_context_changed")
                        } else {
                            Err("browser_protocol_error")
                        }
                    } else {
                        Ok(value["result"].clone())
                    };
                }
                if value["method"] == "Fetch.requestPaused" {
                    self.paused_count += 1;
                    let request = &value["params"]["request"];
                    let url = request["url"].as_str().unwrap_or("");
                    let method = request["method"].as_str().unwrap_or("");
                    let continue_request = azure_request_allowed(method, url);
                    if !continue_request {
                        self.blocked_count += 1;
                    }
                    let cmd = if continue_request {
                        "Fetch.continueRequest"
                    } else {
                        "Fetch.failRequest"
                    };
                    let params = if continue_request {
                        json!({"requestId":value["params"]["requestId"]})
                    } else {
                        json!({"requestId":value["params"]["requestId"],"errorReason":"BlockedByClient"})
                    };
                    self.id += 1;
                    let mut response = json!({"id":self.id,"method":cmd,"params":params});
                    let session = value["sessionId"].as_str().map(str::to_owned);
                    if let Some(session) = &session {
                        response["sessionId"] = json!(session);
                    }
                    if self.pending_acks.len() >= 256 {
                        return Err("browser_protocol_error");
                    }
                    self.pending_acks.insert(self.id, (session, "fetch"));
                    self.ws
                        .send(Message::Text(response.to_string().into()))
                        .await
                        .map_err(|_| "browser_protocol_error")?;
                } else if value["method"] == "Target.attachedToTarget"
                    && let Some(session) = value["params"]["sessionId"].as_str()
                {
                    if self.guard_enable.len() + self.guard_remaining.len() >= 256 {
                        return Err("browser_protocol_error");
                    }
                    self.guard_remaining.insert(session.to_owned(), 2);
                    for (method, params) in [
                        ("Fetch.enable", fetch_params()),
                        ("Target.setAutoAttach", auto_attach_params()),
                    ] {
                        self.id += 1;
                        let guard_id = self.id;
                        self.guard_enable.insert(guard_id, session.to_owned());
                        let guard = json!({"id":guard_id,"method":method,"params":params,"sessionId":session});
                        self.ws
                            .send(Message::Text(guard.to_string().into()))
                            .await
                            .map_err(|_| "browser_protocol_error")?;
                    }
                }
            }
        }
    }
}

fn benign_ack_error(kind: &str, error: &Value) -> bool {
    let message = error["message"].as_str().unwrap_or("");
    match kind {
        "fetch" => {
            message.contains("No resource with given identifier")
                || message.contains("Target closed")
                || message.contains("Invalid InterceptionId")
        }
        "resume" => message.contains("Target closed"),
        _ => false,
    }
}

fn recognized_navigation_error(error: &Value) -> bool {
    let message = error["message"].as_str().unwrap_or("");
    message.contains("Execution context was destroyed")
        || message.contains("Cannot find context with specified id")
}

fn auto_attach_params() -> Value {
    json!({"autoAttach":true,"waitForDebuggerOnStart":true,"flatten":true,"filter":[{"type":"page"},{"type":"iframe"},{"type":"worker"},{"type":"service_worker"},{"type":"shared_worker"}]})
}

fn fetch_params() -> Value {
    json!({"patterns":[{"urlPattern":"*://azure-api.net/*","requestStage":"Request"},{"urlPattern":"*://*.azure-api.net/*","requestStage":"Request"},{"urlPattern":"*://azurewebsites.net/*","requestStage":"Request"},{"urlPattern":"*://*.azurewebsites.net/*","requestStage":"Request"},{"urlPattern":"*://binus.ac.id/*","requestStage":"Request"},{"urlPattern":"*://*.binus.ac.id/*","requestStage":"Request"}]})
}

pub async fn capture(timeout: Duration) -> Result<CredentialRecord, &'static str> {
    capture_with_cancellation(timeout, std::future::pending()).await
}

pub async fn capture_with_cancellation<C>(
    timeout: Duration,
    cancellation: C,
) -> Result<CredentialRecord, &'static str>
where
    C: std::future::Future<Output = ()>,
{
    #[cfg(windows)]
    {
        let temp = tempfile::tempdir().map_err(|_| "browser_launch_failed")?;
        let profile_path = temp.path().to_path_buf();
        let binary = browser::resolve_executable()?;
        let args = [
            "--remote-debugging-port=0".to_owned(),
            "--remote-debugging-address=127.0.0.1".to_owned(),
            "--no-first-run".to_owned(),
            "--no-default-browser-check".to_owned(),
            format!("--user-data-dir={}", profile_path.to_string_lossy()),
            "about:blank".to_owned(),
        ];
        let mut browser = Browser::spawn(binary, &args, temp)?;
        let operation = async {
            match tokio::time::timeout(timeout, capture_inner(profile_path, &mut browser)).await {
                Ok(result) => result,
                Err(_) => Err("capture_timeout"),
            }
        };
        let result = capture_operation(operation, cancellation).await;
        return finalize_capture(result, || async { browser.cleanup() }).await;
    }
    #[cfg(not(windows))]
    {
        let operation = async {
            match tokio::time::timeout(timeout, capture_inner()).await {
                Ok(result) => result,
                Err(_) => Err("capture_timeout"),
            }
        };
        capture_operation(operation, cancellation).await
    }
}

async fn capture_operation<T, Op, Can>(operation: Op, cancellation: Can) -> Result<T, &'static str>
where
    Op: std::future::Future<Output = Result<T, &'static str>>,
    Can: std::future::Future<Output = ()>,
{
    tokio::select! {
        biased;
        result = operation => result,
        _ = cancellation => Err("capture_cancelled"),
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
async fn finalize_capture<T, Fin, FinFut>(
    result: Result<T, &'static str>,
    finalize: Fin,
) -> Result<T, &'static str>
where
    Fin: FnOnce() -> FinFut,
    FinFut: std::future::Future<Output = Result<(), &'static str>>,
{
    match finalize().await {
        Ok(()) => result,
        Err(_) => Err("browser_cleanup_failed"),
    }
}

async fn capture_inner(
    #[cfg(windows)] profile_path: PathBuf,
    #[cfg(windows)] browser: &mut Browser,
) -> Result<CredentialRecord, &'static str> {
    use tokio::time::sleep;
    #[cfg(not(windows))]
    let temp = tempfile::tempdir().map_err(|_| "browser_launch_failed")?;
    #[cfg(not(windows))]
    let profile_path = temp.path().to_path_buf();
    let port_file = profile_path.join("DevToolsActivePort");
    #[cfg(not(windows))]
    let mut browser = {
        let binary = std::env::var("BINUS_CHROMIUM").unwrap_or_else(|_| "/usr/bin/chromium".into());
        let mut command = Command::new(binary);
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        command
            .args([
                "--remote-debugging-port=0",
                "--remote-debugging-address=127.0.0.1",
                "--no-first-run",
                "--no-default-browser-check",
                "--ozone-platform=wayland",
                "about:blank",
            ])
            .arg(format!("--user-data-dir={}", profile_path.display()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
            .env("XDG_RUNTIME_DIR", "/run/user/1000")
            .env("WAYLAND_DISPLAY", "wayland-1");
        let child = command.spawn().map_err(|_| "browser_launch_failed")?;
        Browser(child)
    };
    while !port_file.exists() {
        if browser
            .try_wait()
            .map_err(|_| "browser_launch_failed")?
            .is_some()
        {
            return Err("browser_launch_failed");
        }
        sleep(Duration::from_millis(100)).await;
    }
    let active = tokio::fs::read_to_string(&port_file)
        .await
        .map_err(|_| "browser_protocol_error")?;
    let port = active
        .lines()
        .next()
        .and_then(|line| line.parse::<u16>().ok())
        .filter(|port| *port != 0)
        .ok_or("browser_protocol_error")?;
    let base = format!("http://127.0.0.1:{port}");
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "browser_protocol_error")?;
    let targets: Value = http
        .get(format!("{base}/json/list"))
        .send()
        .await
        .map_err(|_| "browser_protocol_error")?
        .json()
        .await
        .map_err(|_| "browser_protocol_error")?;
    let target = targets
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|t| t["type"] == "page" && t["url"] == "about:blank")
        })
        .ok_or("browser_protocol_error")?;
    let target_id = target["id"].as_str().ok_or("browser_protocol_error")?;
    let version: Value = http
        .get(format!("{base}/json/version"))
        .send()
        .await
        .map_err(|_| "browser_protocol_error")?
        .json()
        .await
        .map_err(|_| "browser_protocol_error")?;
    let socket = version["webSocketDebuggerUrl"]
        .as_str()
        .ok_or("browser_protocol_error")?;
    let socket_url = url::Url::parse(socket).map_err(|_| "browser_protocol_error")?;
    if socket_url.scheme() != "ws"
        || socket_url.host_str() != Some("127.0.0.1")
        || socket_url.port() != Some(port)
    {
        return Err("browser_protocol_error");
    }
    let (ws, _) = connect_async(socket)
        .await
        .map_err(|_| "browser_protocol_error")?;
    let mut cdp = Cdp {
        ws,
        id: 0,
        session: None,
        guard_enable: HashMap::new(),
        guard_remaining: HashMap::new(),
        pending_acks: HashMap::new(),
        paused_count: 0,
        blocked_count: 0,
    };
    let attached = cdp
        .send(
            "Target.attachToTarget",
            json!({"targetId":target_id,"flatten":true}),
        )
        .await?;
    let page_session = attached["sessionId"]
        .as_str()
        .ok_or("browser_protocol_error")?
        .to_owned();
    cdp.send("Target.setAutoAttach", auto_attach_params())
        .await?;
    cdp.session = Some(page_session);
    cdp.send("Fetch.enable", fetch_params()).await?;
    cdp.send("Target.setAutoAttach", auto_attach_params())
        .await?;
    cdp.send("Page.enable", json!({})).await?;
    cdp.send("Runtime.enable", json!({})).await?;
    cdp.send(
        "Page.navigate",
        json!({"url":"https://lms.binus.ac.id/login"}),
    )
    .await?;
    eprintln!(
        "Chromium opened. Sign in to BINUS and use the portal's Continue to LMS button in this window."
    );
    loop {
        let result = match cdp
            .send(
                "Runtime.evaluate",
                json!({"expression":CAPTURE_EXPRESSION,"returnByValue":true,"awaitPromise":false}),
            )
            .await
        {
            Ok(result) => result,
            Err("evaluation_context_changed") => {
                sleep(Duration::from_millis(100)).await;
                continue;
            }
            Err(error) => return Err(error),
        };
        if let Some(exception) = result.get("exceptionDetails") {
            if !transitional_navigation_exception(exception) {
                return Err("capture_evaluation_failed");
            }
        } else {
            if let Some(record) = capture_evaluate_result("https://lms.binus.ac.id", &result)? {
                cdp.session = None;
                cdp.id += 1;
                let close = json!({"id":cdp.id,"method":"Browser.close","params":{}});
                let _ = cdp.ws.send(Message::Text(close.to_string().into())).await;
                for _ in 0..10 {
                    if browser.try_wait().ok().flatten().is_some() {
                        break;
                    }
                    sleep(Duration::from_millis(100)).await;
                }
                return Ok(record);
            }
        }
        sleep(Duration::from_millis(500)).await;
    }
}

fn transitional_navigation_exception(exception: &Value) -> bool {
    exception
        .to_string()
        .contains("Execution context was destroyed")
        || exception
            .to_string()
            .contains("Cannot find context with specified id")
}

fn capture_evaluate_result(
    origin: &str,
    evaluated: &Value,
) -> Result<Option<CredentialRecord>, &'static str> {
    if let Some(exception) = evaluated.get("exceptionDetails") {
        return if transitional_navigation_exception(exception) {
            Ok(None)
        } else {
            Err("capture_evaluation_failed")
        };
    }
    let remote = &evaluated["result"];
    let value = &remote["value"];
    if !value.is_object() {
        return Ok(None);
    }
    parse_page_state(origin, value).map(Some)
}

pub fn parse_page_state(origin: &str, v: &Value) -> Result<CredentialRecord, &'static str> {
    if origin != "https://lms.binus.ac.id" {
        return Err("invalid_capture");
    }
    let access = v["accessToken"]
        .as_str()
        .filter(|s| valid_token(s))
        .ok_or("invalid_capture")?;
    let refresh = v["refreshToken"]
        .as_str()
        .filter(|s| valid_token(s))
        .ok_or("invalid_capture")?;
    let role = &v["activeRole"];
    let pick = |key: &str| {
        role[key]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .map(str::to_owned)
            .ok_or("invalid_capture")
    };
    let account = v["account"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            let payload = access.split('.').nth(1)?;
            let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(payload)
                .ok()?;
            let claims: Value = serde_json::from_slice(&decoded).ok()?;
            ["email", "upn", "preferred_username"]
                .iter()
                .find_map(|k| claims[*k].as_str().map(str::to_owned))
        })
        .unwrap_or_default();
    Ok(CredentialRecord {
        account,
        access_token: access.to_owned(),
        refresh_token: refresh.to_owned(),
        role: RoleContext {
            academic_career: pick("academicCareer")?,
            institution: pick("institution")?,
            name: pick("name")?,
            role_id: pick("roleId")?,
            organization_id: v["activeRoleOrganizationId"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256)
                .map(str::to_owned)
                .ok_or("invalid_capture")?,
        },
    })
}

fn valid_token(value: &str) -> bool {
    value.len() >= 16 && value.len() <= 32_768 && value.bytes().all(|b| b.is_ascii_graphic())
}

pub fn safe_error(_detail: &str) -> &'static str {
    "request failed"
}

pub fn azure_request_allowed(method: &str, value: &str) -> bool {
    let Ok(u) = url::Url::parse(value) else {
        return false;
    };
    let Some(host) = u.host_str() else {
        return false;
    };
    let oauth_tenant = u
        .path()
        .strip_prefix('/')
        .and_then(|path| path.split_once('/'))
        .is_some_and(|(tenant, suffix)| !tenant.is_empty() && suffix == "oauth2/v2.0/authorize");
    if host == "login.microsoftonline.com"
        && u.scheme() == "https"
        && u.port().is_none()
        && oauth_tenant
        && matches!(method, "GET" | "HEAD")
    {
        return true;
    }
    let source = host == "binus.ac.id"
        || host.ends_with(".binus.ac.id")
        || host == "azure-api.net"
        || host.ends_with(".azure-api.net")
        || host == "azurewebsites.net"
        || host.ends_with(".azurewebsites.net");
    if !source {
        return false;
    }
    if u.scheme() != "https" || u.port().is_some() {
        return false;
    }
    if matches!(method, "GET" | "HEAD" | "OPTIONS") {
        return true;
    }
    method == "POST"
        && ((host == "apim-bm7-prod-web.azure-api.net"
            && [
                "/func-bm7-identity-prod/AccessAndRefreshToken/AzureAD",
                "/func-bm7-identity-prod/AccessAndRefreshToken/Refresh",
            ]
            .contains(&u.path()))
            || (host == "apim-mydashboardbinusmaya-prod-idc.azure-api.net"
                && u.path() == "/func-mydashboardbinusmaya-profile-prod-idc/Person")
            || (host == "apim-bm7-prod-web.azure-api.net"
                && u.path() == "/func-bm7-identity-prod/IsActiveLinkAccountExists"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(windows))]
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    #[tokio::test]
    async fn capture_finalization_covers_results_timeout_and_cancellation() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let calls = Arc::new(AtomicUsize::new(0));
        for expected in [
            Ok::<_, &'static str>("success"),
            Err("capture_protocol_error"),
        ] {
            let result = capture_operation(async move { expected }, std::future::pending()).await;
            let call = calls.clone();
            let result = finalize_capture(result, move || {
                call.fetch_add(1, Ordering::SeqCst);
                async { Ok(()) }
            })
            .await;
            assert_eq!(result, expected);
        }
        let operation = async {
            tokio::time::timeout(
                Duration::ZERO,
                std::future::pending::<Result<(), &'static str>>(),
            )
            .await
            .unwrap_or(Err("capture_timeout"))
        };
        let result = capture_operation(operation, std::future::pending()).await;
        let call = calls.clone();
        assert_eq!(
            finalize_capture(result, move || {
                call.fetch_add(1, Ordering::SeqCst);
                async { Ok(()) }
            })
            .await,
            Err("capture_timeout")
        );

        let result = capture_operation(
            std::future::pending::<Result<(), &'static str>>(),
            std::future::ready(()),
        )
        .await;
        let call = calls.clone();
        assert_eq!(
            finalize_capture(result, move || {
                call.fetch_add(1, Ordering::SeqCst);
                async { Ok::<(), &'static str>(()) }
            })
            .await,
            Err("capture_cancelled")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 4);

        let result =
            capture_operation(async { Ok::<_, &'static str>(()) }, std::future::pending()).await;
        assert_eq!(
            finalize_capture(result, || async { Err("internal_cleanup_detail") }).await,
            Err("browser_cleanup_failed")
        );
        let result = capture_operation(
            async { Err::<(), _>("capture_protocol_error") },
            std::future::pending(),
        )
        .await;
        assert_eq!(
            finalize_capture(result, || async { Err("internal_cleanup_detail") }).await,
            Err("browser_cleanup_failed")
        );
        let timeout = tokio::time::timeout(
            Duration::ZERO,
            std::future::pending::<Result<(), &'static str>>(),
        )
        .await
        .unwrap_or(Err("capture_timeout"));
        let timeout = capture_operation(async move { timeout }, std::future::pending()).await;
        assert_eq!(
            finalize_capture(timeout, || async { Err("internal_cleanup_detail") }).await,
            Err("browser_cleanup_failed")
        );
        let cancelled = capture_operation(
            std::future::pending::<Result<(), &'static str>>(),
            std::future::ready(()),
        )
        .await;
        assert_eq!(
            finalize_capture(cancelled, || async { Err("internal_cleanup_detail") }).await,
            Err("browser_cleanup_failed")
        );
    }

    fn record() -> CredentialRecord {
        CredentialRecord {
            account: "synthetic-student".into(),
            access_token: "synthetic-access-token-value".into(),
            refresh_token: "synthetic-refresh-token-value".into(),
            role: RoleContext {
                academic_career: "UGRD".into(),
                institution: "BINUS".into(),
                name: "Student".into(),
                role_id: "1".into(),
                organization_id: "2".into(),
            },
        }
    }
    fn app_file(temp: &tempfile::TempDir, name: &str) -> PathBuf {
        temp.path().join("binus-auth-spike").join(name)
    }
    #[test]
    fn capture_requires_expected_https_origin_and_path() {
        assert!(trusted_exchange_url(
            "https://apim-bm7-prod-web.azure-api.net/func-bm7-identity-prod/AccessAndRefreshToken/AzureAD"
        ));
        assert!(!trusted_exchange_url(
            "http://apim-bm7-prod-web.azure-api.net/func-bm7-identity-prod/AccessAndRefreshToken/AzureAD"
        ));
        assert!(!trusted_exchange_url(
            "https://evil.example/func-bm7-identity-prod/AccessAndRefreshToken/AzureAD"
        ));
        assert!(!trusted_exchange_url(
            "https://apim-bm7-prod-web.azure-api.net/elsewhere"
        ));
    }
    #[test]
    fn azure_boundary_allows_safe_verbs_and_known_post_routes_only() {
        assert!(azure_request_allowed(
            "GET",
            "https://apim-bm7-prod-web.azure-api.net/any/read"
        ));
        assert!(azure_request_allowed(
            "POST",
            "https://apim-bm7-prod-web.azure-api.net/func-bm7-identity-prod/AccessAndRefreshToken/AzureAD"
        ));
        assert!(azure_request_allowed(
            "POST",
            "https://apim-mydashboardbinusmaya-prod-idc.azure-api.net/func-mydashboardbinusmaya-profile-prod-idc/Person?subscription-key=redacted"
        ));
        assert!(!azure_request_allowed(
            "POST",
            "https://other.azure-api.net/func-mydashboardbinusmaya-profile-prod-idc/Person"
        ));
        assert!(!azure_request_allowed(
            "POST",
            "https://apim-mydashboardbinusmaya-prod-idc.azure-api.net/func-mydashboardbinusmaya-profile-prod-idc/PersonExtra"
        ));
        assert!(!azure_request_allowed(
            "POST",
            "https://apim-mydashboardbinusmaya-prod-idc.azure-api.net/func-mydashboardbinusmaya-profile-prod-idc/Person/SetIsFirstTime"
        ));
        assert!(!azure_request_allowed(
            "POST",
            "https://apim-mydashboardbinusmaya-prod-idc.azure-api.net/func-mydashboardbinusmaya-profile-prod-idc/Person/Save"
        ));
        assert!(!azure_request_allowed(
            "PUT",
            "https://apim-mydashboardbinusmaya-prod-idc.azure-api.net/func-mydashboardbinusmaya-profile-prod-idc/Person"
        ));
        assert!(!azure_request_allowed(
            "DELETE",
            "https://apim-mydashboardbinusmaya-prod-idc.azure-api.net/func-mydashboardbinusmaya-profile-prod-idc/Person"
        ));
        assert!(!azure_request_allowed(
            "POST",
            "https://apim-bm7-prod-web.azure-api.net/func-bm7-course-prod/ToDo/ETextBook/Redeem"
        ));
        assert!(!azure_request_allowed(
            "DELETE",
            "https://apim-bm7-prod-web.azure-api.net/func-bm7-course-prod/anything"
        ));
        assert!(!azure_request_allowed("GET", "https://evil.example/"));
        assert!(!azure_request_allowed(
            "POST",
            "https://unrelated.azure-api.net/func-bm7-identity-prod/AccessAndRefreshToken/AzureAD"
        ));
        assert!(!azure_request_allowed(
            "POST",
            "https://apim-bm7-prod-web.azure-api.net:8443/func-bm7-identity-prod/AccessAndRefreshToken/AzureAD"
        ));
        assert!(!azure_request_allowed(
            "POST",
            "http://apim-bm7-prod-web.azure-api.net/func-bm7-identity-prod/AccessAndRefreshToken/AzureAD"
        ));
        assert!(azure_request_allowed(
            "POST",
            "https://apim-bm7-prod-web.azure-api.net/func-bm7-identity-prod/AccessAndRefreshToken/Refresh"
        ));
        assert!(azure_request_allowed(
            "POST",
            "https://apim-bm7-prod-web.azure-api.net/func-bm7-identity-prod/IsActiveLinkAccountExists"
        ));
        assert!(!azure_request_allowed(
            "POST",
            "https://apim-bm7-prod-web.azure-api.net/func-bm7-course-prod/Forum/LatestPostForumV2"
        ));
        assert!(azure_request_allowed(
            "GET",
            "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?client_id=synthetic"
        ));
        assert!(azure_request_allowed(
            "HEAD",
            "https://login.microsoftonline.com/organizations/oauth2/v2.0/authorize"
        ));
        assert!(azure_request_allowed(
            "GET",
            "https://login.microsoftonline.com/123e4567-e89b-12d3-a456-426614174000/oauth2/v2.0/authorize"
        ));
        for (method, url) in [
            (
                "POST",
                "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            ),
            (
                "OPTIONS",
                "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            ),
            (
                "GET",
                "http://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            ),
            (
                "GET",
                "https://login.microsoftonline.com:8443/common/oauth2/v2.0/authorize",
            ),
            (
                "GET",
                "https://login.microsoftonline.com.evil.example/common/oauth2/v2.0/authorize",
            ),
            (
                "GET",
                "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            ),
            (
                "GET",
                "https://login.microsoftonline.com/common/x/oauth2/v2.0/authorize",
            ),
            (
                "GET",
                "https://login.microsoftonline.com//oauth2/v2.0/authorize",
            ),
            (
                "GET",
                "https://login.microsoftonline.com/common/extra/oauth2/v2.0/authorize",
            ),
        ] {
            assert!(
                !azure_request_allowed(method, url),
                "unexpectedly allowed {method} {url}"
            );
        }
        for (method, url) in [
            (
                "POST",
                "https://apim-bm7-prod-web.azure-api.net/func-bm7-identity-prod/ResetLinkAccountUser",
            ),
            (
                "POST",
                "https://apim-bm7-prod-web.azure-api.net/func-bm7-identity-prod/LinkAccountByToken",
            ),
            (
                "POST",
                "https://apim-bm7-prod-web.azure-api.net/func-bm7-identity-prod/SaveByPassOTP",
            ),
            (
                "POST",
                "https://apim-bm7-prod-web.azure-api.net/func-bm7-identity-prod/IsActiveLinkAccountExistsExtra",
            ),
            (
                "POST",
                "https://other.azure-api.net/func-bm7-identity-prod/IsActiveLinkAccountExists",
            ),
        ] {
            assert!(
                !azure_request_allowed(method, url),
                "unexpectedly allowed {method} {url}"
            );
        }
        assert!(!azure_request_allowed(
            "PATCH",
            "https://apim-bm7-prod-web.azure-api.net/func-bm7-identity-prod/AccessAndRefreshToken/AzureAD"
        ));
        assert!(!azure_request_allowed(
            "POST",
            "https://apim-bm7-prod-web.azure-api.net/func-bm7-forum-prod/Forum/LatestPostForumV2"
        ));
        assert!(!azure_request_allowed(
            "POST",
            "https://api.azurewebsites.net/write"
        ));
        assert!(!azure_request_allowed(
            "POST",
            "https://binus.ac.id/apps/write"
        ));
    }
    #[test]
    fn token_pair_parser_rejects_missing_malformed_and_short_tokens() {
        assert!(parse_pair(r#"{"accessToken":"synthetic-access-token-value","refreshToken":"synthetic-refresh-token-value"}"#).is_ok());
        assert!(
            parse_pair(r#"{"accessToken":"","refreshToken":"synthetic-refresh-token-value"}"#)
                .is_err()
        );
        assert!(parse_pair(r#"{"accessToken":"short","refreshToken":"short"}"#).is_err());
        assert!(parse_pair("not-json").is_err());
    }
    #[test]
    fn page_state_requires_exact_origin_and_complete_role_fields() {
        let value = json!({"accessToken":"synthetic-access-token-value","refreshToken":"synthetic-refresh-token-value","activeRole":{"academicCareer":"UGRD","institution":"BINUS","name":"Student","roleId":"1"},"activeRoleOrganizationId":"2"});
        assert_eq!(
            parse_page_state("https://evil.example", &value).unwrap_err(),
            "invalid_capture"
        );
        assert!(parse_page_state("https://lms.binus.ac.id", &value).is_ok());
        assert!(
            parse_page_state(
                "https://lms.binus.ac.id",
                &json!({"accessToken":"synthetic-access-token-value"})
            )
            .is_err()
        );
    }
    #[test]
    fn runtime_evaluate_result_envelope_yields_credential_record() {
        let evaluated = json!({"result":{"type":"object","value":{"accessToken":"synthetic-access-token-value","refreshToken":"synthetic-refresh-token-value","activeRole":{"academicCareer":"UGRD","institution":"BINUS","name":"Student","roleId":"1"},"activeRoleOrganizationId":"2","account":"synthetic-student"}}});
        let record = capture_evaluate_result("https://lms.binus.ac.id", &evaluated)
            .unwrap()
            .unwrap();
        assert_eq!(record.account, "synthetic-student");
        assert_eq!(record.role.organization_id, "2");
    }
    #[test]
    fn production_capture_expression_waits_for_loaded_cached_store() {
        let harness = r#"
const vm = require('node:vm');
const expression = process.argv[1];
let requireCalls = 0;
const runtime = function () { requireCalls++; throw new Error('module factory invoked'); };
runtime.c = {};
function run(extra = {}) {
  const context = { location: { origin: 'https://lms.binus.ac.id' }, window: { 'webpackJsonpbinus-lms-web': { push() {} }, __binusResearchRequire: runtime }, ...extra };
  return vm.runInNewContext(expression, context);
}
if (run() !== null || requireCalls !== 0) throw new Error('empty cache was not passive');
runtime.c[34] = { l: false, exports: {} };
if (run() !== null || requireCalls !== 0) throw new Error('partial cache was not passive');
runtime.c[34] = { l: true, exports: { b: { getState: () => ({}) } } };
if (run() !== null || requireCalls !== 0) throw new Error('missing auth was not passive');
runtime.c[34].exports.b.getState = () => ({ auth: { res: { token: 'synthetic-access-token-value', refreshToken: 'synthetic-refresh-token-value', activeRole: { academicCareer: 'UGRD', institution: 'BINUS', name: 'Student', roleId: '1' }, activeRoleOrganizationId: '2', email: 'synthetic-student' } } });
const ready = run();
if (requireCalls !== 0 || ready.accessToken !== 'synthetic-access-token-value') throw new Error('ready store not captured');
let chunkTouches = 0;
const outside = vm.runInNewContext(expression, { location: { origin: 'https://login.microsoftonline.com' }, window: { get webpackJsonpbinus_lms_web() { chunkTouches++; throw new Error('webpack touched'); } } });
if (outside !== null || chunkTouches !== 0) throw new Error('origin gate failed');
runtime.c[34].exports.b.getState = () => { throw new Error('unexpected failure'); };
let terminal = false;
try { run(); } catch (error) { terminal = error.message === 'unexpected failure'; }
if (!terminal) throw new Error('unexpected exception was swallowed');
process.stdout.write(JSON.stringify(ready));
"#;
        let output = Command::new("node")
            .arg("-e")
            .arg(harness)
            .arg(CAPTURE_EXPRESSION)
            .output()
            .expect("node is required for the production-expression regression");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ready: Value = serde_json::from_slice(&output.stdout).unwrap();
        let record = parse_page_state("https://lms.binus.ac.id", &ready).unwrap();
        assert_eq!(record.account, "synthetic-student");
        assert_eq!(record.role.organization_id, "2");
    }
    #[test]
    #[cfg(not(windows))]
    fn atomic_store_is_private_and_json_is_secret_free() {
        let temp = tempfile::tempdir().unwrap();
        let path = app_file(&temp, "credentials.json");
        store_record(&path, &record()).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let output = sanitized_status(true, false, None);
        let json = serde_json::to_string(&output).unwrap();
        for secret in [
            "synthetic-access-token-value",
            "synthetic-refresh-token-value",
        ] {
            assert!(!json.contains(secret));
        }
        assert!(load_record(&path).is_ok());
    }

    #[test]
    #[cfg(not(windows))]
    fn store_changes_only_the_app_directory_permissions() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("config");
        fs::create_dir(&config).unwrap();
        fs::set_permissions(&config, fs::Permissions::from_mode(0o750)).unwrap();
        let app = config.join("binus-auth-spike");
        let path = app.join("credentials.json");
        store_record(&path, &record()).unwrap();
        assert_eq!(
            fs::metadata(&config).unwrap().permissions().mode() & 0o777,
            0o750
        );
        assert_eq!(
            fs::metadata(&app).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn missing_record_and_metadata_read_error_are_distinct() {
        let temp = tempfile::tempdir().unwrap();
        let path = app_file(&temp, "missing.json");
        assert!(matches!(load_record(&path), Err("not_logged_in")));
        let blocked = temp.path().join("binus-auth-spike");
        fs::write(&blocked, b"not a directory").unwrap();
        assert!(matches!(
            load_record(&blocked.join("credentials.json")),
            Err("storage_error")
        ));
    }

    #[test]
    fn academic_period_request_uses_headers_without_secret_query_or_cookies() {
        let record = record();
        let request = academic_period_request(&record).unwrap();
        assert_eq!(
            request.url().as_str(),
            "https://apim-bm7-prod-web.azure-api.net/func-bm7-course-prod/AcademicPeriod/Student"
        );
        assert!(request.url().query().is_none());
        assert_eq!(
            request.headers()[reqwest::header::AUTHORIZATION],
            "Bearer synthetic-access-token-value"
        );
        for (name, value) in [
            ("rOId", "2"),
            ("academicCareer", "UGRD"),
            ("institution", "BINUS"),
            ("roleName", "Student"),
            ("roleId", "1"),
        ] {
            assert_eq!(request.headers().get(name).unwrap(), value);
        }
        assert!(!request.headers().contains_key(reqwest::header::COOKIE));
    }
    #[test]
    fn status_and_errors_do_not_debug_secret_records() {
        let rendered = format!("{:?}", record());
        assert!(!rendered.contains("synthetic-access-token-value"));
        assert!(!rendered.contains("synthetic-student"));
        assert!(!rendered.contains("UGRD"));
        assert!(!rendered.contains("BINUS"));
        assert_eq!(
            safe_error("request failed at https://example/?subscription-key=secret&token=abc"),
            "request failed"
        );
    }
    #[test]
    #[cfg(not(windows))]
    fn rejects_symlinked_record() {
        let temp = tempfile::tempdir().unwrap();
        let path = app_file(&temp, "credentials.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
        let other = temp.path().join("elsewhere");
        fs::write(&other, b"{}").unwrap();
        std::os::unix::fs::symlink(other, &path).unwrap();
        assert_eq!(store_record(&path, &record()), Err("storage_error"));
        assert!(matches!(load_record(&path), Err("storage_error")));
    }

    #[test]
    #[cfg(not(windows))]
    fn secure_store_rejects_symlinked_ancestor_without_touching_target() {
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        let real_app = real.join("binus-auth-spike");
        fs::create_dir_all(&real_app).unwrap();
        fs::set_permissions(&real_app, fs::Permissions::from_mode(0o755)).unwrap();
        let before = fs::metadata(&real_app).unwrap().permissions().mode() & 0o777;
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        assert_eq!(
            store_record(
                &alias.join("binus-auth-spike").join("credentials.json"),
                &record()
            ),
            Err("storage_error")
        );
        assert_eq!(
            fs::metadata(&real_app).unwrap().permissions().mode() & 0o777,
            before
        );
        assert!(!real_app.join("credentials.json").exists());
    }

    #[test]
    #[cfg(not(windows))]
    fn malicious_temp_and_atomic_failure_preserve_primary_and_sentinel() {
        let temp = tempfile::tempdir().unwrap();
        let primary = app_file(&temp, "credentials.json");
        store_record(&primary, &record()).unwrap();
        let original = load_record(&primary).unwrap();
        let sentinel = temp.path().join("sentinel");
        fs::write(&sentinel, b"synthetic-safe").unwrap();
        let temp_name = format!("credentials.tmp-{}", std::process::id());
        std::os::unix::fs::symlink(&sentinel, primary.parent().unwrap().join(&temp_name)).unwrap();
        let replacement = CredentialRecord {
            account: "synthetic-new".into(),
            ..record()
        };
        assert_eq!(store_record(&primary, &replacement), Err("storage_error"));
        assert_eq!(load_record(&primary).unwrap().account, original.account);
        assert_eq!(fs::read(&sentinel).unwrap(), b"synthetic-safe");
    }

    #[test]
    #[cfg(not(windows))]
    fn logout_attempts_all_owned_artifacts_and_leaves_unrelated_files() {
        let temp = tempfile::tempdir().unwrap();
        let primary = app_file(&temp, "credentials.json");
        let candidate = app_file(&temp, "credentials.captured.json");
        store_record(&candidate, &record()).unwrap();
        fs::write(&primary, b"synthetic").unwrap();
        fs::set_permissions(&primary, fs::Permissions::from_mode(0o640)).unwrap();
        let residue = app_file(&temp, "credentials.tmp-123456");
        fs::write(&residue, b"synthetic").unwrap();
        fs::set_permissions(&residue, fs::Permissions::from_mode(0o600)).unwrap();
        let captured_residue = app_file(&temp, "credentials.captured.tmp-123456");
        fs::write(&captured_residue, b"synthetic").unwrap();
        fs::set_permissions(&captured_residue, fs::Permissions::from_mode(0o600)).unwrap();
        let unrelated = app_file(&temp, "notes.txt");
        fs::write(&unrelated, b"keep").unwrap();
        assert_eq!(remove_spike_artifacts(&primary), Err("storage_error"));
        assert!(primary.exists());
        assert!(!candidate.exists());
        assert!(!residue.exists());
        assert!(!captured_residue.exists());
        assert_eq!(fs::read(unrelated).unwrap(), b"keep");
    }

    #[test]
    fn candidate_remains_recoverable_until_verified_promotion_succeeds() {
        let temp = tempfile::tempdir().unwrap();
        let primary = app_file(&temp, "credentials.json");
        let candidate = app_file(&temp, "credentials.captured.json");
        store_record(&primary, &record()).unwrap();
        let newer = CredentialRecord {
            account: "synthetic-new".into(),
            ..record()
        };
        store_record(&candidate, &newer).unwrap();
        let (preferred, pending) = load_preferred_record(&primary).unwrap();
        assert!(pending);
        assert_eq!(preferred.account, "synthetic-new");
        let failed_verification = CredentialRecord {
            account: "failed-verification".into(),
            ..record()
        };
        assert_eq!(
            promote_verified_candidate(&primary, &candidate, &failed_verification),
            Err("candidate_changed")
        );
        assert_eq!(load_record(&primary).unwrap().account, "synthetic-student");
        assert_eq!(load_record(&candidate).unwrap().account, "synthetic-new");
        assert!(promote_verified_candidate(&primary, &candidate, &newer).unwrap());
        assert_eq!(load_record(&primary).unwrap().account, "synthetic-new");
        assert!(matches!(load_record(&candidate), Err("not_logged_in")));
    }

    #[test]
    fn auto_attach_includes_iframes() {
        assert!(
            auto_attach_params()["filter"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["type"] == "iframe")
        );
    }

    #[test]
    fn promotion_is_bound_to_verified_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let primary = app_file(&temp, "credentials.json");
        let candidate = app_file(&temp, "credentials.captured.json");
        store_record(&primary, &record()).unwrap();
        let verified = CredentialRecord {
            account: "verified-A".into(),
            ..record()
        };
        let newer = CredentialRecord {
            account: "unverified-B".into(),
            ..record()
        };
        store_record(&candidate, &verified).unwrap();
        store_record(&candidate, &newer).unwrap();
        assert_eq!(
            promote_verified_candidate(&primary, &candidate, &verified),
            Err("candidate_changed")
        );
        assert_eq!(load_record(&primary).unwrap().account, "synthetic-student");
        assert_eq!(load_record(&candidate).unwrap().account, "unverified-B");
        store_record(&candidate, &verified).unwrap();
        assert!(promote_verified_candidate(&primary, &candidate, &verified).unwrap());
        assert_eq!(load_record(&primary).unwrap().account, "verified-A");
    }

    #[test]
    #[cfg(not(windows))]
    fn failed_candidate_promotion_keeps_candidate_and_existing_primary_target() {
        let temp = tempfile::tempdir().unwrap();
        let primary = app_file(&temp, "credentials.json");
        let candidate = app_file(&temp, "credentials.captured.json");
        fs::create_dir_all(primary.parent().unwrap()).unwrap();
        fs::set_permissions(primary.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
        let sentinel = app_file(&temp, "sentinel");
        fs::write(&sentinel, b"synthetic-safe").unwrap();
        std::os::unix::fs::symlink(&sentinel, &primary).unwrap();
        store_record(&candidate, &record()).unwrap();
        assert_eq!(
            promote_verified_candidate(&primary, &candidate, &record()),
            Err("storage_error")
        );
        assert_eq!(fs::read(sentinel).unwrap(), b"synthetic-safe");
        assert!(load_record(&candidate).is_ok());
    }

    #[tokio::test]
    async fn cdp_worker_page_guard_is_acknowledged_before_resume() {
        use tokio::net::TcpListener;
        use tokio_tungstenite::accept_async;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let initial = ws.next().await.unwrap().unwrap().into_text().unwrap();
            for (i, (session, kind)) in [
                ("page-1", "page"),
                ("worker-1", "worker"),
                ("sw-1", "service_worker"),
            ]
            .iter()
            .enumerate()
            {
                ws.send(Message::Text(json!({"method":"Target.attachedToTarget","params":{"sessionId":session,"targetInfo":{"type":kind},"waitingForDebugger":true}}).to_string().into())).await.unwrap();
                let first: Value =
                    serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                        .unwrap();
                let second: Value =
                    serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                        .unwrap();
                let guards = [first, second];
                assert!(guards.iter().any(|g| g["method"] == "Fetch.enable"));
                assert!(guards.iter().any(|g| g["method"] == "Target.setAutoAttach"));
                for guard in &guards {
                    assert_eq!(guard["sessionId"], *session);
                    if guard["method"] == "Fetch.enable" {
                        assert!(guard["params"]["patterns"].as_array().unwrap().len() >= 6);
                    }
                }
                ws.send(Message::Text(
                    json!({"id":guards[0]["id"],"sessionId":session,"result":{}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
                assert!(
                    tokio::time::timeout(Duration::from_millis(15), ws.next())
                        .await
                        .is_err(),
                    "target resumed before every guard acknowledged"
                );
                ws.send(Message::Text(
                    json!({"id":guards[1]["id"],"sessionId":session,"result":{}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
                let resume: Value =
                    serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                        .unwrap();
                assert_eq!(resume["method"], "Runtime.runIfWaitingForDebugger");
                assert_eq!(resume["sessionId"], *session);
                if i == 2 {
                    let id = serde_json::from_str::<Value>(&initial).unwrap()["id"].clone();
                    ws.send(Message::Text(
                        json!({"id":id,"result":{"ready":true}}).to_string().into(),
                    ))
                    .await
                    .unwrap();
                }
            }
        });
        let (ws, _) = connect_async(format!("ws://{addr}")).await.unwrap();
        let mut client = Cdp {
            ws,
            id: 0,
            session: None,
            guard_enable: HashMap::new(),
            guard_remaining: HashMap::new(),
            pending_acks: HashMap::new(),
            paused_count: 0,
            blocked_count: 0,
        };
        assert_eq!(
            client
                .send("Target.setAutoAttach", json!({}))
                .await
                .unwrap()["ready"],
            true
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn cdp_setup_command_stall_is_bounded_by_outer_deadline() {
        use tokio::net::TcpListener;
        use tokio_tungstenite::accept_async;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let _ = ws.next().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        let (ws, _) = connect_async(format!("ws://{addr}")).await.unwrap();
        let mut client = Cdp {
            ws,
            id: 0,
            session: None,
            guard_enable: HashMap::new(),
            guard_remaining: HashMap::new(),
            pending_acks: HashMap::new(),
            paused_count: 0,
            blocked_count: 0,
        };
        assert!(
            tokio::time::timeout(
                Duration::from_millis(10),
                client.send("Target.setAutoAttach", json!({}))
            )
            .await
            .is_err()
        );
        server.abort();
    }

    #[test]
    #[cfg(not(windows))]
    fn browser_drop_kills_only_its_process_group_before_profile_removal() {
        use std::os::unix::process::CommandExt;
        let profile = tempfile::tempdir().unwrap();
        let profile_path = profile.path().to_path_buf();
        let marker = profile.path().join("late-marker");
        let mut command = Command::new("/bin/sh");
        command
            .process_group(0)
            .arg("-c")
            .arg(format!("sleep 0.5; touch {}", marker.display()));
        let child = command.spawn().unwrap();
        drop(Browser(child));
        std::thread::sleep(Duration::from_millis(600));
        assert!(!marker.exists());
        drop(profile);
        assert!(!profile_path.exists());
    }

    #[tokio::test]
    async fn synthetic_cdp_fixture_routes_fetch_event_and_command_reply() {
        use tokio::net::TcpListener;
        use tokio_tungstenite::accept_async;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let initial: Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            ws.send(Message::Text(json!({"method":"Target.attachedToTarget","params":{"sessionId":"iframe-session","targetInfo":{"type":"iframe"},"waitingForDebugger":true}}).to_string().into())).await.unwrap();
            for _ in 0..2 {
                let guard: Value =
                    serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                        .unwrap();
                assert_eq!(guard["sessionId"], "iframe-session");
                ws.send(Message::Text(
                    json!({"id":guard["id"],"sessionId":"iframe-session","result":{}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            }
            let resume: Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            assert_eq!(resume["method"], "Runtime.runIfWaitingForDebugger");
            ws.send(Message::Text(json!({"method":"Fetch.requestPaused","sessionId":"iframe-session","params":{"requestId":"fixture","request":{"url":"https://apim-bm7-prod-web.azure-api.net/unsafe","method":"POST"}}}).to_string().into())).await.unwrap();
            let blocked: Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            assert_eq!(blocked["method"], "Fetch.failRequest");
            assert_eq!(blocked["sessionId"], "iframe-session");
            ws.send(Message::Text(
                json!({"id":blocked["id"],"sessionId":"iframe-session","result":{}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            ws.send(Message::Text(json!({"method":"Fetch.requestPaused","sessionId":"iframe-session","params":{"requestId":"oauth-redirect","request":{"url":"https://login.microsoftonline.com/tenant-guid/oauth2/v2.0/authorize?redirect_uri=https%3A%2F%2Flms.binus.ac.id%2Flogin","method":"GET"}}}).to_string().into())).await.unwrap();
            let oauth: Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            assert_eq!(oauth["method"], "Fetch.continueRequest");
            assert_eq!(oauth["sessionId"], "iframe-session");
            ws.send(Message::Text(
                json!({"id":oauth["id"],"sessionId":"iframe-session","result":{}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            ws.send(Message::Text(json!({"id":initial["id"],"result":{"result":{"type":"object","value":{"accessToken":"synthetic-access-token-value","refreshToken":"synthetic-refresh-token-value","activeRole":{"academicCareer":"UGRD","institution":"BINUS","name":"Student","roleId":"1"},"activeRoleOrganizationId":"2","account":"synthetic-student"}}}}).to_string().into()))
            .await
            .unwrap();
        });
        let (ws, _) = connect_async(format!("ws://{addr}")).await.unwrap();
        let mut client = Cdp {
            ws,
            id: 0,
            session: None,
            guard_enable: HashMap::new(),
            guard_remaining: HashMap::new(),
            pending_acks: HashMap::new(),
            paused_count: 0,
            blocked_count: 0,
        };
        let reply = client.send("Runtime.evaluate", json!({})).await.unwrap();
        let record = capture_evaluate_result("https://lms.binus.ac.id", &reply)
            .unwrap()
            .unwrap();
        assert_eq!(record.account, "synthetic-student");
        assert_eq!(client.blocked_count, 1);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn runtime_evaluate_retries_top_level_navigation_context_error() {
        use tokio::net::TcpListener;
        use tokio_tungstenite::accept_async;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let first: Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            ws.send(Message::Text(json!({"id":first["id"],"result":{},"error":{"message":"Execution context was destroyed."}}).to_string().into())).await.unwrap();
            let second: Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            assert!(second["id"].as_u64().unwrap() > first["id"].as_u64().unwrap());
            ws.send(Message::Text(json!({"id":second["id"],"result":{"result":{"type":"object","value":{"accessToken":"synthetic-access-token-value","refreshToken":"synthetic-refresh-token-value","activeRole":{"academicCareer":"UGRD","institution":"BINUS","name":"Student","roleId":"1"},"activeRoleOrganizationId":"2","account":"synthetic-student"}}}}).to_string().into())).await.unwrap();
        });
        let (ws, _) = connect_async(format!("ws://{addr}")).await.unwrap();
        let mut client = Cdp {
            ws,
            id: 0,
            session: None,
            guard_enable: HashMap::new(),
            guard_remaining: HashMap::new(),
            pending_acks: HashMap::new(),
            paused_count: 0,
            blocked_count: 0,
        };
        assert_eq!(
            client.send("Runtime.evaluate", json!({})).await,
            Err("evaluation_context_changed")
        );
        let reply = client.send("Runtime.evaluate", json!({})).await.unwrap();
        let record = capture_evaluate_result("https://lms.binus.ac.id", &reply)
            .unwrap()
            .unwrap();
        assert_eq!(record.account, "synthetic-student");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn fatal_runtime_evaluate_protocol_error_is_terminal() {
        use tokio::net::TcpListener;
        use tokio_tungstenite::accept_async;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let command: Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            ws.send(Message::Text(
                json!({"id":command["id"],"error":{"message":"synthetic fatal protocol failure"}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        });
        let (ws, _) = connect_async(format!("ws://{addr}")).await.unwrap();
        let mut client = Cdp {
            ws,
            id: 0,
            session: None,
            guard_enable: HashMap::new(),
            guard_remaining: HashMap::new(),
            pending_acks: HashMap::new(),
            paused_count: 0,
            blocked_count: 0,
        };
        assert_eq!(
            client.send("Runtime.evaluate", json!({})).await,
            Err("browser_protocol_error")
        );
        server.await.unwrap();
    }
}
