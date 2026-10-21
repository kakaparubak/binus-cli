use binus_auth_spike::{CredentialRecord, academic_period_request, load_preferred_record};
use reqwest::{Method, Request, Response, Url};
use serde_json::{Value, json};
use std::{
    error::Error,
    io,
    path::PathBuf,
    process::ExitCode,
    time::{Duration, Instant},
};

const ORIGIN: &str = "https://apim-bm7-prod-web.azure-api.net/";
const SERVICE: &str = "func-bm7-course-prod";
const TOTAL: Duration = Duration::from_secs(60);
const REQUEST: Duration = Duration::from_secs(20);
const MAX_BODY: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Construction {
    Inherited,
    Reset,
}

#[derive(Debug)]
struct Args {
    class_id: String,
    construction: Construction,
    details: bool,
}

fn credentials_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    base.join("binus-auth-spike").join("credentials.json")
}

fn safe_segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn parse_args<I>(args: I) -> Result<Option<Args>, &'static str>
where
    I: IntoIterator<Item = String>,
{
    let mut iter = args.into_iter();
    let mut class = None;
    let mut construction = Construction::Inherited;
    let mut details = false;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--class" if class.is_none() => {
                let id = iter.next().ok_or("usage")?;
                if !safe_segment(&id) {
                    return Err("usage");
                }
                class = Some(id);
            }
            "--construction" => {
                construction = match iter.next().as_deref() {
                    Some("inherited") => Construction::Inherited,
                    Some("reset") => Construction::Reset,
                    _ => return Err("usage"),
                };
            }
            "--details" if !details => details = true,
            _ => return Err("usage"),
        }
    }
    Ok(Some(Args {
        class_id: class.ok_or("usage")?,
        construction,
        details,
    }))
}

fn make_request(
    record: &CredentialRecord,
    id: &str,
    construction: Construction,
    endpoint: &str,
) -> Result<Request, &'static str> {
    if !safe_segment(id) {
        return Err("invalid_identifier");
    }
    let mut req = academic_period_request(record).map_err(|_| "request_build_failed")?;
    *req.method_mut() = Method::GET;
    if construction == Construction::Reset {
        *req.url_mut() = Url::parse(ORIGIN).map_err(|_| "request_build_failed")?;
    }
    let mut segments = req
        .url_mut()
        .path_segments_mut()
        .map_err(|_| "request_build_failed")?;
    segments.clear();
    match endpoint {
        "class" => {
            for segment in [SERVICE, "ClassSession", "Class", id, "Student"] {
                segments.push(segment);
            }
        }
        "primary" => {
            for segment in [
                SERVICE,
                "ClassSession",
                "Session",
                id,
                "Resource",
                "Student",
            ] {
                segments.push(segment);
            }
        }
        "additional" => {
            for segment in [
                SERVICE,
                "ClassSession",
                "Session",
                id,
                "AdditionalResources",
            ] {
                segments.push(segment);
            }
        }
        _ => return Err("request_build_failed"),
    }
    drop(segments);
    req.url_mut()
        .set_query((endpoint == "primary").then_some("isWeb=true"));
    Ok(req)
}

fn request_comparison(a: &Request, b: &Request) -> Value {
    json!({
        "url_equal": a.url() == b.url(),
        "method_equal": a.method() == b.method(),
        "headers_equal": a.headers() == b.headers()
    })
}

