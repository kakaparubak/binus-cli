use crate::{CredentialRecord, academic_period_request, courses::timestamp};
use reqwest::redirect::Policy;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const SERVICE: &str = "func-bm7-course-prod";
const BODY_CAP: usize = 8 * 1024 * 1024;
const TOTAL: Duration = Duration::from_secs(60);

#[cfg(test)]
#[derive(Clone)]
struct Target {
    origin: String,
    total: Duration,
    request: Duration,
    cap: usize,
}

fn error_value(
    session_id: &str,
    code: &str,
    stage: &str,
    source: Option<&str>,
    status: Option<u16>,
) -> Value {
    let mut e = json!({"code":code,"stage":stage,"session_id":session_id});
    if let Some(s) = source {
        e["source"] = json!(s);
    }
    if let Some(s) = status {
        e["http_status"] = json!(s);
    }
    e
}

fn sentinel_date(v: Option<&Value>) -> (Value, Value) {
    let raw = v.cloned().filter(Value::is_string).unwrap_or(Value::Null);
    let normalized = raw
        .as_str()
        .map(|s| {
            if s.starts_with("0001-01-01") {
                Value::Null
            } else {
                json!(s)
            }
        })
        .unwrap_or(Value::Null);
    (normalized, raw)
}

fn paginated(v: &Value) -> bool {
    let Some(o) = v.as_object() else { return false };
    o.iter().any(|(k, x)| match k.as_str() {
        "hasNext" | "hasMore" => x == true,
        "continuationToken" | "nextToken" | "nextPage" | "nextCursor" => {
            x.as_str().is_some_and(|s| !s.is_empty())
        }
        "pagination" | "paging" => paginated(x),
        _ => false,
    })
}

fn required_string(v: &Value, key: &str) -> Result<String, &'static str> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or("invalid_schema")
}

fn map_resources(
    root: &Value,
    session_id: &str,
    primary: bool,
) -> Result<(Vec<Value>, Vec<Value>), &'static str> {
    if paginated(root) {
        return Err("unsupported_pagination");
    }
    let key = if primary {
        "resources"
    } else {
        "additionalResources"
    };
    let rows = root
        .get(key)
        .and_then(Value::as_array)
        .ok_or("invalid_schema")?;
    let mut seen: std::collections::HashMap<String, &Value> = Default::default();
    let mut mapped = Vec::new();
    for row in rows {
        for key in ["resourceId", "type", "tabName", "url"] {
            if row.get(key).is_none() {
                return Err("invalid_schema");
            }
        }
        let id = required_string(row, "id")?;
        let class_session_id = required_string(row, "classSessionId")?;
        if class_session_id != session_id {
            return Err("context_mismatch");
        }
        let name = required_string(row, "name")?;
        let resource_type = required_string(row, "resourceType")?;
        let index = row
            .get("index")
            .and_then(Value::as_i64)
            .ok_or("invalid_schema")?;
        let raw_type = row.get("type").cloned().unwrap_or(Value::Null);
        if !raw_type.is_null() && !raw_type.is_string() {
            return Err("invalid_schema");
        }
        let tab_name = required_string(row, "tabName")?;
        let url = row.get("url");
        let has_link = url.and_then(Value::as_str).is_some_and(|s| !s.is_empty());
        let mut out = json!({"id":id,"session_resource_id":id,"resource_id":row.get("resourceId").cloned().unwrap_or(Value::Null),"class_session_id":class_session_id,"name":name,"raw_resource_type":resource_type,"raw_type":raw_type,"index":index,"tab_name":tab_name,"is_open":row.get("isOpen").and_then(Value::as_bool).ok_or("invalid_schema")?,"has_link":has_link});
        if !out["resource_id"].is_null() && !out["resource_id"].is_string() {
            return Err("invalid_schema");
        }
        if !row["url"].is_null() && !row["url"].is_string() {
            return Err("invalid_schema");
        }
        if primary {
            let (due, raw_due) = sentinel_date(row.get("dueDate"));
            out["due_date"] = due;
            out["source_due_date"] = raw_due;
            for (source, dest) in [
                ("contentLength", "source_content_length"),
                ("progressStatus", "source_progress_status"),
                ("progressStamp", "source_progress_stamp"),
                ("resourceStatus", "source_resource_status"),
                ("timesAccessed", "source_times_accessed"),
                ("lastUpdatedDate", "source_last_updated_date"),
            ] {
                if let Some(value) = row.get(source) {
                    if value.is_null() {
                        continue;
                    }
                    let valid = match source {
                        "contentLength" | "progressStatus" | "progressStamp" | "timesAccessed" => {
                            value.as_i64().is_some()
                        }
                        "resourceStatus" | "lastUpdatedDate" => value.is_string(),
                        _ => false,
                    };
                    if !valid {
                        return Err("invalid_schema");
                    }
                    out[dest] = value.clone();
                }
            }
        }
        if let Some(old) = seen.get(&id) {
            if *old != row {
                return Err("conflicting_duplicate");
            }
        } else {
            seen.insert(id, row);
            mapped.push(out);
        }
    }
    Ok((mapped, Vec::new()))
}

