use binus_auth_spike::{academic_period_request, load_preferred_record};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    process::ExitCode,
    time::{Duration, Instant},
};
use url::Url;

const ORIGIN: &str = "https://apim-bm7-prod-web.azure-api.net";
const SERVICE: &str = "func-bm7-course-prod";
const TOTAL: Duration = Duration::from_secs(30);
const MAX_BODY: usize = 8 * 1024 * 1024;

fn credentials_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    base.join("binus-auth-spike").join("credentials.json")
}

fn safe_session_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Primary,
    Additional,
}

fn request(
    record: &binus_auth_spike::CredentialRecord,
    session_id: &str,
    kind: Kind,
) -> Result<reqwest::Request, &'static str> {
    if !safe_session_id(session_id) {
        return Err("invalid_session_id");
    }
    let mut req = academic_period_request(record).map_err(|_| "request_failed")?;
    *req.method_mut() = reqwest::Method::GET;
    let url = req.url_mut();
    *url = Url::parse(&format!("{ORIGIN}/")).map_err(|_| "request_failed")?;
    {
        let mut path = url.path_segments_mut().map_err(|_| "request_failed")?;
        path.clear();
        let tail = match kind {
            Kind::Primary => "Resource/Student",
            Kind::Additional => "AdditionalResources",
        };
        for segment in [SERVICE, "ClassSession", "Session", session_id]
            .into_iter()
            .chain(tail.split('/'))
        {
            path.push(segment);
        }
    }
    url.set_query(if kind == Kind::Primary {
        Some("isWeb=true")
    } else {
        None
    });
    Ok(req)
}

fn ty(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn schema(v: &Value) -> Value {
    fn walk(
        value: &Value,
        path: &str,
        fields: &mut BTreeMap<String, BTreeSet<String>>,
        arrays: &mut BTreeMap<String, (BTreeSet<usize>, BTreeSet<String>)>,
    ) {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    let child_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    fields
                        .entry(child_path.clone())
                        .or_default()
                        .insert(ty(child).to_string());
                    walk(child, &child_path, fields, arrays);
                }
            }
            Value::Array(items) => {
                arrays
                    .entry(path.to_owned())
                    .or_default()
                    .0
                    .insert(items.len());
                for item in items {
                    arrays
                        .entry(path.to_owned())
                        .or_default()
                        .1
                        .insert(ty(item).to_owned());
                    walk(item, &format!("{path}[]"), fields, arrays);
                }
            }
            _ => {}
        }
    }

    let mut fields = BTreeMap::new();
    let mut arrays = BTreeMap::new();
    walk(v, "$", &mut fields, &mut arrays);

    let root_type = ty(v);
    let root_count = match v {
        Value::Array(a) => Some(a.len()),
        Value::Object(o) => Some(o.len()),
        _ => None,
    };
    let root_array_counts = v
        .as_object()
        .map(|o| {
            o.iter()
                .filter_map(|(k, x)| x.as_array().map(|a| (k.clone(), a.len())))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let pagination = fields
        .iter()
        .filter(|(path, _)| {
            let name = path.rsplit('.').next().unwrap_or(path).to_ascii_lowercase();
            [
                "total",
                "totalcount",
                "total_count",
                "count",
                "page",
                "pagesize",
                "page_size",
                "next",
                "nextpage",
                "next_page",
                "continuation",
                "continuationtoken",
                "continuation_token",
                "hasnext",
                "has_next",
                "hasmore",
                "has_more",
                "skip",
                "offset",
            ]
            .contains(&name.as_str())
        })
        .map(|(path, types)| (path.clone(), types.clone()))
        .collect::<BTreeMap<_, _>>();
    let array_report = arrays
        .into_iter()
        .map(|(path, (counts, item_types))| {
            (path, json!({"counts":counts,"item_types":item_types}))
        })
        .collect::<BTreeMap<_, _>>();
    let field_report: Map<String, Value> = fields
        .into_iter()
        .map(|(path, types)| (path, json!(types)))
        .collect();
    json!({
        "root_type":root_type,
        "root_count":root_count,
        "root_direct_array_counts":root_array_counts,
        "union_field_types":field_report,
        "named_array_paths_and_counts":array_report,
        "pagination_or_total_field_types":pagination
    })
}

async fn fetch(
    record: &binus_auth_spike::CredentialRecord,
    session_id: &str,
    kind: Kind,
    start: Instant,
) -> Result<(u16, Value), &'static str> {
    let req = request(record, session_id, kind)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "client_error")?;
    let remain = TOTAL
        .saturating_sub(start.elapsed())
        .min(Duration::from_secs(20));
    if remain.is_zero() {
        return Err("total_timeout");
    }
    let mut response = tokio::time::timeout(remain, client.execute(req))
        .await
        .map_err(|_| "timeout")?
        .map_err(|_| "network_error")?;
    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        return Err("auth_error");
    }
    if !(200..300).contains(&status) {
        return Err("upstream_error");
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BODY as u64)
    {
        return Err("response_too_large");
    }
    let mut bytes = Vec::new();
    loop {
        let remain = TOTAL.saturating_sub(start.elapsed());
        if remain.is_zero() {
            return Err("total_timeout");
        }
        let chunk = tokio::time::timeout(remain, response.chunk())
            .await
            .map_err(|_| "timeout")?
            .map_err(|_| "response_error")?;
        let Some(chunk) = chunk else { break };
        if bytes.len().saturating_add(chunk.len()) > MAX_BODY {
            return Err("response_too_large");
        }
        bytes.extend_from_slice(&chunk);
    }
    let value = serde_json::from_slice(&bytes).map_err(|_| "invalid_json")?;
    Ok((status, value))
}

