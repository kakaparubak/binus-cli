use crate::{CredentialRecord, academic_period_request};
use html5ever::{ParseOpts, parse_document, tendril::TendrilSink, tree_builder::TreeBuilderOpts};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use reqwest::{Method, redirect::Policy};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use url::Url;

const ORIGIN: &str = "https://apim-bm7-prod-web.azure-api.net";
const OP: Duration = Duration::from_secs(60);
const REQ: Duration = Duration::from_secs(20);
const CAP: usize = 8 * 1024 * 1024;
const PAGE_SIZE: u8 = 20;
const MAX_PAGES: u8 = 100;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ThreadRow {
    class_id: String,
    session_id: String,
    thread_id: String,
    title: String,
    post_date: String,
    post_date_utc: String,
    date_end: Option<String>,
    source_date_end: String,
    status: String,
    masked: bool,
    locked: bool,
    assessment: bool,
    total_posts: i64,
    total_comments: i64,
    author_role: String,
}
#[derive(Serialize)]
struct Coverage {
    scope: &'static str,
    complete: bool,
    pages_fetched: u8,
    declared_total: Option<usize>,
}
#[derive(Serialize)]
struct ApiError {
    code: &'static str,
    stage: &'static str,
    page: Option<u8>,
    http_status: Option<u16>,
}
#[derive(Serialize)]
pub struct Envelope<T> {
    schema_version: &'static str,
    status: &'static str,
    data: Vec<T>,
    coverage: Coverage,
    errors: Vec<ApiError>,
    warnings: Vec<String>,
    fetched_at: String,
}
fn now() -> String {
    crate::courses::timestamp()
}
fn envelope<T>(
    status: &'static str,
    data: Vec<T>,
    pages: u8,
    total: Option<usize>,
    errors: Vec<ApiError>,
    warnings: Vec<String>,
) -> Envelope<T> {
    Envelope {
        schema_version: "1",
        status,
        data,
        coverage: Coverage {
            scope: "selected_session",
            complete: status == "complete",
            pages_fetched: pages,
            declared_total: total,
        },
        errors,
        warnings,
        fetched_at: now(),
    }
}
fn err(code: &'static str, stage: &'static str, page: Option<u8>, status: Option<u16>) -> ApiError {
    ApiError {
        code,
        stage,
        page,
        http_status: status,
    }
}
#[cfg(test)]
#[derive(Clone)]
struct Target {
    origin: String,
    total: Duration,
    request: Duration,
    cap: usize,
}
#[derive(Clone)]
enum Dest {
    Production,
    #[cfg(test)]
    Local(Target),
}
fn make_request(
    record: &CredentialRecord,
    method: Method,
    segs: &[&str],
    body: bool,
    dest: &Dest,
) -> Result<reqwest::Request, &'static str> {
    let mut r = academic_period_request(record).map_err(|_| "request_failed")?;
    let origin = match dest {
        Dest::Production => ORIGIN,
        #[cfg(test)]
        Dest::Local(t) => {
            let u = Url::parse(&t.origin).map_err(|_| "request_failed")?;
            if !matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "::1")) {
                return Err("request_failed");
            }
            &t.origin
        }
    };
    *r.url_mut() = Url::parse(&format!("{origin}/")).map_err(|_| "request_failed")?;
    let mut p = r
        .url_mut()
        .path_segments_mut()
        .map_err(|_| "request_failed")?;
    p.clear();
    for s in segs {
        p.push(s);
    }
    drop(p);
    *r.method_mut() = method.clone();
    r.headers_mut().remove(reqwest::header::COOKIE);
    if body {
        *r.body_mut() = Some(json!({"TotalDataPerPage":PAGE_SIZE}).to_string().into());
        r.headers_mut().insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().map_err(|_| "request_failed")?,
        );
    }
    Ok(r)
}
async fn request_json(
    _record: &CredentialRecord,
    request: reqwest::Request,
    dest: &Dest,
    deadline: Instant,
) -> Result<Value, (&'static str, Option<u16>)> {
    let (total, per, cap) = match dest {
        Dest::Production => (OP, REQ, CAP),
        #[cfg(test)]
        Dest::Local(t) => (t.total, t.request, t.cap),
    };
    let remain = deadline.saturating_duration_since(Instant::now());
    if remain.is_zero() {
        return Err(("timeout", None));
    }
    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(per.min(remain))
        .build()
        .map_err(|_| ("request_failed", None))?;
    let fut = async {
        let resp = client.execute(request).await.map_err(|e| {
            if e.is_timeout() {
                ("timeout", None)
            } else {
                ("network_error", None)
            }
        })?;
        let code = resp.status().as_u16();
        if code == 401 || code == 403 {
            return Err(("authorization_required", Some(code)));
        }
        if !(200..300).contains(&code) {
            return Err(("upstream_error", Some(code)));
        }
        let mut stream = resp.bytes_stream();
        let mut bytes = Vec::new();
        use futures_util::StreamExt;
        while let Some(ch) = stream.next().await {
            let b = ch.map_err(|e| {
                if e.is_timeout() {
                    ("timeout", None)
                } else {
                    ("network_error", None)
                }
            })?;
            if bytes.len().saturating_add(b.len()) > cap {
                return Err(("response_too_large", None));
            }
            bytes.extend_from_slice(&b);
        }
        serde_json::from_slice(&bytes).map_err(|_| ("invalid_json", None))
    };
    tokio::time::timeout(total.min(remain), fut)
        .await
        .map_err(|_| ("timeout", None))?
}
fn strf<'a>(v: &'a Value, k: &str) -> Result<&'a str, &'static str> {
    v.get(k).and_then(Value::as_str).ok_or("invalid_schema")
}
fn boolf(v: &Value, k: &str) -> Result<bool, &'static str> {
    v.get(k).and_then(Value::as_bool).ok_or("invalid_schema")
}
fn intf(v: &Value, k: &str) -> Result<i64, &'static str> {
    v.get(k).and_then(Value::as_i64).ok_or("invalid_schema")
}
fn row(v: &Value, class: &str, session: &str) -> Result<ThreadRow, &'static str> {
    let end = strf(v, "dateEnd")?.to_owned();
    Ok(ThreadRow {
        class_id: class.into(),
        session_id: session.into(),
        thread_id: strf(v, "threadId")?.into(),
        title: strf(v, "threadTitle")?.into(),
        post_date: strf(v, "postDate")?.into(),
        post_date_utc: strf(v, "postDateUtc")?.into(),
        date_end: (!end.starts_with("0001-01-01")).then(|| end.clone()),
        source_date_end: end,
        status: strf(v, "status")?.into(),
        masked: boolf(v, "isMasked")?,
        locked: boolf(v, "isLocked")?,
        assessment: boolf(v, "isAssessment")?,
        total_posts: intf(v, "totalPost")?,
        total_comments: intf(v, "totalComment")?,
        author_role: strf(v, "authorRole")?.into(),
    })
}
fn continuation(root: &Value) -> Result<(), &'static str> {
    fn scan(v: &Value) -> Result<(), &'static str> {
        if let Some(m) = v.as_object() {
            for (k, x) in m {
                match k.as_str() {
                    "continuationToken" | "nextToken" | "nextCursor" => {
                        if !x.is_null() && !x.as_str().is_some_and(str::is_empty) {
                            return Err("unsupported_pagination");
                        }
                    }
                    "nextPage" => {
                        if !x.is_null() && !x.as_u64().is_some_and(|n| n == 0) {
                            return Err("unsupported_pagination");
                        }
                    }
                    "hasNext" | "hasMore" => match x.as_bool() {
                        Some(false) => {}
                        Some(true) => return Err("unsupported_pagination"),
                        None => return Err("invalid_schema"),
                    },
                    "pagination" | "paging" if !x.is_null() => {
                        if !x.is_object() {
                            return Err("invalid_schema");
                        }
                        scan(x)?
                    }
                    "pagination" | "paging" => {}
                    _ => {}
                }
            }
        }
        Ok(())
    }
    scan(root)
}
fn failure(
    data: Vec<ThreadRow>,
    pages: u8,
    total: Option<usize>,
    code: &'static str,
    page: u8,
    status: Option<u16>,
) -> Envelope<ThreadRow> {
    let state = if data.is_empty() { "error" } else { "partial" };
    envelope(
        state,
        data,
        pages,
        total,
        vec![err(code, "list", Some(page), status)],
        vec![],
    )
}
pub async fn list(record: &CredentialRecord, class: &str, session: &str) -> Envelope<ThreadRow> {
    list_inner(record, class, session, Dest::Production).await
}
async fn list_inner(
    record: &CredentialRecord,
    class: &str,
    session: &str,
    dest: Dest,
) -> Envelope<ThreadRow> {
    if !crate::courses::safe_segment(class) || !crate::courses::safe_segment(session) {
        return envelope(
            "error",
            vec![],
            0,
            None,
            vec![err("unsafe_identifier", "request", None, None)],
            vec![],
        );
    }
    let operation_budget = match &dest {
        Dest::Production => OP,
        #[cfg(test)]
        Dest::Local(t) => t.total,
    };
    let deadline = Instant::now() + operation_budget;
    let mut all = Vec::new();
    let mut seen: HashMap<String, Value> = HashMap::new();
    let mut total = None;
    let mut pages = 0;
    let mut zero_based = None;
    for page in 1..=MAX_PAGES {
        if Instant::now() >= deadline {
            return failure(all, pages, total, "timeout", page, None);
        }
        let path = [
            "func-bm7-forum-prod",
            "Thread",
            "Class",
            class,
            "Session",
            session,
            "Paging",
            &page.to_string(),
        ];
        let req = match make_request(record, Method::POST, &path, true, &dest) {
            Ok(r) => r,
            Err(c) => return failure(all, pages, total, c, page, None),
        };
        let root = match request_json(record, req, &dest, deadline).await {
            Ok(v) => v,
            Err((c, s)) => return failure(all, pages, total, c, page, s),
        };
        if let Err(c) = continuation(&root) {
            return failure(all, pages, total, c, page, None);
        }
        let Some(rows) = root.get("datas").and_then(Value::as_array) else {
            return failure(all, pages, total, "invalid_schema", page, None);
        };
        let Some(n) = root
            .get("totalData")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
        else {
            return failure(all, pages, total, "invalid_schema", page, None);
        };
        let Some(index) = root.get("index").and_then(Value::as_u64) else {
            return failure(all, pages, total, "invalid_schema", page, None);
        };
        let this_zero_based = index == page as u64 - 1;
        if (!this_zero_based && index != page as u64)
            || zero_based.is_some_and(|expected| expected != this_zero_based)
        {
            return failure(all, pages, total, "pagination_mismatch", page, None);
        }
        if total.is_some_and(|x| x != n) {
            return failure(all, pages, total, "total_changed", page, None);
        }
        total = Some(n);
        zero_based = Some(this_zero_based);
        let mut page_seen = HashMap::<String, Value>::new();
        let mut mapped = Vec::new();
        for raw in rows {
            let Ok(r) = row(raw, class, session) else {
                return failure(all, pages, total, "invalid_schema", page, None);
            };
            if let Some(prev) = page_seen
                .get(&r.thread_id)
                .or_else(|| seen.get(&r.thread_id))
            {
                if prev != raw {
                    return failure(all, pages, total, "conflicting_duplicate", page, None);
                }
            } else {
                page_seen.insert(r.thread_id.clone(), raw.clone());
                mapped.push((r, raw.clone()));
            }
        }
        if rows.len() > PAGE_SIZE as usize || (rows.is_empty() && all.len() < n && n > 0) {
            return failure(all, pages, total, "pagination_mismatch", page, None);
        }
        if mapped.is_empty() && all.len() < n {
            return failure(all, pages, total, "nonprogress", page, None);
        }
        for (r, raw) in mapped {
            seen.insert(r.thread_id.clone(), raw);
            all.push(r)
        }
        pages += 1;
        if all.len() >= n {
            if all.len() != n {
                return failure(all, pages, total, "row_count_mismatch", page, None);
            }
            return envelope("complete", all, pages, total, vec![], vec![]);
        }
        if page == MAX_PAGES {
            return failure(all, pages, total, "page_limit", page, None);
        }
    }
    unreachable!()
}

