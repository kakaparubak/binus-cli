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

fn safe_class_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn request(
    record: &binus_auth_spike::CredentialRecord,
    class_id: &str,
) -> Result<reqwest::Request, &'static str> {
    if !safe_class_id(class_id) {
        return Err("invalid_class_id");
    }
    let mut req = academic_period_request(record).map_err(|_| "request_failed")?;
    let url = req.url_mut();
    *url = Url::parse(&format!("{ORIGIN}/")).map_err(|_| "request_failed")?;
    let mut path = url.path_segments_mut().map_err(|_| "request_failed")?;
    path.clear();
    for segment in [SERVICE, "ClassSession", "Class", class_id, "Student"] {
        path.push(segment);
    }
    drop(path);
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

fn shape(v: &Value) -> Value {
    match v {
        Value::Array(a) => {
            let mut types = BTreeSet::new();
            for item in a {
                types.insert(ty(item));
            }
            json!({"type":"array", "count":a.len(), "item_types":types})
        }
        Value::Object(m) => {
            let fields: Map<String, Value> = m.iter().map(|(k, v)| (k.clone(), shape(v))).collect();
            json!({"type":"object", "fields":fields})
        }
        _ => json!(ty(v)),
    }
}

fn schema(v: &Value) -> Value {
    fn walk(
        v: &Value,
        path: &str,
        fields: &mut BTreeMap<String, BTreeSet<String>>,
        arrays: &mut BTreeMap<String, BTreeSet<usize>>,
    ) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    let p = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    fields.entry(p.clone()).or_default().insert(ty(x).into());
                    walk(x, &p, fields, arrays);
                }
            }
            Value::Array(a) => {
                arrays.entry(path.to_string()).or_default().insert(a.len());
                for x in a {
                    walk(x, path, fields, arrays);
                }
            }
            _ => (),
        }
    }
    fn records(v: &Value) -> Vec<&Value> {
        if let Some(a) = v.as_array() {
            return a.iter().collect();
        }
        v.as_object()
            .and_then(|o| o.values().find_map(Value::as_array))
            .map(|a| a.iter().collect())
            .unwrap_or_default()
    }
    let mut fields = BTreeMap::new();
    let mut arrays = BTreeMap::new();
    walk(v, "", &mut fields, &mut arrays);
    let root_records = records(v);
    let mut row_fields: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut nested_arrays: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for row in &root_records {
        walk(row, "$[]", &mut row_fields, &mut nested_arrays);
    }
    let pagination: BTreeMap<_, _> = fields
        .iter()
        .filter(|(k, _)| {
            let n = k.rsplit('.').next().unwrap_or(k).to_ascii_lowercase();
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
                "skip",
                "offset",
            ]
            .contains(&n.as_str())
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    json!({"root_shape":shape(v), "root_array_count":v.as_array().map(Vec::len),
        "root_object_array_counts":v.as_object().map(|o| o.iter().filter_map(|(k,x)| x.as_array().map(|a|(k.clone(),a.len()))).collect::<BTreeMap<_,_>>()).unwrap_or_default(),
        "union_field_types":fields, "record_field_types":row_fields,
        "nested_array_paths_and_counts":nested_arrays, "apparent_total_or_continuation_fields":pagination})
}

async fn fetch(
    record: &binus_auth_spike::CredentialRecord,
    class_id: &str,
    start: Instant,
) -> Result<(u16, Value), &'static str> {
    let req = request(record, class_id)?;
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
    let response = tokio::time::timeout(remain, client.execute(req))
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
    let mut response = response;
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
        let Some(chunk) = chunk else {
            break;
        };
        if bytes.len().saturating_add(chunk.len()) > MAX_BODY {
            return Err("response_too_large");
        }
        bytes.extend_from_slice(&chunk);
    }
    let value = serde_json::from_slice(&bytes).map_err(|_| "invalid_json")?;
    Ok((status, value))
}

fn parse_args() -> Result<Option<String>, ()> {
    let mut args = std::env::args().skip(1);
    if args.len() == 0 {
        return Ok(None);
    }
    if args.next().as_deref() != Some("--class") {
        return Err(());
    }
    let id = args.next().ok_or(())?;
    if args.next().is_some() || !safe_class_id(&id) {
        return Err(());
    }
    Ok(Some(id))
}

#[tokio::main]
async fn main() -> ExitCode {
    if std::env::args().any(|a| a == "--help" || a == "-h") {
        println!("{{\"command\":\"session_schema\",\"usage\":\"session_schema --class ID\"}}");
        return ExitCode::SUCCESS;
    }
    let Some(class_id) = (match parse_args() {
        Ok(Some(id)) => Some(id),
        _ => None,
    }) else {
        println!(
            "{{\"status\":\"error\",\"error\":\"usage\",\"usage\":\"session_schema --class ID\"}}"
        );
        return ExitCode::from(2);
    };
    let (record, _) = match load_preferred_record(&credentials_path()) {
        Ok(x) => x,
        Err(e) => {
            println!("{{\"status\":\"error\",\"error\":\"{e}\"}}");
            return ExitCode::from(1);
        }
    };
    match fetch(&record, &class_id, Instant::now()).await {
        Ok((status, value)) => {
            let report = schema(&value);
            println!("{{\"status\":\"complete\",\"http_status\":{status},\"schema\":{report}}}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("{{\"status\":\"error\",\"error\":\"{e}\"}}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_path_injection() {
        for id in ["", ".", "..", "a/b", "%2f", "a?b", "a\\b"] {
            assert!(!safe_class_id(id));
        }
        assert!(safe_class_id("123-abc_X"));
    }
    #[test]
    fn reports_union_shapes_without_values() {
        let v = json!([{ "sessionId":"SECRET-A", "topic":"PRIVATE", "nested":[{"x":1},{"x":"SECRET-B"}] }, {"sessionId":2,"nested":[]}]);
        let out = schema(&v).to_string();
        assert!(out.contains("sessionId"));
        assert!(out.contains("integer"));
        assert!(out.contains("string"));
        assert!(out.contains("nested"));
        assert!(!out.contains("SECRET"));
        assert!(!out.contains("PRIVATE"));
    }
}