fn map_primary(root: &Value, session_id: &str) -> Result<Value, &'static str> {
    if !root.is_object() || paginated(root) {
        return Err(if paginated(root) {
            "unsupported_pagination"
        } else {
            "invalid_schema"
        });
    }
    let session_number = root
        .get("sessionNumber")
        .and_then(Value::as_i64)
        .ok_or("invalid_schema")?;
    for key in [
        "academicPeriod",
        "classCode",
        "classDeliveryMode",
        "ssrComponent",
        "courseCode",
        "courseTitleEn",
        "dateStart",
        "dateEnd",
        "startDateSessionUtc",
        "endDateSessionUtc",
        "deliveryMode",
        "deliveryModeDesc",
    ] {
        if root.get(key).and_then(Value::as_str).is_none() {
            return Err("invalid_schema");
        }
    }
    let topic = required_string(root, "topic")?;
    let subtopics = root
        .get("courseSubTopic")
        .and_then(Value::as_array)
        .ok_or("invalid_schema")?;
    if !subtopics.iter().all(Value::is_string) {
        return Err("invalid_schema");
    }
    let outcomes = root
        .get("learningOutcomes")
        .and_then(Value::as_array)
        .ok_or("invalid_schema")?;
    let mut texts = Vec::new();
    for row in outcomes {
        texts.push(required_string(row, "learningOutcome")?);
    }
    let (start, raw_start) = sentinel_date(root.get("dateStart"));
    let (end, raw_end) = sentinel_date(root.get("dateEnd"));
    let resources = root
        .get("resources")
        .and_then(Value::as_array)
        .ok_or("invalid_schema")?;
    let count = root
        .get("totalResource")
        .and_then(Value::as_u64)
        .ok_or("invalid_schema")?;
    let (mapped, _) = map_resources(&json!({"resources":resources}), session_id, true)?;
    let mismatch = count != resources.len() as u64;
    let mut data = json!({"session_id":session_id,"topic":topic,"subtopics":subtopics,"learning_outcomes":texts,"course_code":root.get("courseCode").cloned().unwrap_or(Value::Null),"course_title":root.get("courseTitleEn").cloned().unwrap_or(Value::Null),"class_code":root.get("classCode").cloned().unwrap_or(Value::Null),"component":root.get("ssrComponent").cloned().unwrap_or(Value::Null),"period":root.get("academicPeriod").cloned().unwrap_or(Value::Null),"source_session_number":session_number,"start_date":start,"source_start_date":raw_start,"end_date":end,"source_end_date":raw_end,"start_date_session_utc":root.get("startDateSessionUtc").cloned().unwrap_or(Value::Null),"end_date_session_utc":root.get("endDateSessionUtc").cloned().unwrap_or(Value::Null),"delivery_code":root.get("deliveryMode").cloned().unwrap_or(Value::Null),"delivery_description":root.get("deliveryModeDesc").cloned().unwrap_or(Value::Null),"primary_resources":mapped});
    if mismatch {
        data["_declared_total_resource"] = json!(count);
        data["_observed_resource_rows"] = json!(resources.len());
    }
    Ok(data)
}

#[derive(Clone)]
struct SourceResult {
    data: Option<Value>,
    error: Option<Value>,
    status: &'static str,
}

fn envelope(session_id: &str, primary: SourceResult, additional: SourceResult) -> Value {
    let failed = usize::from(primary.data.is_none()) + usize::from(additional.data.is_none());
    let mut data = primary.data.unwrap_or_else(|| json!({"session_id":session_id,"topic":null,"subtopics":null,"learning_outcomes":null,"course_code":null,"course_title":null,"class_code":null,"component":null,"period":null,"source_session_number":null,"start_date":null,"source_start_date":null,"end_date":null,"source_end_date":null,"start_date_session_utc":null,"end_date_session_utc":null,"delivery_code":null,"delivery_description":null,"primary_resources":null}));
    data["additional_resources"] = additional.data.unwrap_or(Value::Null);
    let errors: Vec<Value> = primary.error.into_iter().chain(additional.error).collect();
    let incomplete = primary.status != "complete" || additional.status != "complete";
    let status = if failed == 0 && !incomplete {
        "complete"
    } else if failed == 2 {
        "error"
    } else {
        "partial"
    };
    json!({"schema_version":"1","status":status,"data":if failed == 2 { Value::Null } else { data },"errors":errors,"warnings":[],"coverage":{"scope":"selected_session","selected_sessions":1,"discovery_complete":!incomplete,"complete":!incomplete,"primary":primary.status,"additional":additional.status},"fetched_at":timestamp()})
}

#[cfg(test)]
async fn get(
    record: &CredentialRecord,
    session_id: &str,
    source: &str,
    target: &Target,
    started: Instant,
    client: &reqwest::Client,
) -> Result<Value, Value> {
    get_impl(record, session_id, source, target, started, client).await
}
#[cfg(not(test))]
async fn get(
    record: &CredentialRecord,
    session_id: &str,
    source: &str,
    started: Instant,
    client: &reqwest::Client,
) -> Result<Value, Value> {
    let target = Production;
    get_impl(record, session_id, source, &target, started, client).await
}

#[cfg(test)]
struct TargetImpl<'a>(&'a Target);
#[cfg(test)]
impl TargetImpl<'_> {
    fn origin(&self) -> &str {
        &self.0.origin
    }
    fn total(&self) -> Duration {
        self.0.total
    }
    fn request(&self) -> Duration {
        self.0.request
    }
    fn cap(&self) -> usize {
        self.0.cap
    }
}
#[cfg(not(test))]
struct Production;
#[cfg(test)]
async fn get_impl(
    record: &CredentialRecord,
    session_id: &str,
    source: &str,
    target: &Target,
    started: Instant,
    client: &reqwest::Client,
) -> Result<Value, Value> {
    get_with_target(
        record,
        session_id,
        source,
        &TargetImpl(target),
        started,
        client,
    )
    .await
}
#[cfg(not(test))]
async fn get_impl(
    record: &CredentialRecord,
    session_id: &str,
    source: &str,
    target: &Production,
    started: Instant,
    client: &reqwest::Client,
) -> Result<Value, Value> {
    get_with_target(record, session_id, source, target, started, client).await
}