#[derive(Serialize)]
pub struct Attachment {
    source_position: usize,
    name: String,
    kind: String,
    size_bytes: u64,
    has_link: bool,
}
#[derive(Serialize)]
pub struct ThreadDetail {
    class_id: String,
    thread_id: String,
    title: String,
    content_text: String,
    content_redacted: bool,
    post_date: String,
    post_date_utc: String,
    date_end: Option<String>,
    source_date_end: String,
    status: String,
    masked: bool,
    assessment: bool,
    session_number: u64,
    attachments: Vec<Attachment>,
}
#[derive(Serialize)]
pub struct ShowEnvelope {
    schema_version: &'static str,
    status: &'static str,
    data: Option<ThreadDetail>,
    coverage: ShowCoverage,
    errors: Vec<ApiError>,
    warnings: Vec<String>,
    fetched_at: String,
}
#[derive(Serialize)]
struct ShowCoverage {
    scope: &'static str,
    complete: bool,
}
pub async fn show(record: &CredentialRecord, class: &str, thread: &str) -> ShowEnvelope {
    show_inner(record, class, thread, Dest::Production).await
}
fn show_error(class: &str, thread: &str, code: &'static str, status: Option<u16>) -> ShowEnvelope {
    let _ = (class, thread);
    ShowEnvelope {
        schema_version: "1",
        status: "error",
        data: None,
        coverage: ShowCoverage {
            scope: "selected_thread",
            complete: false,
        },
        errors: vec![err(code, "show", None, status)],
        warnings: vec![],
        fetched_at: now(),
    }
}
async fn show_inner(
    record: &CredentialRecord,
    class: &str,
    thread: &str,
    dest: Dest,
) -> ShowEnvelope {
    if !crate::courses::safe_segment(class) || !crate::courses::safe_segment(thread) {
        return show_error(class, thread, "unsafe_identifier", None);
    }
    let path = ["func-bm7-forum-prod", "Forum", class, "Thread", thread];
    let req = match make_request(record, Method::GET, &path, false, &dest) {
        Ok(r) => r,
        Err(c) => return show_error(class, thread, c, None),
    };
    let root = match request_json(record, req, &dest, Instant::now() + OP).await {
        Ok(v) => v,
        Err((c, s)) => return show_error(class, thread, c, s),
    };
    if strf(&root, "id") != Ok(thread) || strf(&root, "forumId") != Ok(class) {
        return show_error(class, thread, "context_mismatch", None);
    }
    let parsed = (|| -> Result<ThreadDetail, &'static str> {
        let arr = root
            .get("attachments")
            .and_then(Value::as_array)
            .ok_or("invalid_schema")?;
        if arr.len() > 1000 {
            return Err("invalid_schema");
        }
        let mut attachments = Vec::new();
        for (i, a) in arr.iter().enumerate() {
            let size = a
                .get("size")
                .and_then(Value::as_u64)
                .ok_or("invalid_schema")?;
            let url = strf(a, "url")?;
            attachments.push(Attachment {
                source_position: i,
                name: strf(a, "name")?.into(),
                kind: strf(a, "type")?.into(),
                size_bytes: size,
                has_link: !url.is_empty(),
            });
        }
        let body = strf(&root, "threadContentText")?;
        let (content_text, redacted) = visible_text(body);
        let end = strf(&root, "dateEnd")?.to_owned();
        Ok(ThreadDetail {
            class_id: class.into(),
            thread_id: thread.into(),
            title: strf(&root, "threadTitle")?.into(),
            content_text,
            content_redacted: redacted,
            post_date: strf(&root, "threadPostDate")?.into(),
            post_date_utc: strf(&root, "threadPostDateUtc")?.into(),
            date_end: (!end.starts_with("0001-01-01")).then(|| end.clone()),
            source_date_end: end,
            status: strf(&root, "threadStatus")?.into(),
            masked: boolf(&root, "isMasked")?,
            assessment: boolf(&root, "isAssessment")?,
            session_number: root
                .get("sessionNumber")
                .and_then(Value::as_u64)
                .ok_or("invalid_schema")?,
            attachments,
        })
    })();
    match parsed {
        Err(c) => show_error(class, thread, c, None),
        Ok(data) => {
            let warnings = if data.content_redacted {
                vec!["Credential-bearing URL text was redacted from lecturer content.".into()]
            } else {
                vec![]
            };
            ShowEnvelope {
                schema_version: "1",
                status: "complete",
                data: Some(data),
                coverage: ShowCoverage {
                    scope: "selected_thread",
                    complete: true,
                },
                errors: vec![],
                warnings,
                fetched_at: now(),
            }
        }
    }
}
fn visible_text(html: &str) -> (String, bool) {
    let dom = parse_document(
        RcDom::default(),
        ParseOpts {
            tree_builder: TreeBuilderOpts {
                drop_doctype: true,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .one(html);
    fn walk(n: &Handle, out: &mut String, skip: bool) {
        match &n.data {
            NodeData::Text { contents } => {
                if !skip {
                    out.push_str(&contents.borrow())
                }
            }
            NodeData::Element { name, .. } => {
                let tag = name.local.as_ref();
                let hidden = skip || tag == "script" || tag == "style";
                if !hidden && matches!(tag, "p" | "div" | "br" | "li" | "h1" | "h2" | "h3" | "tr") {
                    out.push('\n')
                }
                for c in n.children.borrow().iter() {
                    walk(c, out, hidden)
                }
                if !hidden && matches!(tag, "p" | "div" | "li" | "h1" | "h2" | "h3" | "tr") {
                    out.push('\n')
                }
            }
            _ => {
                for c in n.children.borrow().iter() {
                    walk(c, out, skip)
                }
            }
        }
    }
    let mut raw = String::new();
    walk(&dom.document, &mut raw, false);
    let mut redacted = false;
    let clean = raw
        .lines()
        .filter_map(|line| {
            let mut words = Vec::new();
            for token in line.split_whitespace() {
                if sensitive_url(token) {
                    words.push("[redacted URL]");
                    redacted = true;
                } else {
                    words.push(token);
                }
            }
            (!words.is_empty()).then(|| words.join(" "))
        })
        .collect::<Vec<_>>()
        .join("\n");
    (clean, redacted)
}
fn sensitive_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    let normalized = lower
        .replace("&amp;", "&")
        .replace("&#38;", "&")
        .replace("&#x26;", "&");
    [
        "sig=",
        "token=",
        "access_token=",
        "refresh_token=",
        "signature=",
    ]
    .iter()
    .any(|k| normalized.contains(k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CredentialRecord, RoleContext};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
    };
    fn record() -> CredentialRecord {
        CredentialRecord {
            account: "test".into(),
            access_token: "synthetic-access-token-123456".into(),
            refresh_token: "synthetic-refresh-token-123456".into(),
            role: RoleContext {
                academic_career: "UGRD".into(),
                institution: "BINUS".into(),
                name: "Student".into(),
                role_id: "1".into(),
                organization_id: "2".into(),
            },
        }
    }
    fn fixture(responses: Vec<String>) -> (String, thread::JoinHandle<Vec<String>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.set_nonblocking(true).unwrap();
        let a = l.local_addr().unwrap();
        let h = thread::spawn(move || {
            let mut reqs = Vec::new();
            for body in responses {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                let (mut s, _) = loop {
                    match l.accept() {
                        Ok(connection) => break connection,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if std::time::Instant::now() >= deadline {
                                return reqs;
                            }
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => return reqs,
                    }
                };
                let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = s.set_write_timeout(Some(Duration::from_secs(2)));
                let mut bytes = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = match s.read(&mut buf) {
                        Ok(n) => n,
                        Err(_) => return reqs,
                    };
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    let end = bytes.windows(4).position(|x| x == b"\r\n\r\n");
                    if let Some(end) = end {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                        let len = headers
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length:")
                                    .and_then(|v| v.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                reqs.push(String::from_utf8_lossy(&bytes).to_string());
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            }
            reqs
        });
        (format!("http://{a}"), h)
    }
    fn status_fixture(status: u16) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    _ => return,
                }
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
            let mut request = [0; 2048];
            let _ = stream.read(&mut request);
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Fixture\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
        });
        (format!("http://{addr}"), handle)
    }
    fn accept_bounded(
        listener: &TcpListener,
    ) -> std::io::Result<(std::net::TcpStream, std::net::SocketAddr)> {
        listener.set_nonblocking(true)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok(connection) => return Ok(connection),
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(error) => return Err(error),
            }
        }
    }
    fn page(index: u8, id: &str, total: u8) -> String {
        format!(
            r#"{{"index":{index},"totalData":{total},"datas":[{{"id":"decoy-{id}","threadId":"{id}","threadTitle":"Topic {id}","postDate":"2026-01-01","postDateUtc":"2026-01-01T00:00:00Z","dateEnd":"0001-01-01T00:00:00","status":"Open","isMasked":false,"isLocked":false,"isAssessment":true,"totalPost":1,"totalComment":0,"authorRole":"Lecturer"}}]}}"#
        )
    }
    fn target(origin: String) -> Dest {
        Dest::Local(Target {
            origin,
            total: Duration::from_secs(3),
            request: Duration::from_secs(2),
            cap: CAP,
        })
    }
    fn parsed_request(raw: &str) -> (String, String, String, Value) {
        let (head, body) = raw.split_once("\r\n\r\n").expect("complete HTTP request");
        let mut lines = head.lines();
        let mut request = lines.next().unwrap().split_whitespace();
        let method = request.next().unwrap().to_owned();
        let path = request.next().unwrap().to_owned();
        let headers = lines
            .map(|line| {
                let (name, value) = line.split_once(':').expect("header name and value");
                format!("{}: {}", name.to_ascii_lowercase(), value.trim())
            })
            .collect::<Vec<_>>()
            .join("\n");
        let declared = headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("content-length:")
                    .and_then(|value| value.trim().parse::<usize>().ok())
            })
            .expect("declared body length");
        assert_eq!(
            body.len(),
            declared,
            "server captured complete declared body"
        );
        (
            method,
            path,
            headers,
            serde_json::from_str(body).expect("JSON request body"),
        )
    }
    fn honeypot() -> (String, Arc<AtomicBool>, thread::JoinHandle<usize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let handle = thread::spawn(move || {
            let mut accepted = 0;
            let deadline = std::time::Instant::now() + Duration::from_secs(8);
            while !worker_stop.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        accepted += 1;
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
                        let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
                        let mut req = [0; 1024];
                        let _ = stream.read(&mut req);
                        let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
            accepted
        });
        (format!("http://{addr}"), stop, handle)
    }
    fn delayed_fixture(
        responses: Vec<(String, Duration)>,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let mut reqs = Vec::new();
            let accept_deadline = std::time::Instant::now() + Duration::from_secs(5);
            for (body, delay) in responses {
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(connection) => break connection,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if std::time::Instant::now() >= accept_deadline {
                                return reqs;
                            }
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => return reqs,
                    }
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let mut bytes = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = match stream.read(&mut buf) {
                        Ok(n) => n,
                        Err(_) => return reqs,
                    };
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                reqs.push(String::from_utf8_lossy(&bytes).to_string());
                thread::sleep(delay);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            }
            reqs
        });
        (format!("http://{addr}"), handle)
    }
    fn custom_target(origin: String, total: Duration, request: Duration, cap: usize) -> Dest {
        Dest::Local(Target {
            origin,
            total,
            request,
            cap,
        })
    }
    #[tokio::test]
    async fn list_pipeline_follows_pages_and_uses_verified_route_body_and_headers() {
        let (o, h) = fixture(vec![page(1, "thread-A", 2), page(2, "thread-B", 2)]);
        let out = list_inner(&record(), "class-1", "session-2", target(o)).await;
        assert_eq!(out.status, "complete");
        assert_eq!(
            out.data
                .iter()
                .map(|r| r.thread_id.as_str())
                .collect::<Vec<_>>(),
            vec!["thread-A", "thread-B"]
        );
        let req = h.join().unwrap();
        assert_eq!(req.len(), 2);
        for (i, r) in req.iter().enumerate() {
            let (method, path, headers, body) = parsed_request(r);
            assert_eq!(method, "POST");
            assert_eq!(
                path,
                format!(
                    "/func-bm7-forum-prod/Thread/Class/class-1/Session/session-2/Paging/{}",
                    i + 1
                )
            );
            assert_eq!(body, json!({"TotalDataPerPage":20}));
            assert!(headers.contains("content-type: application/json"));
            assert!(
                headers
                    .lines()
                    .any(|line| line == "authorization: Bearer synthetic-access-token-123456")
            );
            for (name, value) in [
                ("academiccareer", "UGRD"),
                ("institution", "BINUS"),
                ("rolename", "Student"),
                ("roleid", "1"),
                ("roid", "2"),
            ] {
                assert!(
                    headers
                        .lines()
                        .any(|line| line.trim() == format!("{name}: {value}"))
                );
            }
            assert!(!headers.contains("cookie:"));
            assert!(!path.contains('?'));
            assert_eq!(body.as_object().unwrap().len(), 1);
        }
        assert!(req.iter().all(|request| !request.contains("decoy-thread-")));
        let (serialized, code) = crate::cli::courses_output(out);
        assert_eq!(serialized["status"], "complete");
        assert_eq!(code, std::process::ExitCode::SUCCESS);
    }
    #[tokio::test]
    async fn pagination_rejects_convention_switch_and_accepts_zero_based() {
        let (o, h) = fixture(vec![page(1, "thread-A", 2), page(1, "thread-B", 2)]);
        let out = list_inner(&record(), "class-1", "session-2", target(o)).await;
        assert_eq!(out.status, "partial");
        assert_eq!(
            out.data
                .iter()
                .map(|r| r.thread_id.as_str())
                .collect::<Vec<_>>(),
            vec!["thread-A"]
        );
        assert_eq!(out.coverage.pages_fetched, 1);
        assert_eq!(out.errors[0].code, "pagination_mismatch");
        assert_eq!(out.errors[0].page, Some(2));
        h.join().unwrap();

        let (o, h) = fixture(vec![page(0, "thread-A", 2), page(1, "thread-B", 2)]);
        let out = list_inner(&record(), "class-1", "session-2", target(o)).await;
        assert_eq!(out.status, "complete");
        assert_eq!(out.data.len(), 2);
        h.join().unwrap();
    }
    #[tokio::test]
    async fn show_pipeline_projects_html_and_attachment_without_fetching_embedded_urls() {
        let (hp, stop, observer) = honeypot();
        let body = format!(
            r#"{{"id":"thread-A","forumId":"class-1","threadTitle":"Title","threadContentText":"<p>Visible prose</p><script>bad()</script><a href=&#34;{hp}/honeypot&#34;>https://public.example/path</a><p>https://x/?sig=SECRET</p>","threadPostDate":"2026-01-01","threadPostDateUtc":"2026-01-01T00:00:00Z","dateEnd":"0001-01-01","threadStatus":"Open","isMasked":false,"isAssessment":false,"sessionNumber":2,"attachments":[{{"name":"slides.pdf","size":42,"type":"application/pdf","url":"{hp}/file"}}],"threadImages":[{{"url":"{hp}/image"}}]}}"#
        );
        let (o, h) = fixture(vec![body]);
        let out = show_inner(&record(), "class-1", "thread-A", target(o)).await;
        assert_eq!(out.status, "complete");
        let d = out.data.as_ref().unwrap();
        assert!(d.content_redacted);
        assert!(d.content_text.contains("Visible prose"));
        assert!(d.content_text.contains("https://public.example/path"));
        assert!(!d.content_text.contains("SECRET"));
        assert_eq!(d.attachments[0].name, "slides.pdf");
        let (serialized, code) = crate::cli::courses_output(out);
        assert_eq!(code, std::process::ExitCode::SUCCESS);
        assert_eq!(serialized["data"]["content_redacted"], true);
        assert!(serialized["data"]["attachments"][0].get("url").is_none());
        let req = h.join().unwrap();
        assert_eq!(req.len(), 1);
        assert!(req[0].starts_with("GET /func-bm7-forum-prod/Forum/class-1/Thread/thread-A"));
        assert!(!req[0].contains(&hp));
        stop.store(true, Ordering::SeqCst);
        assert_eq!(observer.join().unwrap(), 0);
    }
    #[tokio::test]
    async fn empty_is_complete_first_failure_errors_and_duplicates_are_exact() {
        let (o, h) = fixture(vec![r#"{"index":1,"totalData":0,"datas":[]}"#.into()]);
        let out = list_inner(&record(), "c", "s", target(o)).await;
        assert_eq!(out.status, "complete");
        assert!(out.data.is_empty());
        h.join().unwrap();
        let (o, h) = fixture(vec!["bad".into()]);
        let out = list_inner(&record(), "c", "s", target(o)).await;
        assert_eq!(out.status, "error");
        assert!(out.data.is_empty());
        assert_eq!(out.errors[0].code, "invalid_json");
        assert_eq!(out.coverage.pages_fetched, 0);
        assert!(!out.coverage.complete);
        let (serialized, code) = crate::cli::courses_output(out);
        assert_eq!(code, std::process::ExitCode::from(1));
        assert_eq!(serialized["errors"][0]["code"], "invalid_json");
        h.join().unwrap();
        let base = page(1, "thread-A", 1);
        let row = serde_json::from_str::<Value>(&base).unwrap()["datas"][0].clone();
        let duplicate = json!({"index":1,"totalData":1,"datas":[row.clone(),row]});
        let (o, h) = fixture(vec![duplicate.to_string()]);
        let out = list_inner(&record(), "c", "s", target(o)).await;
        assert_eq!(out.status, "complete");
        assert_eq!(out.data.len(), 1);
        h.join().unwrap();
        let mut other = serde_json::from_str::<Value>(&base).unwrap()["datas"][0].clone();
        other["threadTitle"] = json!("different");
        let conflict = json!({"index":1,"totalData":1,"datas":[serde_json::from_str::<Value>(&base).unwrap()["datas"][0].clone(),other]});
        let (o, h) = fixture(vec![conflict.to_string()]);
        let out = list_inner(&record(), "c", "s", target(o)).await;
        assert_eq!(out.status, "error");
        assert!(out.data.is_empty());
        assert_eq!(out.errors[0].code, "conflicting_duplicate");
        h.join().unwrap();
    }
    #[tokio::test]
    async fn later_page_failure_retains_validated_rows_and_reports_partial() {
        let (o, h) = fixture(vec![page(1, "thread-A", 2), "not json".into()]);
        let out = list_inner(&record(), "class-1", "session-2", target(o)).await;
        assert_eq!(out.status, "partial");
        assert_eq!(out.data.len(), 1);
        assert_eq!(out.data[0].thread_id, "thread-A");
        assert_eq!(out.errors[0].page, Some(2));
        assert_eq!(out.errors[0].code, "invalid_json");
        assert_eq!(out.coverage.pages_fetched, 1);
        assert!(!out.coverage.complete);
        let (serialized, code) = crate::cli::courses_output(out);
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(serialized["status"], "partial");
        assert_eq!(serialized["errors"][0]["code"], "invalid_json");
        h.join().unwrap();
    }
    #[tokio::test]
    async fn upstream_statuses_and_closed_port_are_serialized_as_errors() {
        for (status, expected) in [
            (401, "authorization_required"),
            (403, "authorization_required"),
            (500, "upstream_error"),
        ] {
            let (origin, worker) = status_fixture(status);
            let out = list_inner(&record(), "c", "s", target(origin)).await;
            assert_eq!(out.status, "error");
            assert!(out.data.is_empty());
            assert_eq!(out.errors[0].code, expected);
            assert_eq!(out.errors[0].http_status, Some(status));
            assert_eq!(out.coverage.pages_fetched, 0);
            let (serialized, code) = crate::cli::courses_output(out);
            assert_eq!(code, std::process::ExitCode::from(1));
            assert_eq!(serialized["status"], "error");
            assert_eq!(serialized["errors"][0]["code"], expected);
            assert_eq!(serialized["errors"][0]["http_status"], status);
            worker.join().unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let out = list_inner(&record(), "c", "s", target(format!("http://{addr}"))).await;
        assert_eq!(out.status, "error");
        assert_eq!(out.errors[0].code, "network_error");
        assert_eq!(out.errors[0].http_status, None);
        let (serialized, code) = crate::cli::courses_output(out);
        assert_eq!(code, std::process::ExitCode::from(1));
        assert_eq!(serialized["errors"][0]["code"], "network_error");
    }
    #[tokio::test]
    async fn repeated_page_without_new_unique_rows_stops_as_partial() {
        let (o, h) = fixture(vec![page(1, "thread-A", 2), page(2, "thread-A", 2)]);
        let out = list_inner(&record(), "c", "s", target(o)).await;
        assert_eq!(out.status, "partial");
        assert_eq!(out.data.len(), 1);
        assert_eq!(out.errors[0].code, "nonprogress");
        let _ = h.join().unwrap();
    }
    #[tokio::test]
    async fn malformed_later_list_row_discards_only_that_page() {
        let valid =
            serde_json::from_str::<Value>(&page(2, "thread-B", 3)).unwrap()["datas"][0].clone();
        let mut malformed =
            serde_json::from_str::<Value>(&page(2, "thread-C", 3)).unwrap()["datas"][0].clone();
        malformed.as_object_mut().unwrap().remove("postDate");
        let later = json!({"index":2,"totalData":3,"datas":[valid,malformed]});
        let (o, h) = fixture(vec![page(1, "thread-A", 3), later.to_string()]);
        let out = list_inner(&record(), "c", "s", target(o)).await;
        assert_eq!(out.status, "partial");
        assert_eq!(
            out.data
                .iter()
                .map(|r| r.thread_id.as_str())
                .collect::<Vec<_>>(),
            vec!["thread-A"]
        );
        assert_eq!(out.errors[0].code, "invalid_schema");
        let _ = h.join().unwrap();
    }
    #[tokio::test]
    async fn wrong_detail_context_is_rejected_and_malformed_attachment_is_atomic() {
        let good = r#"{"id":"other","forumId":"class-1"}"#.to_owned();
        let (o, h) = fixture(vec![good]);
        let out = show_inner(&record(), "class-1", "thread-A", target(o)).await;
        assert_eq!(out.errors[0].code, "context_mismatch");
        h.join().unwrap();
        let bad=r#"{"id":"thread-A","forumId":"class-1","attachments":[{"name":"x","size":"bad","type":"pdf","url":"https://x"}]}"#.to_owned();
        let (o, h) = fixture(vec![bad]);
        let out = show_inner(&record(), "class-1", "thread-A", target(o)).await;
        assert_eq!(out.status, "error");
        assert!(out.data.is_none());
        assert_eq!(out.errors[0].code, "invalid_schema");
        h.join().unwrap();
    }
    #[tokio::test]
    async fn request_deadline_covers_headers_and_all_response_chunks() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let h = thread::spawn(move || {
            let (mut s, _) = accept_bounded(&listener).unwrap();
            let _ = s.set_read_timeout(Some(Duration::from_secs(1)));
            let _ = s.set_write_timeout(Some(Duration::from_secs(1)));
            let mut b = [0u8; 4096];
            let _ = s.read(&mut b);
            thread::sleep(Duration::from_millis(250));
            let _ = s.write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            );
            let first = br#"{"first":"chunk"}"#;
            let _ = write!(s, "{:X}\r\n", first.len());
            let _ = s.write_all(first);
            let _ = s.write_all(b"\r\n");
            thread::sleep(Duration::from_millis(400));
            let last = br#"{"next":"chunk"}"#;
            let _ = write!(s, "{:X}\r\n", last.len());
            let _ = s.write_all(last);
            let _ = s.write_all(b"\r\n0\r\n\r\n");
        });
        let origin = format!("http://{addr}");
        let dest = custom_target(
            origin.clone(),
            Duration::from_secs(3),
            Duration::from_millis(500),
            CAP,
        );
        let req = make_request(
            &record(),
            Method::GET,
            &[
                "func-bm7-forum-prod",
                "Forum",
                "class-1",
                "Thread",
                "thread-A",
            ],
            false,
            &dest,
        )
        .unwrap();
        let e = request_json(
            &record(),
            req,
            &dest,
            Instant::now() + Duration::from_secs(3),
        )
        .await
        .unwrap_err();
        assert_eq!(e.0, "timeout");
        h.join().unwrap();
    }
    #[tokio::test]
    async fn redirect_is_refused_without_forwarding_authorization() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let h = thread::spawn(move || {
            let (mut s, _) = accept_bounded(&listener).unwrap();
            let _ = s.set_read_timeout(Some(Duration::from_secs(1)));
            let _ = s.set_write_timeout(Some(Duration::from_secs(1)));
            let mut b = [0u8; 4096];
            let n = s.read(&mut b).unwrap();
            let req = String::from_utf8_lossy(&b[..n]).to_string();
            let _=s.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            req
        });
        let origin = format!("http://{addr}");
        let dest = custom_target(
            origin.clone(),
            Duration::from_secs(2),
            Duration::from_secs(1),
            CAP,
        );
        let req = make_request(
            &record(),
            Method::GET,
            &["func-bm7-forum-prod", "Forum", "c", "Thread", "t"],
            false,
            &dest,
        )
        .unwrap();
        let e = request_json(
            &record(),
            req,
            &dest,
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .unwrap_err();
        assert_eq!(e, ("upstream_error", Some(302)));
        let captured = h.join().unwrap();
        assert!(captured.contains("authorization: Bearer synthetic-access-token-123456"));
        assert!(!captured.contains("/steal"));
    }
    #[tokio::test]
    async fn response_cap_applies_to_chunked_unknown_length_body() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let h = thread::spawn(move || {
            let (mut s, _) = accept_bounded(&listener).unwrap();
            let _ = s.set_read_timeout(Some(Duration::from_secs(1)));
            let _ = s.set_write_timeout(Some(Duration::from_secs(1)));
            let mut b = [0u8; 4096];
            let _ = s.read(&mut b);
            let _ = s.write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            );
            for chunk in [
                b"[]                              ".as_slice(),
                b"                                ",
                b"                                ",
                b"                                ",
            ] {
                let _ = write!(s, "{:X}\r\n", chunk.len());
                let _ = s.write_all(chunk);
                let _ = s.write_all(b"\r\n");
            }
            let _ = s.write_all(b"0\r\n\r\n");
        });
        let origin = format!("http://{addr}");
        let req = make_request(
            &record(),
            Method::GET,
            &[
                "func-bm7-forum-prod",
                "Forum",
                "class-1",
                "Thread",
                "thread-A",
            ],
            false,
            &custom_target(
                origin.clone(),
                Duration::from_secs(2),
                Duration::from_secs(2),
                64,
            ),
        )
        .unwrap();
        let err = request_json(
            &record(),
            req,
            &custom_target(origin, Duration::from_secs(2), Duration::from_secs(2), 64),
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .unwrap_err();
        assert_eq!(err.0, "response_too_large");
        h.join().unwrap();
    }
    #[tokio::test]
    async fn operation_deadline_spans_pages_and_retains_page_one() {
        let (o, h) = delayed_fixture(vec![
            (page(1, "thread-A", 2), Duration::from_millis(800)),
            (page(2, "thread-B", 2), Duration::from_millis(1200)),
        ]);
        let out = list_inner(
            &record(),
            "class-1",
            "session-2",
            custom_target(o, Duration::from_millis(1800), Duration::from_secs(3), CAP),
        )
        .await;
        assert_eq!(out.status, "partial");
        assert_eq!(out.data.len(), 1);
        assert_eq!(out.coverage.pages_fetched, 1);
        assert!(!out.coverage.complete);
        assert_eq!(out.errors[0].code, "timeout");
        h.join().unwrap();
    }
    #[test]
    fn html_text_skips_markup_and_redacts_signed_urls() {
        let (t, r) = visible_text(
            "<p>Hello <b>world</b></p><script>evil()</script><a href='https://x/?sig=SECRET'>https://x/?sig=SECRET</a>",
        );
        assert!(r);
        assert!(t.contains("Hello world"));
        assert!(!t.contains("evil"));
        assert!(!t.contains("SECRET"));
    }
}