fn parse_args() -> Result<Option<(String, Kind)>, &'static str> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--session") {
        return Err("usage");
    }
    let id = args.next().ok_or("usage")?;
    if !safe_session_id(&id) {
        return Err("invalid_session_id");
    }
    if args.next().as_deref() != Some("--kind") {
        return Err("usage");
    }
    let kind = match args.next().as_deref() {
        Some("primary") => Kind::Primary,
        Some("additional") => Kind::Additional,
        _ => return Err("usage"),
    };
    if args.next().is_some() {
        return Err("usage");
    }
    Ok(Some((id, kind)))
}

#[tokio::main]
async fn main() -> ExitCode {
    if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "{{\"command\":\"session_detail_schema\",\"usage\":\"session_detail_schema --session ID --kind primary|additional\"}}"
        );
        return ExitCode::SUCCESS;
    }
    let (session_id, kind) = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) | Err(_) => {
            println!("{{\"status\":\"error\",\"error\":\"usage_or_invalid_session_id\"}}");
            return ExitCode::from(2);
        }
    };
    let (record, _) = match load_preferred_record(&credentials_path()) {
        Ok(loaded) => loaded,
        Err(error) => {
            println!("{{\"status\":\"error\",\"error\":\"{error}\"}}");
            return ExitCode::from(1);
        }
    };
    match fetch(&record, &session_id, kind, Instant::now()).await {
        Ok((status, value)) => {
            println!(
                "{{\"status\":\"complete\",\"http_status\":{status},\"schema\":{}}}",
                schema(&value)
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("{{\"status\":\"error\",\"error\":\"{error}\"}}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_session_ids() {
        for id in ["", ".", "..", "a/b", "%2f", "a?b", "a\\b"] {
            assert!(!safe_session_id(id));
        }
        assert!(safe_session_id("123-abc_X"));
    }

    #[test]
    fn schema_reports_named_arrays_and_all_union_types_without_values() {
        let input = json!({
            "sessions":[{"id":"PRIVATE_SESSION","topic":"PRIVATE_TOPIC","items":[{"resourceUrl":"https://private.invalid/signed"}, {"resourceId":"PRIVATE_ID"}]}],
            "lecturers":[{"name":"PRIVATE_NAME"}],
            "studentId":"PRIVATE_STUDENT"
        });
        let report = schema(&input).to_string();
        assert!(report.contains("$.sessions"));
        assert!(report.contains("$.lecturers"));
        assert!(report.contains("$.sessions[].items"));
        assert!(report.contains("$.sessions[].topic"));
        assert!(report.contains("$.lecturers[].name"));
        for private in [
            "PRIVATE_SESSION",
            "PRIVATE_TOPIC",
            "https://private.invalid/signed",
            "PRIVATE_ID",
            "PRIVATE_NAME",
            "PRIVATE_STUDENT",
        ] {
            assert!(!report.contains(private));
        }
    }
}