fn error_projection(error: &reqwest::Error) -> Value {
    let mut io_kind = None;
    let mut depth = 0u8;
    let mut source: Option<&(dyn Error + 'static)> = Some(error);
    while let Some(item) = source {
        depth = depth.saturating_add(1).min(8);
        if io_kind.is_none()
            && let Some(io_error) = item.downcast_ref::<io::Error>()
        {
            io_kind = Some(format!("{:?}", io_error.kind()));
        }
        if depth == 8 {
            break;
        }
        source = item.source();
    }
    json!({
        "is_timeout": error.is_timeout(), "is_connect": error.is_connect(),
        "is_request": error.is_request(), "is_body": error.is_body(),
        "is_decode": error.is_decode(), "is_builder": error.is_builder(),
        "is_redirect": error.is_redirect(), "is_status": error.is_status(),
        "io_error_kind": io_kind, "error_chain_depth": depth
    })
}

fn error_row(stage: &'static str, elapsed: u128, error: &reqwest::Error) -> Value {
    json!({"stage":stage,"elapsed_ms":elapsed,"status":"transport_error","error_categories":error_projection(error)})
}

fn stage_error(stage: &'static str, elapsed: u128, category: &'static str) -> Value {
    json!({"stage":stage,"elapsed_ms":elapsed,"status":category})
}

async fn fetch(
    record: &CredentialRecord,
    id: &str,
    endpoint: &'static str,
    construction: Construction,
    start: Instant,
    client: &reqwest::Client,
) -> (Value, Option<Value>) {
    let stage = match endpoint {
        "class" => "class",
        "primary" => "primary",
        _ => "additional",
    };
    let req = match make_request(record, id, construction, endpoint) {
        Ok(req) => req,
        Err(_) => return (stage_error(stage, 0, "request_build_failed"), None),
    };
    let request_start = Instant::now();
    let remain = TOTAL
        .saturating_sub(start.elapsed())
        .min(REQUEST.saturating_sub(request_start.elapsed()));
    if remain.is_zero() {
        return (stage_error(stage, 0, "total_budget_exhausted"), None);
    }
    let began = Instant::now();
    let response = match tokio::time::timeout(remain, client.execute(req)).await {
        Err(_) => {
            return (
                stage_error("execute", began.elapsed().as_millis(), "timeout"),
                None,
            );
        }
        Ok(Err(error)) => {
            return (
                error_row("execute", began.elapsed().as_millis(), &error),
                None,
            );
        }
        Ok(Ok(response)) => response,
    };
    let headers_ms = began.elapsed().as_millis();
    let code = response.status().as_u16();
    if !(200..300).contains(&code) {
        let status_class = if code == 401 || code == 403 {
            "auth_error"
        } else {
            "http_error"
        };
        return (
            json!({"stage":stage,"execute_elapsed_ms":headers_ms,"http_status":code,"status":status_class,"body_bytes":0}),
            None,
        );
    }
    let (body_row, value) = read_body(response, stage, headers_ms, start, request_start).await;
    (body_row, value)
}

async fn read_body(
    mut response: Response,
    stage: &'static str,
    execute_ms: u128,
    start: Instant,
    request_start: Instant,
) -> (Value, Option<Value>) {
    let code = response.status().as_u16();
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BODY as u64)
    {
        return (
            json!({"stage":stage,"execute_elapsed_ms":execute_ms,"http_status":code,"body_elapsed_ms":0,"status":"body_too_large","body_bytes":0}),
            None,
        );
    }
    let began = Instant::now();
    let mut bytes = Vec::new();
    loop {
        let remain = TOTAL
            .saturating_sub(start.elapsed())
            .min(REQUEST.saturating_sub(request_start.elapsed()));
        if remain.is_zero() {
            return (
                json!({"stage":stage,"execute_elapsed_ms":execute_ms,"http_status":code,"body_elapsed_ms":began.elapsed().as_millis(),"status":"timeout","body_bytes":bytes.len()}),
                None,
            );
        }
        match tokio::time::timeout(remain, response.chunk()).await {
            Err(_) => {
                return (
                    json!({"stage":stage,"execute_elapsed_ms":execute_ms,"http_status":code,"body_elapsed_ms":began.elapsed().as_millis(),"status":"timeout","body_bytes":bytes.len()}),
                    None,
                );
            }
            Ok(Err(error)) => {
                return (
                    json!({"stage":stage,"execute_elapsed_ms":execute_ms,"http_status":code,"body_elapsed_ms":began.elapsed().as_millis(),"status":"transport_error","error_categories":error_projection(&error),"body_bytes":bytes.len()}),
                    None,
                );
            }
            Ok(Ok(None)) => break,
            Ok(Ok(Some(chunk))) => {
                if bytes.len().saturating_add(chunk.len()) > MAX_BODY {
                    return (
                        json!({"stage":stage,"execute_elapsed_ms":execute_ms,"http_status":code,"body_elapsed_ms":began.elapsed().as_millis(),"status":"body_too_large","body_bytes":bytes.len()}),
                        None,
                    );
                }
                bytes.extend_from_slice(&chunk);
            }
        }
    }
    let count = bytes.len();
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) => (
            json!({"stage":stage,"execute_elapsed_ms":execute_ms,"http_status":code,"body_elapsed_ms":began.elapsed().as_millis(),"status":"complete","body_bytes":count}),
            Some(value),
        ),
        Err(_) => (
            json!({"stage":stage,"execute_elapsed_ms":execute_ms,"http_status":code,"body_elapsed_ms":began.elapsed().as_millis(),"status":"invalid_json","body_bytes":count}),
            None,
        ),
    }
}