async fn get_with_target<T: TargetConfig>(
    record: &CredentialRecord,
    session_id: &str,
    source: &str,
    target: &T,
    started: Instant,
    client: &reqwest::Client,
) -> Result<Value, Value> {
    let mut req = academic_period_request(record)
        .map_err(|_| error_value(session_id, "request_error", "request", Some(source), None))?;
    #[cfg(test)]
    {
        let origin = target.origin();
        let parsed = url::Url::parse(&format!("{origin}/"))
            .map_err(|_| error_value(session_id, "request_error", "request", Some(source), None))?;
        if !parsed
            .host_str()
            .is_some_and(|h| h == "127.0.0.1" || h == "localhost" || h == "::1")
        {
            return Err(error_value(
                session_id,
                "request_error",
                "request",
                Some(source),
                None,
            ));
        }
        *req.url_mut() = parsed;
    }
    let mut path = req
        .url_mut()
        .path_segments_mut()
        .map_err(|_| error_value(session_id, "request_error", "request", Some(source), None))?;
    path.clear();
    for part in [SERVICE, "ClassSession", "Session", session_id] {
        path.push(part);
    }
    if source == "primary" {
        path.push("Resource");
        path.push("Student");
    } else {
        path.push("AdditionalResources");
    }
    drop(path);
    if source == "primary" {
        req.url_mut().query_pairs_mut().append_pair("isWeb", "true");
    }
    *req.method_mut() = reqwest::Method::GET;
    let budget_end = started + target.total();
    let deadline = (Instant::now() + target.request()).min(budget_end);
    let response = tokio::time::timeout_at(deadline.into(), client.execute(req))
        .await
        .map_err(|_| error_value(session_id, "timeout", "request", Some(source), None))?
        .map_err(|_| error_value(session_id, "network_error", "request", Some(source), None))?;
    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        return Err(error_value(
            session_id,
            "reauth_required",
            "request",
            Some(source),
            Some(status),
        ));
    }
    if status != 200 {
        return Err(error_value(
            session_id,
            "upstream_error",
            "request",
            Some(source),
            Some(status),
        ));
    }
    if response
        .content_length()
        .is_some_and(|n| n > target.cap() as u64)
    {
        return Err(error_value(
            session_id,
            "body_too_large",
            "response",
            Some(source),
            None,
        ));
    }
    let mut response = response;
    let mut bytes = Vec::new();
    loop {
        let chunk = tokio::time::timeout_at(deadline.into(), response.chunk())
            .await
            .map_err(|_| error_value(session_id, "timeout", "response", Some(source), None))?
            .map_err(|_| {
                error_value(session_id, "response_error", "response", Some(source), None)
            })?;
        let Some(chunk) = chunk else {
            break;
        };
        if bytes.len().saturating_add(chunk.len()) > target.cap() {
            return Err(error_value(
                session_id,
                "body_too_large",
                "response",
                Some(source),
                None,
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| error_value(session_id, "invalid_json", "response", Some(source), None))
}
trait TargetConfig {
    #[cfg(test)]
    fn origin(&self) -> &str;
    fn total(&self) -> Duration;
    fn request(&self) -> Duration;
    fn cap(&self) -> usize;
}
#[cfg(not(test))]
impl TargetConfig for Production {
    fn total(&self) -> Duration {
        TOTAL
    }
    fn request(&self) -> Duration {
        Duration::from_secs(20)
    }
    fn cap(&self) -> usize {
        BODY_CAP
    }
}
#[cfg(test)]
impl TargetConfig for TargetImpl<'_> {
    fn origin(&self) -> &str {
        self.origin()
    }
    fn total(&self) -> Duration {
        self.total()
    }
    fn request(&self) -> Duration {
        self.request()
    }
    fn cap(&self) -> usize {
        self.cap()
    }
}

pub async fn show(record: &CredentialRecord, session_id: &str) -> Value {
    if !crate::courses::safe_segment(session_id) {
        return envelope(
            session_id,
            SourceResult {
                data: None,
                error: Some(error_value(
                    session_id,
                    "unsafe_identifier",
                    "request",
                    Some("primary"),
                    None,
                )),
                status: "failed",
            },
            SourceResult {
                data: None,
                error: Some(error_value(
                    session_id,
                    "unsafe_identifier",
                    "request",
                    Some("additional"),
                    None,
                )),
                status: "failed",
            },
        );
    }
    show_inner(record, session_id, None).await
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::RoleContext;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn record() -> CredentialRecord {
        CredentialRecord {
            account: "synthetic".into(),
            access_token: "synthetic-token".into(),
            refresh_token: "synthetic-refresh".into(),
            role: RoleContext {
                academic_career: "UGRD".into(),
                institution: "BINUS".into(),
                name: "Student".into(),
                role_id: "STUDENT".into(),
                organization_id: "ORG".into(),
            },
        }
    }
    fn fixture() -> Value {
        json!({"academicPeriod":"2610","classCode":"A","classDeliveryMode":"HYBRID","ssrComponent":"LAB","courseCode":"CODE","courseTitleEn":"Synthetic","topic":"Topic text","courseSubTopic":["Subtopic"],"sessionNumber":1,"learningOutcomes":[{"learningOutcome":"Outcome"}],"dateStart":"0001-01-01T00:00:00","dateEnd":"2026-10-19T10:00:00","startDateSessionUtc":"2026-10-19T03:00:00Z","endDateSessionUtc":"2026-10-19T04:00:00Z","deliveryMode":"ONLINE","deliveryModeDesc":"Online","resources":[{"id":"sid","classSessionId":"s-1","resourceId":null,"name":"Material","resourceType":"future-type","type":null,"index":0,"tabName":"Materials","isOpen":true,"url":"https://signed.invalid/secret","token":"secret","dueDate":"0001-01-01T00:00:00","contentLength":23,"progressStatus":2,"progressStamp":3,"resourceStatus":"read","timesAccessed":4,"lastUpdatedDate":"2026-10-18"}],"totalResource":1,"lecturers":[{"id":"private"}],"attendanceDateTime":"private","joinUrl":"https://private.invalid"})
    }
    #[test]
    fn verified_shapes_map_to_separate_safe_arrays() {
        let primary = map_primary(&fixture(), "s-1").unwrap();
        assert_eq!(primary["topic"], "Topic text");
        assert_eq!(primary["component"], "LAB");
        assert_eq!(primary["subtopics"][0], "Subtopic");
        assert_eq!(primary["learning_outcomes"][0], "Outcome");
        assert_eq!(primary["start_date"], Value::Null);
        assert_eq!(primary["source_start_date"], "0001-01-01T00:00:00");
        assert_eq!(primary["primary_resources"][0]["resource_id"], Value::Null);
        assert_eq!(primary["primary_resources"][0]["id"], "sid");
        assert_eq!(
            primary["primary_resources"][0]["raw_resource_type"],
            "future-type"
        );
        let encoded = primary.to_string();
        for secret in [
            "signed.invalid",
            "secret",
            "private.invalid",
            "lecturers",
            "attendanceDateTime",
            "joinUrl",
        ] {
            assert!(!encoded.contains(secret));
        }
        let (additional,_) = map_resources(&json!({"additionalResources":[{"id":"a","classSessionId":"s-1","resourceId":"r","name":"Link","resourceType":"URL","type":null,"index":1,"tabName":"More","isOpen":true,"url":"https://signed.invalid/x","generatedDate":"raw"}]}),"s-1",false).unwrap();
        assert_eq!(additional[0]["has_link"], true);
        assert!(additional[0].get("source_progress_status").is_none());
        assert!(additional[0].get("url").is_none());
    }
    #[test]
    fn invalid_context_count_pagination_and_conflicting_ids_fail_atomically() {
        let mut v = fixture();
        v["totalResource"] = json!(9);
        assert_eq!(map_primary(&v, "s-1").unwrap()["topic"], "Topic text");
        let mut v = fixture();
        v["hasMore"] = json!(true);
        assert_eq!(
            map_primary(&v, "s-1").unwrap_err(),
            "unsupported_pagination"
        );
        let mut row = v["resources"][0].clone();
        row["classSessionId"] = json!("other");
        let mut v = fixture();
        v["resources"] = json!([row]);
        v["totalResource"] = json!(1);
        assert_eq!(map_primary(&v, "s-1").unwrap_err(), "context_mismatch");
        let row = json!({"id":"dup","classSessionId":"s-1","resourceId":null,"name":"A","resourceType":"DOC","type":null,"index":0,"tabName":"tab","isOpen":true,"url":null});
        let mut other = row.clone();
        other["name"] = json!("B");
        assert_eq!(
            map_resources(&json!({"additionalResources":[row,other]}), "s-1", false).unwrap_err(),
            "conflicting_duplicate"
        );
    }

    #[test]
    fn exact_source_duplicates_dedupe_but_hidden_field_conflicts_fail() {
        let row = json!({"id":"dup","classSessionId":"s-1","resourceId":null,"name":"A","resourceType":"DOC","type":null,"index":0,"tabName":"tab","isOpen":true,"url":null});
        let (rows, _) = map_resources(
            &json!({"additionalResources":[row.clone(),row.clone()]}),
            "s-1",
            false,
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        let mut changed = row.clone();
        changed["url"] = json!("https://signed.invalid/a");
        assert_eq!(
            map_resources(&json!({"additionalResources":[row,changed]}), "s-1", false).unwrap_err(),
            "conflicting_duplicate"
        );
    }

    #[test]
    fn malformed_progress_fact_fails_closed() {
        let mut v = fixture();
        v["resources"][0]["progressStatus"] =
            json!({"token":"synthetic-secret","url":"https://signed.invalid/x"});
        assert_eq!(map_primary(&v, "s-1").unwrap_err(), "invalid_schema");
    }

    fn server(responses: Vec<(u16, String)>) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0; 1024];
                loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&buf[..end]).to_lowercase();
                        let length = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(0);
                        if buf.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8_lossy(&buf).into_owned());
                write!(stream,"HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
            requests
        });
        (format!("http://{addr}"), handle)
    }
    fn streamed_oversize_server(
        primary: String,
        streamed_body: Vec<u8>,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for index in 0..2 {
                let accept_deadline = Instant::now() + Duration::from_secs(4);
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(connection) => break connection,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if Instant::now() >= accept_deadline {
                                return requests;
                            }
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => return requests,
                    }
                };
                requests.push(read_request(&mut stream));
                if index == 0 {
                    if write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{primary}", primary.len()).is_err() {
                        return requests;
                    }
                } else {
                    if stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").is_err() {
                        return requests;
                    }
                    for chunk in streamed_body.chunks(512) {
                        if write!(stream, "{:X}\r\n", chunk.len()).is_err()
                            || stream.write_all(chunk).is_err()
                            || stream.write_all(b"\r\n").is_err()
                        {
                            break;
                        }
                    }
                    // The client may close as soon as the cumulative body crosses its cap.
                    let _ = stream.write_all(b"0\r\n\r\n");
                }
            }
            requests
        });
        (format!("http://{addr}"), handle)
    }

    fn target(origin: String) -> Target {
        Target {
            origin,
            total: Duration::from_secs(3),
            request: Duration::from_secs(2),
            cap: BODY_CAP,
        }
    }

    fn read_request(stream: &mut std::net::TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0; 1024];
        loop {
            let n = stream.read(&mut chunk).unwrap_or(0);
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
    }

    fn reply(stream: &mut std::net::TcpStream, status: u16, reason: &str, body: &str, extra: &str) {
        write!(stream, "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}", body.len()).unwrap();
    }

    fn additional_body() -> String {
        json!([{"id":"a","classSessionId":"s-1","resourceId":null,"name":"Extra","resourceType":"DOC","type":null,"index":0,"tabName":"More","isOpen":true,"url":null}]).to_string()
    }

    fn observer(
        responses: Vec<(u16, String, String)>,
    ) -> (
        String,
        std::sync::mpsc::Sender<()>,
        thread::JoinHandle<Vec<String>>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let request = read_request(&mut stream);
                        let index = requests.len();
                        requests.push(request);
                        if let Some((status, body, extra)) = responses.get(index) {
                            reply(
                                &mut stream,
                                *status,
                                if *status == 200 { "OK" } else { "Found" },
                                body,
                                extra,
                            );
                        } else {
                            reply(&mut stream, 200, "OK", "{}", "");
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
            requests
        });
        (format!("http://{addr}"), stop_tx, handle)
    }

    fn stop_observer(
        stop: std::sync::mpsc::Sender<()>,
        handle: thread::JoinHandle<Vec<String>>,
    ) -> Vec<String> {
        let _ = stop.send(());
        handle.join().unwrap()
    }

    struct DelayedResponse {
        header_delay: Duration,
        body_chunks: Vec<(Duration, Vec<u8>)>,
    }

    fn delayed_server(
        responses: Vec<DelayedResponse>,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for response in responses {
                let accept_deadline = Instant::now() + Duration::from_secs(4);
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(connection) => break connection,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if Instant::now() >= accept_deadline {
                                return requests;
                            }
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => return requests,
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                requests.push(read_request(&mut stream));
                thread::sleep(response.header_delay);
                let body_len: usize = response.body_chunks.iter().map(|(_, c)| c.len()).sum();
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n"
                );
                if stream.write_all(header.as_bytes()).is_err() {
                    break;
                }
                for (delay, chunk) in response.body_chunks {
                    thread::sleep(delay);
                    if stream.write_all(&chunk).is_err() {
                        break;
                    }
                }
            }
            requests
        });
        (format!("http://{addr}"), handle)
    }

    fn delayed_body(body: String, header_delay: Duration, body_delay: Duration) -> DelayedResponse {
        DelayedResponse {
            header_delay,
            body_chunks: vec![(body_delay, body.into_bytes())],
        }
    }

    fn delayed_chunks(chunks: Vec<Vec<u8>>, delay: Duration) -> DelayedResponse {
        DelayedResponse {
            header_delay: Duration::ZERO,
            body_chunks: chunks.into_iter().map(|c| (delay, c)).collect(),
        }
    }

    fn assert_timeout_partial(out: &Value, code: std::process::ExitCode, source: &str) {
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(out["status"], "partial");
        assert_eq!(out["coverage"]["complete"], false);
        assert_eq!(out["errors"][0]["code"], "timeout");
        assert_eq!(out["errors"][0]["source"], source);
    }

    #[tokio::test]
    async fn request_deadline_covers_headers_and_body_and_keeps_additional_result() {
        let (origin, handle) = delayed_server(vec![
            delayed_body(
                fixture().to_string(),
                Duration::from_millis(400),
                Duration::from_millis(700),
            ),
            delayed_body(additional_body(), Duration::ZERO, Duration::ZERO),
        ]);
        let mut t = target(origin);
        t.request = Duration::from_secs(1);
        t.total = Duration::from_secs(4);
        let value = show_inner(&record(), "s-1", Some(t)).await;
        let (out, code) = crate::cli::courses_output(value);
        assert_timeout_partial(&out, code, "primary");
        assert_eq!(out["data"]["topic"], Value::Null);
        assert_eq!(out["data"]["primary_resources"], Value::Null);
        assert_eq!(
            out["data"]["additional_resources"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(out["coverage"]["additional"], "complete");
        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[0].starts_with(
                "GET /func-bm7-course-prod/ClassSession/Session/s-1/Resource/Student?"
            )
        );
    }

    #[tokio::test]
    async fn request_deadline_covers_trickled_body_and_keeps_additional_result() {
        let body = fixture().to_string().into_bytes();
        let cuts = [body.len() / 4, body.len() / 2, body.len() * 3 / 4];
        let mut start = 0;
        let chunks = cuts
            .into_iter()
            .chain(std::iter::once(body.len()))
            .map(|end| {
                let chunk = body[start..end].to_vec();
                start = end;
                chunk
            })
            .collect();
        let (origin, handle) = delayed_server(vec![
            delayed_chunks(chunks, Duration::from_millis(350)),
            delayed_body(additional_body(), Duration::ZERO, Duration::ZERO),
        ]);
        let mut t = target(origin);
        t.request = Duration::from_secs(1);
        t.total = Duration::from_secs(4);
        let value = show_inner(&record(), "s-1", Some(t)).await;
        let (out, code) = crate::cli::courses_output(value);
        assert_timeout_partial(&out, code, "primary");
        assert_eq!(out["data"]["topic"], Value::Null);
        assert_eq!(out["data"]["primary_resources"], Value::Null);
        assert_eq!(
            out["data"]["additional_resources"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(out["coverage"]["additional"], "complete");
        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 2);
    }

    #[tokio::test]
    async fn shared_total_deadline_covers_both_sources_and_retains_primary() {
        let (origin, handle) = delayed_server(vec![
            delayed_body(
                fixture().to_string(),
                Duration::ZERO,
                Duration::from_millis(900),
            ),
            delayed_body(
                additional_body(),
                Duration::ZERO,
                Duration::from_millis(1700),
            ),
        ]);
        let mut t = target(origin);
        t.request = Duration::from_secs(3);
        t.total = Duration::from_millis(2200);
        let started = Instant::now();
        let value = show_inner(&record(), "s-1", Some(t)).await;
        let elapsed = started.elapsed();
        let (out, code) = crate::cli::courses_output(value);
        assert_timeout_partial(&out, code, "additional");
        assert_eq!(out["data"]["topic"], "Topic text");
        assert_eq!(
            out["data"]["primary_resources"].as_array().unwrap().len(),
            1
        );
        assert_eq!(out["data"]["additional_resources"], Value::Null);
        assert_eq!(out["coverage"]["primary"], "complete");
        assert_eq!(out["coverage"]["additional"], "failed");
        assert!(
            elapsed >= Duration::from_millis(1900),
            "elapsed: {elapsed:?}"
        );
        assert!(elapsed < Duration::from_secs(4), "elapsed: {elapsed:?}");
        assert_eq!(handle.join().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn show_inner_observes_exactly_two_gets_and_never_fetches_embedded_url() {
        let (honeypot, hp_stop, hp_handle) = observer(vec![]);
        let mut primary = fixture();
        primary["resources"][0]["url"] = json!(honeypot);
        let (origin, stop, handle) = observer(vec![
            (200, primary.to_string(), String::new()),
            (200, additional_body(), String::new()),
        ]);
        let value = show_inner(&record(), "s-1", Some(target(origin))).await;
        let requests = stop_observer(stop, handle);
        let hp_requests = stop_observer(hp_stop, hp_handle);
        assert_eq!(requests.len(), 2, "observed requests: {requests:?}");
        assert!(requests[0].starts_with("GET /func-bm7-course-prod/ClassSession/Session/s-1/Resource/Student?isWeb=true HTTP/1.1"));
        assert!(requests[1].starts_with(
            "GET /func-bm7-course-prod/ClassSession/Session/s-1/AdditionalResources HTTP/1.1"
        ));
        for request in &requests {
            let lower = request.to_lowercase();
            for (header, value) in [
                ("academiccareer", "ugrd"),
                ("institution", "binus"),
                ("rolename", "student"),
                ("roleid", "student"),
                ("roid", "org"),
            ] {
                assert!(
                    lower.contains(&format!("{header}: {value}")),
                    "missing {header}: {request:?}"
                );
            }
            assert!(lower.contains("authorization: bearer synthetic-token"));
            assert!(!lower.contains("cookie:"));
            assert!(
                request.ends_with("\r\n\r\n"),
                "unexpected request body: {request:?}"
            );
        }
        assert!(
            hp_requests.is_empty(),
            "embedded URL was fetched: {hp_requests:?}"
        );
        let (out, code) = crate::cli::courses_output(value);
        assert_eq!(code, std::process::ExitCode::SUCCESS);
        assert_eq!(out["status"], "complete");
    }

    #[tokio::test]
    async fn primary_redirect_is_reported_without_following_or_forwarding_authorization() {
        let (destination, dest_stop, dest_handle) = observer(vec![]);
        let (origin, stop, handle) = observer(vec![
            (302, String::new(), format!("Location: {destination}\r\n")),
            (200, additional_body(), String::new()),
        ]);
        let value = show_inner(&record(), "s-1", Some(target(origin))).await;
        let requests = stop_observer(stop, handle);
        let dest_requests = stop_observer(dest_stop, dest_handle);
        assert_eq!(requests.len(), 2);
        assert_eq!(
            dest_requests.len(),
            0,
            "redirect destination received: {dest_requests:?}"
        );
        assert!(
            requests[0]
                .to_lowercase()
                .contains("authorization: bearer synthetic-token")
        );
        let (out, code) = crate::cli::courses_output(value);
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(out["status"], "partial");
        assert_eq!(out["errors"][0]["source"], "primary");
        assert_eq!(out["errors"][0]["code"], "upstream_error");
        assert_eq!(out["errors"][0]["http_status"], 302);
        assert_eq!(
            out["data"]["additional_resources"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(out["coverage"]["additional"], "complete");
    }

    #[tokio::test]
    async fn refused_additional_connection_keeps_valid_primary_result() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            // Closing the listening socket makes the subsequent additional connection fail,
            // while this accepted stream remains available for the valid primary response.
            drop(listener);
            reply(&mut stream, 200, "OK", &fixture().to_string(), "");
            request
        });
        let value = show_inner(&record(), "s-1", Some(target(format!("http://{addr}")))).await;
        let primary_request = handle.join().unwrap();
        assert!(primary_request.starts_with("GET /func-bm7-course-prod/ClassSession/Session/s-1/Resource/Student?isWeb=true HTTP/1.1"));
        let (out, code) = crate::cli::courses_output(value);
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(out["status"], "partial");
        assert_eq!(out["errors"][0]["source"], "additional");
        assert_eq!(out["errors"][0]["code"], "network_error");
        assert_eq!(out["data"]["topic"], "Topic text");
        assert_eq!(
            out["data"]["primary_resources"].as_array().unwrap().len(),
            1
        );
        assert_eq!(out["data"]["additional_resources"], Value::Null);
        assert_eq!(out["coverage"]["primary"], "complete");
        assert_eq!(out["coverage"]["additional"], "failed");
    }

    #[tokio::test]
    async fn actual_two_source_pipeline_is_safe_and_serializes_complete_result() {
        let p = fixture().to_string();
        let a=json!([{"id":"a","classSessionId":"s-1","resourceId":null,"name":"Extra","resourceType":"DOC","index":1,"tabIndex":0,"tabName":"Additional","type":null,"url":"https://signed.invalid/x","generatedDate":"raw","isOpen":true}]).to_string();
        let (origin, h) = server(vec![(200, p), (200, a)]);
        let value = show_inner(&record(), "s-1", Some(target(origin))).await;
        let requests = h.join().unwrap();
        assert!(requests[0].starts_with("GET /func-bm7-course-prod/ClassSession/Session/s-1/Resource/Student?isWeb=true HTTP/1.1"));
        assert!(requests[1].starts_with(
            "GET /func-bm7-course-prod/ClassSession/Session/s-1/AdditionalResources HTTP/1.1"
        ));
        for request in requests {
            assert!(
                request
                    .to_lowercase()
                    .contains("authorization: bearer synthetic-token")
            );
            assert!(!request.to_lowercase().contains("cookie:"));
            assert!(!request.to_lowercase().contains("secret"));
            assert!(
                request.ends_with("\r\n\r\n"),
                "request unexpectedly had a body: {request:?}"
            );
            for (header, value) in [
                ("academiccareer", "ugrd"),
                ("institution", "binus"),
                ("rolename", "student"),
                ("roleid", "student"),
                ("roid", "org"),
            ] {
                assert!(
                    request
                        .to_lowercase()
                        .contains(&format!("{header}: {value}")),
                    "missing {header}: {request:?}"
                );
            }
        }
        let (out, code) = crate::cli::courses_output(value);
        assert_eq!(code, std::process::ExitCode::SUCCESS);
        assert_eq!(out["status"], "complete");
        assert_eq!(out["data"]["session_id"], "s-1");
        assert_eq!(
            out["data"]["additional_resources"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn authorization_failures_are_distinct_from_upstream_and_keep_valid_source() {
        for (status, expected) in [
            (401, "reauth_required"),
            (403, "reauth_required"),
            (500, "upstream_error"),
        ] {
            let (origin, h) = server(vec![(200, fixture().to_string()), (status, "{}".into())]);
            let (out, code) = crate::cli::courses_output(
                show_inner(&record(), "s-1", Some(target(origin))).await,
            );
            assert_eq!(h.join().unwrap().len(), 2);
            assert_eq!(code, std::process::ExitCode::from(3));
            assert_eq!(out["status"], "partial");
            assert_eq!(out["errors"][0]["code"], expected);
            assert_eq!(out["errors"][0]["source"], "additional");
            assert_eq!(out["errors"][0]["http_status"], status);
            assert_eq!(out["data"]["topic"], "Topic text");
            assert_eq!(out["coverage"]["primary"], "complete");
            assert_eq!(out["coverage"]["additional"], "failed");
        }
    }

    #[tokio::test]
    async fn invalid_json_and_wrong_roots_preserve_the_other_source() {
        for (primary, additional) in [("{", "[]"), (fixture().to_string().leak(), "{}")] {
            let (origin, h) = server(vec![(200, primary.into()), (200, additional.into())]);
            let (out, code) = crate::cli::courses_output(
                show_inner(&record(), "s-1", Some(target(origin))).await,
            );
            assert_eq!(h.join().unwrap().len(), 2);
            assert_eq!(code, std::process::ExitCode::from(3));
            assert_eq!(out["status"], "partial");
            assert_eq!(out["coverage"]["complete"], false);
            assert_eq!(
                out["errors"][0]["code"],
                if primary == "{" {
                    "invalid_json"
                } else {
                    "invalid_schema"
                }
            );
        }
        let (origin, h) = server(vec![(200, fixture().to_string()), (200, "[{}]".into())]);
        let (out, code) =
            crate::cli::courses_output(show_inner(&record(), "s-1", Some(target(origin))).await);
        h.join().unwrap();
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(out["data"]["topic"], "Topic text");
        assert_eq!(out["errors"][0]["code"], "invalid_schema");
    }

    #[tokio::test]
    async fn declared_oversize_response_is_rejected_before_reading_body() {
        let (origin, h) = server(vec![(200, fixture().to_string()), (200, "x".repeat(2048))]);
        let mut t = target(origin);
        t.cap = 1024;
        let (out, code) = crate::cli::courses_output(show_inner(&record(), "s-1", Some(t)).await);
        h.join().unwrap();
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(out["status"], "partial");
        assert_eq!(out["errors"][0]["code"], "body_too_large");
        assert_eq!(out["errors"][0]["source"], "additional");
        assert_eq!(out["data"]["topic"], "Topic text");
        assert_eq!(out["coverage"]["primary"], "complete");
        assert_eq!(out["coverage"]["additional"], "failed");
        assert_eq!(out["coverage"]["complete"], false);
        assert_eq!(
            out["data"]["primary_resources"].as_array().unwrap().len(),
            1
        );
        assert_eq!(out["data"]["additional_resources"], Value::Null);
    }

    #[tokio::test]
    async fn streamed_oversize_response_is_rejected_after_cumulative_bytes_cross_cap() {
        let mut body = b"[]".to_vec();
        body.resize(2048, b' ');
        assert!(body.chunks(512).all(|chunk| chunk.len() < 1024));
        let (origin, handle) = streamed_oversize_server(fixture().to_string(), body);
        let mut t = target(origin);
        t.cap = 1024;
        let (out, code) = crate::cli::courses_output(show_inner(&record(), "s-1", Some(t)).await);
        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(out["status"], "partial");
        assert_eq!(out["errors"][0]["code"], "body_too_large");
        assert_eq!(out["errors"][0]["source"], "additional");
        assert_eq!(out["data"]["topic"], "Topic text");
        assert_eq!(
            out["data"]["primary_resources"].as_array().unwrap().len(),
            1
        );
        assert_eq!(out["data"]["additional_resources"], Value::Null);
        assert_eq!(out["coverage"]["primary"], "complete");
        assert_eq!(out["coverage"]["additional"], "failed");
        assert_eq!(out["coverage"]["complete"], false);
    }

    #[tokio::test]
    async fn partial_and_total_failures_keep_source_status_explicit() {
        let (origin, h) = server(vec![(503, "{}".into()), (200, "[]".into())]);
        let partial = show_inner(&record(), "s-1", Some(target(origin))).await;
        h.join().unwrap();
        let (out, code) = crate::cli::courses_output(partial);
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(out["status"], "partial");
        assert_eq!(out["data"]["topic"], Value::Null);
        assert_eq!(out["data"]["primary_resources"], Value::Null);
        assert_eq!(out["data"]["additional_resources"], json!([]));
        assert_eq!(out["errors"][0]["source"], "primary");
        assert_eq!(out["errors"][0]["http_status"], 503);
        assert_eq!(out["coverage"]["complete"], false);

        let (origin, h) = server(vec![(500, "{}".into()), (403, "{}".into())]);
        let failed = show_inner(&record(), "s-1", Some(target(origin))).await;
        h.join().unwrap();
        let (out, code) = crate::cli::courses_output(failed);
        assert_eq!(code, std::process::ExitCode::from(1));
        assert_eq!(out["status"], "error");
        assert_eq!(out["data"], Value::Null);
        assert_eq!(out["errors"][0]["code"], "upstream_error");
        assert_eq!(out["errors"][1]["code"], "reauth_required");
    }

    #[tokio::test]
    async fn partial_pipeline_retains_whichever_source_succeeded_and_redacts_bad_rows() {
        let additional = json!([{"id":"a","classSessionId":"s-1","resourceId":null,"name":"Extra","resourceType":"DOC","type":null,"index":0,"tabName":"More","isOpen":true,"url":null}]).to_string();
        let (origin, h) = server(vec![(200, fixture().to_string()), (500, "{}".into())]);
        let (out, code) =
            crate::cli::courses_output(show_inner(&record(), "s-1", Some(target(origin))).await);
        h.join().unwrap();
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(out["data"]["topic"], "Topic text");
        assert_eq!(
            out["data"]["primary_resources"].as_array().unwrap().len(),
            1
        );
        assert_eq!(out["data"]["additional_resources"], Value::Null);

        let mut malformed = fixture();
        malformed["resources"][0]["progressStatus"] =
            json!(["synthetic-secret", {"url":"https://signed.invalid/x"}]);
        let (origin, h) = server(vec![(200, malformed.to_string()), (200, additional)]);
        let (out, code) =
            crate::cli::courses_output(show_inner(&record(), "s-1", Some(target(origin))).await);
        h.join().unwrap();
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(out["data"]["topic"], Value::Null);
        assert_eq!(out["data"]["primary_resources"], Value::Null);
        assert_eq!(
            out["data"]["additional_resources"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let encoded = out.to_string();
        assert!(!encoded.contains("synthetic-secret"));
        assert!(!encoded.contains("signed.invalid"));
    }

    #[tokio::test]
    async fn empty_additional_and_zero_primary_resources_are_complete() {
        let (origin, h) = server(vec![(200, fixture().to_string()), (200, "[]".into())]);
        let (out, code) =
            crate::cli::courses_output(show_inner(&record(), "s-1", Some(target(origin))).await);
        h.join().unwrap();
        assert_eq!(code, std::process::ExitCode::SUCCESS);
        assert_eq!(out["status"], "complete");
        assert_eq!(out["data"]["additional_resources"], json!([]));

        let mut empty = fixture();
        empty["resources"] = json!([]);
        empty["totalResource"] = json!(0);
        let (origin, h) = server(vec![(200, empty.to_string()), (200, "[]".into())]);
        let (out, code) =
            crate::cli::courses_output(show_inner(&record(), "s-1", Some(target(origin))).await);
        h.join().unwrap();
        assert_eq!(code, std::process::ExitCode::SUCCESS);
        assert_eq!(out["status"], "complete");
        assert_eq!(out["data"]["primary_resources"], json!([]));
    }

    #[tokio::test]
    async fn count_disagreement_retains_valid_primary_and_reports_both_counts() {
        let mut primary = fixture();
        primary["totalResource"] = json!(2);
        let (origin, h) = server(vec![(200, primary.to_string()), (200, "[]".into())]);
        let (out, code) =
            crate::cli::courses_output(show_inner(&record(), "s-1", Some(target(origin))).await);
        assert_eq!(h.join().unwrap().len(), 2);
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(out["status"], "partial");
        assert_eq!(out["data"]["topic"], "Topic text");
        assert_eq!(
            out["data"]["primary_resources"].as_array().unwrap().len(),
            1
        );
        assert_eq!(out["errors"][0]["code"], "source_count_mismatch");
        assert_eq!(out["errors"][0]["source"], "primary");
        assert_eq!(out["errors"][0]["stage"], "primary");
        assert_eq!(out["errors"][0]["declared_total_resource"], 2);
        assert_eq!(out["errors"][0]["observed_resource_rows"], 1);
        assert_eq!(out["coverage"]["primary"], "partial");
        assert_eq!(out["coverage"]["complete"], false);
        assert_eq!(out["coverage"]["discovery_complete"], false);

        let mut primary = fixture();
        primary["totalResource"] = json!(0);
        let (origin, h) = server(vec![(200, primary.to_string()), (500, "{}".into())]);
        let (out, code) =
            crate::cli::courses_output(show_inner(&record(), "s-1", Some(target(origin))).await);
        h.join().unwrap();
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(out["status"], "partial");
        assert_eq!(out["data"]["topic"], "Topic text");
        assert_eq!(out["data"]["additional_resources"], Value::Null);
        assert_eq!(out["errors"][0]["declared_total_resource"], 0);
        assert_eq!(out["errors"][0]["observed_resource_rows"], 1);
        assert_eq!(out["errors"][1]["source"], "additional");
        assert_eq!(out["coverage"]["primary"], "partial");
        assert_eq!(out["coverage"]["additional"], "failed");

        let mut empty = fixture();
        empty["resources"] = json!([]);
        empty["totalResource"] = json!(1);
        let (origin, h) = server(vec![(200, empty.to_string()), (200, "[]".into())]);
        let (out, code) =
            crate::cli::courses_output(show_inner(&record(), "s-1", Some(target(origin))).await);
        h.join().unwrap();
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(out["data"]["topic"], "Topic text");
        assert_eq!(out["data"]["primary_resources"], json!([]));
        assert_eq!(out["errors"][0]["declared_total_resource"], 1);
        assert_eq!(out["errors"][0]["observed_resource_rows"], 0);

        let mut duplicates = fixture();
        duplicates["resources"] = json!([
            fixture()["resources"][0].clone(),
            fixture()["resources"][0].clone()
        ]);
        duplicates["totalResource"] = json!(2);
        let (origin, h) = server(vec![(200, duplicates.to_string()), (200, "[]".into())]);
        let (out, code) =
            crate::cli::courses_output(show_inner(&record(), "s-1", Some(target(origin))).await);
        h.join().unwrap();
        assert_eq!(code, std::process::ExitCode::SUCCESS);
        assert_eq!(out["status"], "complete");
        assert_eq!(
            out["data"]["primary_resources"].as_array().unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn malformed_primary_row_with_count_disagreement_still_fails_atomically() {
        let mut primary = fixture();
        primary["totalResource"] = json!(9);
        primary["resources"][0]["progressStatus"] = json!({"bad":"value"});
        let (origin, h) = server(vec![(200, primary.to_string()), (200, "[]".into())]);
        let (out, code) =
            crate::cli::courses_output(show_inner(&record(), "s-1", Some(target(origin))).await);
        h.join().unwrap();
        assert_eq!(code, std::process::ExitCode::from(3));
        assert_eq!(out["data"]["topic"], Value::Null);
        assert_eq!(out["data"]["primary_resources"], Value::Null);
        assert_eq!(out["data"]["additional_resources"], json!([]));
        assert_eq!(out["errors"][0]["code"], "invalid_schema");
        assert!(out["errors"][0].get("declared_total_resource").is_none());
    }
}
async fn show_inner(
    record: &CredentialRecord,
    session_id: &str,
    #[cfg(test)] target: Option<Target>,
    #[cfg(not(test))] _target: Option<()>,
) -> Value {
    let client = match reqwest::Client::builder().redirect(Policy::none()).build() {
        Ok(c) => c,
        Err(_) => {
            return envelope(
                session_id,
                SourceResult {
                    data: None,
                    error: Some(error_value(
                        session_id,
                        "client_setup_failed",
                        "client",
                        Some("primary"),
                        None,
                    )),
                    status: "failed",
                },
                SourceResult {
                    data: None,
                    error: Some(error_value(
                        session_id,
                        "client_setup_failed",
                        "client",
                        Some("additional"),
                        None,
                    )),
                    status: "failed",
                },
            );
        }
    };
    let started = Instant::now();
    #[cfg(test)]
    let target = target.unwrap_or(Target {
        origin: String::new(),
        total: TOTAL,
        request: Duration::from_secs(20),
        cap: BODY_CAP,
    });
    let primary_result = {
        #[cfg(test)]
        {
            get(record, session_id, "primary", &target, started, &client).await
        }
        #[cfg(not(test))]
        {
            get(record, session_id, "primary", started, &client).await
        }
    };
    let primary = match primary_result {
        Ok(root) => match map_primary(&root, session_id) {
            Ok(mut data) => {
                let declared = data
                    .as_object_mut()
                    .and_then(|object| object.remove("_declared_total_resource"));
                let observed = data
                    .as_object_mut()
                    .and_then(|object| object.remove("_observed_resource_rows"));
                if let (Some(declared), Some(observed)) = (declared, observed) {
                    let mut error = error_value(
                        session_id,
                        "source_count_mismatch",
                        "primary",
                        Some("primary"),
                        None,
                    );
                    error["declared_total_resource"] = declared;
                    error["observed_resource_rows"] = observed;
                    SourceResult {
                        data: Some(data),
                        error: Some(error),
                        status: "partial",
                    }
                } else {
                    SourceResult {
                        data: Some(data),
                        error: None,
                        status: "complete",
                    }
                }
            }
            Err(e) => SourceResult {
                data: None,
                error: Some(error_value(session_id, e, "primary", Some("primary"), None)),
                status: "failed",
            },
        },
        Err(e) => SourceResult {
            data: None,
            error: Some(e),
            status: "failed",
        },
    };
    let additional_result = {
        #[cfg(test)]
        {
            get(record, session_id, "additional", &target, started, &client).await
        }
        #[cfg(not(test))]
        {
            get(record, session_id, "additional", started, &client).await
        }
    };
    let additional = match additional_result {
        Ok(root) => match map_resources(&json!({"additionalResources":root}), session_id, false) {
            Ok((rows, _)) => SourceResult {
                data: Some(json!(rows)),
                error: None,
                status: "complete",
            },
            Err(e) => SourceResult {
                data: None,
                error: Some(error_value(
                    session_id,
                    e,
                    "additional",
                    Some("additional"),
                    None,
                )),
                status: "failed",
            },
        },
        Err(e) => SourceResult {
            data: None,
            error: Some(e),
            status: "failed",
        },
    };
    envelope(session_id, primary, additional)
}
