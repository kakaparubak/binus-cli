use crate::{CredentialRecord, academic_period_request, courses::timestamp};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
#[cfg(test)]
use url::Url;

const MAX_BODY: usize = 8 * 1024 * 1024;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Session {
    session_id: String,
    class_id: String,
    session_number: String,
    course_topic_id: String,
    date_end: Option<String>,
    source_date_end: String,
    has_forum_content: bool,
    source_status: i64,
}
#[derive(Serialize)]
struct ClassScope {
    class_id: String,
    status: &'static str,
}
#[derive(Serialize)]
struct Coverage {
    discovery_complete: bool,
    complete: bool,
    classes: Vec<ClassScope>,
}
#[derive(Serialize)]
struct ApiError {
    code: &'static str,
    stage: &'static str,
    class_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    http_status: Option<u16>,
}
#[derive(Serialize)]
pub struct Envelope {
    schema_version: &'static str,
    status: &'static str,
    data: Vec<Session>,
    coverage: Coverage,
    errors: Vec<ApiError>,
    warnings: Vec<String>,
    fetched_at: String,
}

#[cfg(test)]
#[derive(Clone)]
struct Target {
    origin: String,
    total: Duration,
    request: Duration,
    cap: usize,
}
fn error(
    class_id: &str,
    code: &'static str,
    stage: &'static str,
    http_status: Option<u16>,
) -> Envelope {
    Envelope {
        schema_version: "1",
        status: "error",
        data: vec![],
        coverage: Coverage {
            discovery_complete: false,
            complete: false,
            classes: vec![ClassScope {
                class_id: class_id.into(),
                status: "failed",
            }],
        },
        errors: vec![ApiError {
            code,
            stage,
            class_id: class_id.into(),
            http_status,
        }],
        warnings: vec![],
        fetched_at: timestamp(),
    }
}
fn map_sessions(root: &Value, class_id: &str) -> Result<Vec<Session>, &'static str> {
    fn pagination_indicator(value: &Value) -> Result<bool, &'static str> {
        let Some(object) = value.as_object() else {
            return Ok(false);
        };
        for (key, value) in object {
            match key.as_str() {
                "continuationToken" | "nextToken" | "nextPage" | "nextCursor" => {
                    if !value.is_null() && !value.as_str().is_some_and(str::is_empty) {
                        if value.as_str().is_none() {
                            return Err("invalid_schema");
                        }
                        return Err("unsupported_pagination");
                    }
                    if !value.is_null() && !value.is_string() {
                        return Err("invalid_schema");
                    }
                }
                "hasNext" | "hasMore" => match value.as_bool() {
                    Some(true) => return Err("unsupported_pagination"),
                    Some(false) => {}
                    None => return Err("invalid_schema"),
                },
                "pagination" | "paging" if !value.is_null() => {
                    if !value.is_object() {
                        return Err("invalid_schema");
                    }
                    if pagination_indicator(value)? {
                        return Err("unsupported_pagination");
                    }
                }
                "pagination" | "paging" => {}
                _ => {}
            }
        }
        Ok(false)
    }
    pagination_indicator(root)?;
    let rows = root
        .get("sessions")
        .and_then(Value::as_array)
        .ok_or("invalid_schema")?;
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut mapped = Vec::new();
    for row in rows {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .ok_or("invalid_schema")?;
        let number = row
            .get("classSessionNumber")
            .and_then(Value::as_str)
            .ok_or("invalid_schema")?;
        let topic = row
            .get("courseTopicId")
            .and_then(Value::as_str)
            .ok_or("invalid_schema")?;
        let raw_end = row
            .get("dateEnd")
            .and_then(Value::as_str)
            .ok_or("invalid_schema")?;
        let forum = row
            .get("hasForumContent")
            .and_then(Value::as_bool)
            .ok_or("invalid_schema")?;
        let status = row
            .get("status")
            .and_then(Value::as_i64)
            .ok_or("invalid_schema")?;
        let session = Session {
            session_id: id.into(),
            class_id: class_id.into(),
            session_number: number.into(),
            course_topic_id: topic.into(),
            date_end: if raw_end.starts_with("0001-01-01") {
                None
            } else {
                Some(raw_end.into())
            },
            source_date_end: raw_end.into(),
            has_forum_content: forum,
            source_status: status,
        };
        if let Some(index) = seen.get(id) {
            if mapped[*index] != session {
                return Err("conflicting_duplicate");
            }
        } else {
            seen.insert(id.into(), mapped.len());
            mapped.push(session);
        }
    }
    Ok(mapped)
}
fn request(
    record: &CredentialRecord,
    class_id: &str,
    #[cfg(test)] target: &Target,
) -> Result<reqwest::Request, &'static str> {
    if !crate::courses::safe_segment(class_id) {
        return Err("unsafe_identifier");
    }
    let mut req = academic_period_request(record)?;
    #[cfg(test)]
    {
        let u = Url::parse(&format!("{}/", target.origin)).map_err(|_| "request_error")?;
        if !u
            .host_str()
            .is_some_and(|h| h == "127.0.0.1" || h == "localhost" || h == "::1")
        {
            return Err("request_error");
        }
        *req.url_mut() = u;
    }
    let mut p = req
        .url_mut()
        .path_segments_mut()
        .map_err(|_| "request_error")?;
    p.clear();
    for s in [
        "func-bm7-course-prod",
        "ClassSession",
        "Class",
        class_id,
        "Student",
    ] {
        p.push(s);
    }
    drop(p);
    *req.method_mut() = reqwest::Method::GET;
    Ok(req)
}
pub async fn list(record: &CredentialRecord, class_id: &str) -> Envelope {
    list_inner(record, class_id, None).await
}
#[cfg(test)]
async fn list_test(record: &CredentialRecord, class_id: &str, target: Target) -> Envelope {
    list_inner(record, class_id, Some(target)).await
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::RoleContext;
    use serde_json::json;
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
    fn root(rows: Value) -> Value {
        json!({"sessions":rows,"lecturers":[{"id":"private","userCode":"private"}],"courseTitleEn":"secret title"})
    }
    #[test]
    fn named_sessions_shape_mapping_and_empty_are_exact() {
        let r = root(
            json!([{"id":"stable","classSessionNumber":"01A","courseTopicId":"topic","dateEnd":"0001-01-01T00:00:00","hasForumContent":true,"status":7}]),
        );
        let rows = map_sessions(&r, "class-guid").unwrap();
        assert_eq!(rows.len(), 1);
        let out = serde_json::to_value(&rows[0]).unwrap();
        assert_eq!(out["session_id"], "stable");
        assert_eq!(out["session_number"], "01A");
        assert_eq!(out["source_status"], 7);
        assert_eq!(out["date_end"], Value::Null);
        assert_eq!(out["source_date_end"], "0001-01-01T00:00:00");
        assert!(out.get("lecturers").is_none());
        assert!(out.get("courseTitleEn").is_none());
        assert!(map_sessions(&root(json!([])), "c").unwrap().is_empty());
        assert!(map_sessions(&json!([{"sessions":[]}]), "c").is_err());
        assert!(map_sessions(&json!({"lecturers":[],"items":[]}), "c").is_err());
        assert!(map_sessions(&json!({"sessions":{}}), "c").is_err());
    }
    #[test]
    fn duplicate_exact_dedupes_conflict_and_bad_row_fail_atomically() {
        let row = json!({"id":"x","classSessionNumber":"1","courseTopicId":"t","dateEnd":"2026-01-01T10:00:00","hasForumContent":false,"status":3});
        assert_eq!(
            map_sessions(&root(json!([row.clone(), row.clone()])), "c")
                .unwrap()
                .len(),
            1
        );
        let mut conflict = row.clone();
        conflict["status"] = json!(4);
        assert_eq!(
            map_sessions(&root(json!([row, conflict])), "c").unwrap_err(),
            "conflicting_duplicate"
        );
        assert!(map_sessions(&root(json!([{"id":"bad"}])), "c").is_err());
    }
    fn server(
        status: u16,
        body: &'static str,
        extra: &'static str,
    ) -> (String, thread::JoinHandle<String>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let a = l.local_addr().unwrap();
        let h = thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut b = [0; 8192];
            let n = s.read(&mut b).unwrap();
            let req = String::from_utf8_lossy(&b[..n]).to_string();
            let _ = write!(
                s,
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n{body}",
                body.len()
            );
            req
        });
        (format!("http://{a}"), h)
    }
    fn target(origin: String) -> Target {
        Target {
            origin,
            total: Duration::from_secs(2),
            request: Duration::from_secs(1),
            cap: MAX_BODY,
        }
    }
    fn session_body() -> &'static str {
        r#"{"sessions":[{"id":"synthetic-session","classSessionNumber":"07A","courseTopicId":"synthetic-topic","dateEnd":"2026-10-19T10:00:00","hasForumContent":true,"status":7}]}"#
    }
    fn chunked_server(
        chunks: Vec<&'static [u8]>,
        delay: Duration,
    ) -> (String, thread::JoinHandle<String>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let a = l.local_addr().unwrap();
        let h = thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut b = [0; 8192];
            let n = s.read(&mut b).unwrap();
            let req = String::from_utf8_lossy(&b[..n]).to_string();
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n");
            for chunk in chunks {
                thread::sleep(delay);
                let _ = write!(s, "{:X}\r\n", chunk.len());
                let _ = s.write_all(chunk);
                let _ = s.write_all(b"\r\n");
            }
            let _ = s.write_all(b"0\r\n\r\n");
            req
        });
        (format!("http://{a}"), h)
    }
    fn delayed_response_server(
        body: &'static str,
        header_delay: Duration,
        body_delay: Duration,
    ) -> (String, thread::JoinHandle<String>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let a = l.local_addr().unwrap();
        let h = thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut b = [0; 8192];
            let n = s.read(&mut b).unwrap();
            let req = String::from_utf8_lossy(&b[..n]).to_string();
            thread::sleep(header_delay);
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            thread::sleep(body_delay);
            let _ = s.write_all(body.as_bytes());
            req
        });
        (format!("http://{a}"), h)
    }
    #[tokio::test]
    async fn real_pipeline_builds_safe_get_and_serializes() {
        let body = r#"{"sessions":[],"lecturers":[{"id":"private"}]}"#;
        let (origin, h) = server(200, body, "");
        let out = list_test(&record(), "abc-GUID", target(origin)).await;
        let req = h.join().unwrap();
        assert!(
            req.starts_with(
                "GET /func-bm7-course-prod/ClassSession/Class/abc-GUID/Student HTTP/1.1"
            )
        );
        let low = req.to_lowercase();
        assert!(low.contains("authorization: bearer synthetic-token"));
        assert!(!low.contains("cookie:"));
        assert!(!req.contains('?'));
        assert!(req.ends_with("\r\n\r\n"));
        assert!(req.ends_with("\r\n\r\n"));
        let v = serde_json::to_value(out).unwrap();
        assert_eq!(v["status"], "complete");
        assert_eq!(v["coverage"]["classes"][0]["class_id"], "abc-GUID");
    }
    #[tokio::test]
    async fn actual_pipeline_maps_session_and_cli_serializes_class_scoped_success() {
        let (origin, h) = server(200, session_body(), "");
        let envelope = list_test(&record(), "class-synthetic", target(origin)).await;
        let req = h.join().unwrap().to_lowercase();
        for (header, value) in [
            ("roid", "org"),
            ("academiccareer", "ugrd"),
            ("institution", "binus"),
            ("rolename", "student"),
            ("roleid", "student"),
        ] {
            assert!(
                req.contains(&format!("{header}: {value}")),
                "missing {header}"
            );
        }
        assert!(req.contains("authorization: bearer synthetic-token"));
        assert!(!req.contains("cookie:"));
        assert!(!req.contains('?'));
        let (out, code) = crate::cli::courses_output(envelope);
        assert_eq!(code, std::process::ExitCode::SUCCESS);
        assert_eq!(out["data"][0]["session_id"], "synthetic-session");
        assert_eq!(out["data"][0]["class_id"], "class-synthetic");
        assert_eq!(out["data"][0]["source_status"], 7);
        assert_eq!(out["data"][0]["source_date_end"], "2026-10-19T10:00:00");
        assert_eq!(out["coverage"]["complete"], true);
        assert_eq!(out["coverage"]["classes"][0]["class_id"], "class-synthetic");
    }
    #[tokio::test]
    async fn unsupported_pagination_is_sanitized_and_empty_indicators_complete() {
        for body in [
            r#"{"sessions":[],"continuationToken":"SYNTHETIC-SECRET-PAGE-TOKEN"}"#,
            r#"{"sessions":[],"hasMore":true}"#,
            r#"{"sessions":[],"pagination":{"nextCursor":"synthetic-cursor"}}"#,
        ] {
            let (origin, h) = server(200, body, "");
            let envelope = list_test(&record(), "c", target(origin)).await;
            h.join().unwrap();
            let (out, code) = crate::cli::courses_output(envelope);
            assert_eq!(code, std::process::ExitCode::from(1));
            assert_eq!(out["status"], "error");
            assert_eq!(out["errors"][0]["code"], "unsupported_pagination");
            assert_eq!(out["coverage"]["complete"], false);
            assert_eq!(out["coverage"]["classes"][0]["status"], "failed");
            assert!(!out.to_string().contains("SYNTHETIC-SECRET-PAGE-TOKEN"));
            assert!(!out.to_string().contains("synthetic-cursor"));
        }
        for body in [
            r#"{"sessions":[],"continuationToken":""}"#,
            r#"{"sessions":[],"nextToken":null,"hasMore":false,"hasNext":false}"#,
        ] {
            let (origin, h) = server(200, body, "");
            let envelope = list_test(&record(), "c", target(origin)).await;
            h.join().unwrap();
            let (out, code) = crate::cli::courses_output(envelope);
            assert_eq!(code, std::process::ExitCode::SUCCESS);
            assert_eq!(out["coverage"]["complete"], true);
        }
        assert_eq!(
            map_sessions(&json!({"sessions":[],"hasMore":"yes"}), "c").unwrap_err(),
            "invalid_schema"
        );
        assert_eq!(
            map_sessions(
                &json!({"sessions":[],"classNumber":3,"sessionNumber":"x"}),
                "c"
            )
            .unwrap()
            .len(),
            0
        );
    }
    #[tokio::test]
    async fn cumulative_header_and_body_time_exceed_one_request_deadline() {
        let (origin, h) = delayed_response_server(
            session_body(),
            Duration::from_millis(145),
            Duration::from_millis(145),
        );
        let mut t = target(origin);
        t.request = Duration::from_millis(240);
        let envelope = list_test(&record(), "c", t).await;
        h.join().unwrap();
        let (out, code) = crate::cli::courses_output(envelope);
        assert_eq!(code, std::process::ExitCode::from(1));
        assert_eq!(out["errors"][0]["code"], "timeout");

        let body = session_body().as_bytes();
        let (origin, h) =
            chunked_server(vec![&body[..40], &body[40..]], Duration::from_millis(145));
        let mut t = target(origin);
        t.request = Duration::from_millis(240);
        let envelope = list_test(&record(), "c", t).await;
        h.join().unwrap();
        let (out, code) = crate::cli::courses_output(envelope);
        assert_eq!(code, std::process::ExitCode::from(1));
        assert_eq!(out["errors"][0]["code"], "timeout");
    }
    #[tokio::test]
    async fn chunked_body_without_content_length_is_capped() {
        let (origin, h) = chunked_server(
            vec![b"{\"sessions\":[]}", b"                    "],
            Duration::ZERO,
        );
        let mut t = target(origin);
        t.cap = 20;
        let envelope = list_test(&record(), "c", t).await;
        h.join().unwrap();
        let (out, code) = crate::cli::courses_output(envelope);
        assert_eq!(code, std::process::ExitCode::from(1));
        assert_eq!(out["errors"][0]["code"], "body_too_large");
        assert_eq!(out["coverage"]["complete"], false);
    }
    #[tokio::test]
    async fn malformed_later_row_fails_atomically_through_service_and_cli() {
        let body = r#"{"sessions":[{"id":"ok","classSessionNumber":"1","courseTopicId":"t","dateEnd":"x","hasForumContent":false,"status":1},{"id":"bad"}]}"#;
        let (origin, h) = server(200, body, "");
        let envelope = list_test(&record(), "c", target(origin)).await;
        h.join().unwrap();
        let (out, code) = crate::cli::courses_output(envelope);
        assert_eq!(code, std::process::ExitCode::from(1));
        assert_eq!(out["errors"][0]["code"], "invalid_schema");
        assert!(out["data"].as_array().unwrap().is_empty());
        assert_eq!(out["coverage"]["classes"][0]["status"], "failed");
    }
    #[tokio::test]
    async fn rejects_non_loopback_before_connect_and_sanitizes_closed_port() {
        let mut t = target("http://192.0.2.1:80".into());
        t.request = Duration::from_millis(500);
        let (out, code) = crate::cli::courses_output(list_test(&record(), "c", t).await);
        assert_eq!(code, std::process::ExitCode::from(1));
        assert_eq!(out["errors"][0]["code"], "request_error");

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        // Connection refusal can take several seconds on Windows. Keep this
        // classification fixture separate from the intentional timeout tests.
        let mut refused = target(format!("http://{addr}"));
        refused.total = Duration::from_secs(20);
        refused.request = Duration::from_secs(10);
        let (out, code) = crate::cli::courses_output(list_test(&record(), "c", refused).await);
        assert_eq!(code, std::process::ExitCode::from(1));
        assert_eq!(out["errors"][0]["code"], "network_error");
    }
    #[tokio::test]
    async fn authorization_and_upstream_errors_are_distinct() {
        for (status, code) in [
            (401, "reauth_required"),
            (403, "reauth_required"),
            (500, "upstream_error"),
        ] {
            let (origin, h) = server(status, "{}", "");
            let out = list_test(&record(), "c", target(origin)).await;
            h.join().unwrap();
            assert_eq!(
                serde_json::to_value(out).unwrap()["errors"][0]["code"],
                code
            );
        }
    }
    #[tokio::test]
    async fn redirect_is_not_followed_and_token_is_not_forwarded() {
        let (origin, h) = server(302, "{}", "Location: http://127.0.0.1:9/steal\r\n");
        let out = list_test(&record(), "c", target(origin)).await;
        let req = h.join().unwrap();
        assert_eq!(
            serde_json::to_value(out).unwrap()["errors"][0]["code"],
            "upstream_error"
        );
        assert!(!req.to_lowercase().contains("cookie:"));
    }
    #[tokio::test]
    async fn invalid_json_body_cap_and_request_timeout_are_sanitized() {
        for (body, cap, expected) in [
            ("not-json", MAX_BODY, "invalid_json"),
            ("{}", 1, "body_too_large"),
        ] {
            let (origin, h) = server(200, body, "");
            let mut t = target(origin);
            t.cap = cap;
            let out = list_test(&record(), "c", t).await;
            h.join().unwrap();
            assert_eq!(
                serde_json::to_value(out).unwrap()["errors"][0]["code"],
                expected
            );
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let h = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut b = [0; 8192];
            let _ = s.read(&mut b);
            thread::sleep(Duration::from_millis(200));
            let _ =
                s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
        });
        let mut t = target(format!("http://{addr}"));
        t.request = Duration::from_millis(50);
        let out = list_test(&record(), "c", t).await;
        h.join().unwrap();
        assert_eq!(
            serde_json::to_value(out).unwrap()["errors"][0]["code"],
            "timeout"
        );
    }
}
async fn list_inner(
    record: &CredentialRecord,
    class_id: &str,
    #[cfg(test)] target: Option<Target>,
    #[cfg(not(test))] _target: Option<()>,
) -> Envelope {
    if !crate::courses::safe_segment(class_id) {
        return error(class_id, "unsafe_identifier", "request", None);
    }
    let built = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(_) => return error(class_id, "client_setup_failed", "client", None),
    };
    let started = Instant::now();
    #[cfg(test)]
    let (budget, request_cap, body_cap) = target.as_ref().map_or(
        (Duration::from_secs(30), Duration::from_secs(20), MAX_BODY),
        |t| (t.total, t.request, t.cap),
    );
    #[cfg(not(test))]
    let (budget, request_cap, body_cap) =
        (Duration::from_secs(30), Duration::from_secs(20), MAX_BODY);
    #[cfg(test)]
    let req = request(
        record,
        class_id,
        target.as_ref().unwrap_or(&Target {
            origin: String::new(),
            total: budget,
            request: request_cap,
            cap: body_cap,
        }),
    );
    #[cfg(not(test))]
    let req = request(record, class_id);
    let req = match req {
        Ok(v) => v,
        Err(c) => return error(class_id, c, "request", None),
    };
    let deadline = started + budget;
    let request_deadline = (Instant::now() + request_cap).min(deadline);
    let response = match tokio::time::timeout_at(request_deadline.into(), built.execute(req)).await
    {
        Ok(Ok(r)) => r,
        Ok(Err(_)) => return error(class_id, "network_error", "request", None),
        Err(_) => return error(class_id, "timeout", "request", None),
    };
    let code = response.status().as_u16();
    if code == 401 || code == 403 {
        return error(class_id, "reauth_required", "request", Some(code));
    }
    if code != 200 {
        return error(class_id, "upstream_error", "request", Some(code));
    }
    if response
        .content_length()
        .is_some_and(|n| n > body_cap as u64)
    {
        return error(class_id, "body_too_large", "response", None);
    }
    let mut response = response;
    let mut bytes = Vec::new();
    loop {
        let chunk = match tokio::time::timeout_at(request_deadline.into(), response.chunk()).await {
            Ok(Ok(Some(chunk))) => chunk,
            Ok(Ok(None)) => break,
            Ok(Err(_)) => return error(class_id, "response_error", "response", None),
            Err(_) => return error(class_id, "timeout", "response", None),
        };
        if bytes.len().saturating_add(chunk.len()) > body_cap {
            return error(class_id, "body_too_large", "response", None);
        }
        bytes.extend_from_slice(&chunk);
    }
    let root: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return error(class_id, "invalid_json", "response", None),
    };
    match map_sessions(&root, class_id) {
        Ok(data) => Envelope {
            schema_version: "1",
            status: "complete",
            data,
            coverage: Coverage {
                discovery_complete: true,
                complete: true,
                classes: vec![ClassScope {
                    class_id: class_id.into(),
                    status: "complete",
                }],
            },
            errors: vec![],
            warnings: vec![],
            fetched_at: timestamp(),
        },
        Err(code) => error(class_id, code, "sessions", None),
    }
}