fn first_session_id(value: &Value) -> Option<&str> {
    value
        .get("sessions")?
        .as_array()?
        .iter()
        .find_map(|row| row.get("id")?.as_str().filter(|id| safe_segment(id)))
}

fn primary_counts(value: &Value) -> Value {
    let declared_total_resource = value.get("totalResource").and_then(Value::as_u64);
    let raw_resource_rows = value
        .get("resources")
        .and_then(Value::as_array)
        .and_then(|rows| u64::try_from(rows.len()).ok());
    let count_matches = declared_total_resource
        .zip(raw_resource_rows)
        .map(|(declared, rows)| declared == rows);
    json!({
        "declared_total_resource": declared_total_resource,
        "raw_resource_rows": raw_resource_rows,
        "count_matches": count_matches
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(None) => {
            println!(
                "{}",
                json!({"command":"transport_diagnostic","usage":"transport_diagnostic --class ID [--construction inherited|reset] [--details]"})
            );
            return ExitCode::SUCCESS;
        }
        Ok(Some(args)) => args,
        Err(_) => {
            println!(
                "{}",
                json!({"status":"error","error":"usage_or_invalid_class"})
            );
            return ExitCode::from(2);
        }
    };
    let (record, _) = match load_preferred_record(&credentials_path()) {
        Ok(value) => value,
        Err(_) => {
            println!(
                "{}",
                json!({"status":"error","error":"credentials_unavailable"})
            );
            return ExitCode::from(1);
        }
    };
    let other = if args.construction == Construction::Inherited {
        Construction::Reset
    } else {
        Construction::Inherited
    };
    let comparison = ["class", "primary", "additional"]
        .into_iter()
        .map(|endpoint| {
            let a = make_request(&record, &args.class_id, args.construction, endpoint);
            let b = make_request(&record, &args.class_id, other, endpoint);
            let result = match (a, b) {
                (Ok(a), Ok(b)) => request_comparison(&a, &b),
                _ => json!({"url_equal":false,"method_equal":false,"headers_equal":false}),
            };
            (endpoint.to_owned(), result)
        })
        .collect::<serde_json::Map<_, _>>();
    let construction = match args.construction {
        Construction::Inherited => "inherited",
        Construction::Reset => "reset",
    };
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(REQUEST)
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            println!(
                "{}",
                json!({"status":"error","error":"client_build_failed"})
            );
            return ExitCode::from(1);
        }
    };
    let started = Instant::now();
    let (class_row, class_value) = fetch(
        &record,
        &args.class_id,
        "class",
        args.construction,
        started,
        &client,
    )
    .await;
    let mut rows = vec![class_row];
    let mut details_status = if args.details {
        "skipped_class_failure"
    } else {
        "not_requested"
    };
    if args.details {
        let session_id = class_value
            .filter(|_| rows[0]["status"] == "complete")
            .and_then(|class_json| first_session_id(&class_json).map(str::to_owned));
        if let Some(session_id) = session_id {
            details_status = "attempted";
            let (mut primary, primary_value) = fetch(
                &record,
                &session_id,
                "primary",
                args.construction,
                started,
                &client,
            )
            .await;
            if primary["status"] == "complete"
                && let Some(value) = primary_value.as_ref()
            {
                primary["source_counts"] = primary_counts(value);
            }
            rows.push(primary);
            let (additional, _) = fetch(
                &record,
                &session_id,
                "additional",
                args.construction,
                started,
                &client,
            )
            .await;
            rows.push(additional);
        } else if rows[0]["status"] == "complete" {
            details_status = "skipped_no_valid_session_id";
        }
    }
    let complete = rows.iter().all(|r| r["status"] == "complete");
    let class_failed = rows.first().is_some_and(|r| r["status"] != "complete");
    let status = if complete {
        "complete"
    } else if class_failed && !args.details {
        "failure"
    } else {
        "partial"
    };
    println!(
        "{}",
        json!({"status":status,"construction":construction,"request_comparisons_equal":comparison,"requests":rows,"details":details_status,"total_elapsed_ms":started.elapsed().as_millis()})
    );
    if status == "complete" {
        ExitCode::SUCCESS
    } else if status == "failure" {
        ExitCode::from(1)
    } else {
        ExitCode::from(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> CredentialRecord {
        CredentialRecord {
            account: "SYNTHETIC_ACCOUNT".into(),
            access_token: "SYNTHETIC_SECRET".into(),
            refresh_token: "SYNTHETIC_REFRESH".into(),
            role: binus_auth_spike::RoleContext {
                academic_career: "SYNTHETIC_CAREER".into(),
                institution: "SYNTHETIC_INST".into(),
                name: "SYNTHETIC_ROLE".into(),
                role_id: "SYNTHETIC_ROLE_ID".into(),
                organization_id: "SYNTHETIC_ORG".into(),
            },
        }
    }

    #[test]
    fn argument_validation_and_defaults() {
        assert!(parse_args(vec!["--class".into(), "../x".into()]).is_err());
        assert!(
            parse_args(vec![
                "--class".into(),
                "abc".into(),
                "--construction".into(),
                "unknown".into()
            ])
            .is_err()
        );
        let parsed = parse_args(vec!["--class".into(), "abc".into()])
            .unwrap()
            .unwrap();
        assert_eq!(parsed.construction, Construction::Inherited);
        assert!(!parsed.details);
    }

    #[test]
    fn compare_exact_request_fields_and_never_serialize_credentials() {
        let record = record();
        for endpoint in ["class", "primary", "additional"] {
            let inherited =
                make_request(&record, "abc", Construction::Inherited, endpoint).unwrap();
            let reset = make_request(&record, "abc", Construction::Reset, endpoint).unwrap();
            let comparison = request_comparison(&inherited, &reset);
            assert_eq!(
                comparison,
                json!({"url_equal":true,"method_equal":true,"headers_equal":true})
            );
            let projection = json!({"url_equal":inherited.url()==reset.url(),"method_equal":inherited.method()==reset.method(),"headers_equal":inherited.headers()==reset.headers()}).to_string();
            for secret in [
                "SYNTHETIC_SECRET",
                "SYNTHETIC_REFRESH",
                "SYNTHETIC_ACCOUNT",
                "SYNTHETIC_ORG",
            ] {
                assert!(!projection.contains(secret));
            }
        }
    }

    #[test]
    fn only_safe_session_id_is_selected() {
        let value = json!({"sessions":[{"id":"bad/id"},{"id":"valid_1"}]});
        assert_eq!(first_session_id(&value), Some("valid_1"));
    }

    #[test]
    fn primary_count_projection_is_numeric_and_excludes_response_content() {
        let value = json!({
            "totalResource": 9,
            "resources": [{"title":"SYNTHETIC_TITLE_SECRET", "url":"https://synthetic.invalid/SECRET"}],
            "nested_secret": "SYNTHETIC_NESTED_SECRET"
        });
        let projection = primary_counts(&value);
        assert_eq!(
            projection,
            json!({
                "declared_total_resource": 9,
                "raw_resource_rows": 1,
                "count_matches": false
            })
        );
        let serialized = projection.to_string();
        for marker in [
            "SYNTHETIC_TITLE_SECRET",
            "https://synthetic.invalid/SECRET",
            "SYNTHETIC_NESTED_SECRET",
        ] {
            assert!(!serialized.contains(marker));
        }
    }

    #[test]
    fn reqwest_error_projection_contains_categories_but_no_error_text() {
        let error = reqwest::Client::new()
            .get("http://[SYNTHETIC_SECRET")
            .build()
            .unwrap_err();
        let projection = error_projection(&error).to_string();
        assert!(projection.contains("is_builder"));
        assert!(!projection.contains("SYNTHETIC_SECRET"));
        assert!(!projection.contains(&error.to_string()));
    }
}
