use binus_auth_spike::{academic_period_request, load_preferred_record};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::ExitCode,
    time::{Duration, Instant},
};
use url::Url;

const ORIGIN: &str = "https://apim-bm7-prod-web.azure-api.net";
const SERVICE: &str = "func-bm7-course-prod";
const MAX_COMPONENTS: usize = 5;
const TOTAL: Duration = Duration::from_secs(60);

fn credentials_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    base.join("binus-auth-spike").join("credentials.json")
}

fn shape(value: &Value) -> Value {
    match value {
        Value::Null => json!("null"),
        Value::Bool(_) => json!("boolean"),
        Value::Number(n) => json!(if n.is_i64() || n.is_u64() {
            "integer"
        } else {
            "number"
        }),
        Value::String(_) => json!("string"),
        Value::Array(a) => {
            json!({"type":"array", "count":a.len(), "items": a.first().map(shape).unwrap_or(json!("unknown"))})
        }
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, v) in m {
                out.insert(k.clone(), shape(v));
            }
            json!({"type":"object","fields":out})
        }
    }
}

fn field_types(value: &Value) -> Value {
    let mut fields: BTreeMap<String, Vec<String>> = BTreeMap::new();
    fn walk(v: &Value, prefix: &str, out: &mut BTreeMap<String, Vec<String>>) {
        match v {
            Value::Array(a) => {
                for x in a {
                    walk(x, prefix, out);
                }
            }
            Value::Object(m) => {
                for (k, x) in m {
                    let path = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    let ty = match x {
                        Value::Null => "null",
                        Value::Bool(_) => "boolean",
                        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
                        Value::Number(_) => "number",
                        Value::String(_) => "string",
                        Value::Array(_) => "array",
                        Value::Object(_) => "object",
                    };
                    let vals = out.entry(path.clone()).or_default();
                    if !vals.iter().any(|s| s == ty) {
                        vals.push(ty.into());
                    }
                    walk(x, &path, out);
                }
            }
            _ => (),
        }
    }
    walk(value, "", &mut fields);
    json!(fields)
}

fn catalog_value(v: &Value) -> bool {
    v.is_string() || v.as_i64().is_some() || v.as_u64().is_some()
}
fn safe_catalog(records: &[Value], keys: &[&str]) -> Vec<Value> {
    records
        .iter()
        .map(|r| {
            let mut m = Map::new();
            if let Some(o) = r.as_object() {
                for &k in keys {
                    if let Some(v) = o.get(k).filter(|x| catalog_value(x)) {
                        m.insert(k.into(), v.clone());
                    }
                }
            }
            Value::Object(m)
        })
        .collect()
}
fn safe_component_catalog(records: &[Value]) -> Vec<Value> {
    records
        .iter()
        .map(|r| {
            if r.is_string() {
                r.clone()
            } else {
                safe_catalog(
                    std::slice::from_ref(r),
                    &[
                        "id",
                        "value",
                        "component",
                        "ssrComponent",
                        "label",
                        "name",
                        "description",
                    ],
                )
                .into_iter()
                .next()
                .unwrap_or(Value::Null)
            }
        })
        .collect()
}
fn component_value(record: &Value) -> Option<&str> {
    if let Some(value) = record.as_str() {
        return Some(value);
    }
    let object = record.as_object()?;
    let candidates = ["ssrComponent", "component", "value", "id"]
        .iter()
        .filter_map(|key| object.get(*key).and_then(Value::as_str));
    let mut candidates = candidates;
    let value = candidates.next()?;
    if candidates.next().is_none() {
        Some(value)
    } else {
        None
    }
}
fn records(v: &Value) -> Vec<Value> {
    if let Some(a) = v.as_array() {
        a.clone()
    } else if let Some(a) = v
        .as_object()
        .and_then(|o| o.values().find(|x| x.is_array()))
        .and_then(Value::as_array)
    {
        a.clone()
    } else {
        vec![]
    }
}

fn make_request(
    record: &binus_auth_spike::CredentialRecord,
    segments: &[&str],
) -> Result<reqwest::Request, &'static str> {
    let mut req = academic_period_request(record).map_err(|_| "request_failed")?;
    let url = req.url_mut();
    *url = Url::parse(&format!("{ORIGIN}/")).map_err(|_| "request_failed")?;
    {
        let mut p = url.path_segments_mut().map_err(|_| "request_failed")?;
        p.clear();
        for s in segments {
            p.push(s);
        }
    }
    Ok(req)
}

async fn get(
    client: &reqwest::Client,
    record: &binus_auth_spike::CredentialRecord,
    segments: &[&str],
    start: Instant,
) -> Result<(u16, Value), String> {
    if start.elapsed() >= TOTAL {
        return Err("total_timeout".into());
    }
    let req = make_request(record, segments).map_err(str::to_owned)?;
    let remain = TOTAL
        .saturating_sub(start.elapsed())
        .min(Duration::from_secs(20));
    let response = tokio::time::timeout(remain, client.execute(req))
        .await
        .map_err(|_| "timeout".to_string())?
        .map_err(|_| "network_error".to_string())?;
    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        return Err(format!("auth_http_{status}"));
    }
    if !(200..300).contains(&status) {
        return Err(format!("http_{status}"));
    }
    let bytes = tokio::time::timeout(remain, response.bytes())
        .await
        .map_err(|_| "timeout".to_string())?
        .map_err(|_| "response_error".to_string())?;
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid_json".to_string())?;
    Ok((status, v))
}

fn parse_args() -> Result<(String, Option<String>), String> {
    let mut a = std::env::args().skip(1);
    let command = a.next().ok_or("usage")?;
    if command == "periods" && a.next().is_none() {
        return Ok((command, None));
    }
    if command == "list" && a.next().as_deref() == Some("--period") {
        let p = a.next().ok_or("usage")?;
        if a.next().is_some() || p.is_empty() {
            return Err("usage".into());
        }
        return Ok((command, Some(p)));
    }
    Err("usage".into())
}

#[tokio::main]
async fn main() -> ExitCode {
    if std::env::args().any(|a| a == "--help" || a == "-h") {
        println!(
            "{{\"command\":\"course_schema\",\"usage\":[\"course_schema periods\",\"course_schema list --period ID\"]}}"
        );
        return ExitCode::SUCCESS;
    }
    let (cmd, period) = match parse_args() {
        Ok(x) => x,
        Err(_) => {
            println!(
                "{{\"status\":\"error\",\"error\":\"usage\",\"usage\":\"course_schema periods | course_schema list --period ID\"}}"
            );
            return ExitCode::from(2);
        }
    };
    let (record, _) = match load_preferred_record(&credentials_path()) {
        Ok(x) => x,
        Err(e) => {
            println!("{{\"status\":\"error\",\"error\":\"{}\"}}", e);
            return ExitCode::from(1);
        }
    };
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(x) => x,
        Err(_) => {
            println!("{{\"status\":\"error\",\"error\":\"client_error\"}}");
            return ExitCode::from(1);
        }
    };
    let start = Instant::now();
    let result = if cmd == "periods" {
        match get(
            &client,
            &record,
            &[SERVICE, "AcademicPeriod", "Student"],
            start,
        )
        .await
        {
            Ok((status, v)) => {
                let rs = records(&v);
                json!({"command":"periods","status":"complete","http_status":status,"root_shape":shape(&v),"record_field_types":field_types(&v),"periods":safe_catalog(&rs,&["id","value","academicPeriod","academicPeriodDescription","academicPeriodStatus","isActiveRunningPeriod","isRunningPeriod","termBeginDate","termEndDate"])})
            }
            Err(e) => json!({"command":"periods","status":"error","error":e}),
        }
    } else {
        let pid = period.unwrap();
        match get(
            &client,
            &record,
            &[
                SERVICE,
                "Course",
                "Period",
                &pid,
                "ClassComponentList",
                "Student",
            ],
            start,
        )
        .await
        {
            Err(e) => {
                json!({"command":"list","status":"error","selected_period_id":pid,"stage":"components","error":e})
            }
            Ok((status, v)) => {
                let comps = records(&v);
                let selected = safe_component_catalog(&comps);
                let mut fetched = Vec::new();
                let mut failures = Vec::new();
                for c in comps.iter().take(MAX_COMPONENTS) {
                    let Some(cv) = component_value(c) else {
                        failures.push(json!({"error":"component_selection_unknown_or_ambiguous"}));
                        continue;
                    };
                    match get(&client,&record,&[SERVICE,"Course","Period",&pid,"Component",cv,"Student"],start).await {
                        Ok((st,body))=>{let rows=records(&body);fetched.push(json!({"component":safe_component_catalog(std::slice::from_ref(c)),"http_status":st,"count":rows.len(),"root_shape":shape(&body),"record_field_types":field_types(&body),"class_schema":"Synthetic values intentionally omitted; inspect field names and types above."}));},
                        Err(e)=>failures.push(json!({"component":safe_component_catalog(std::slice::from_ref(c)),"error":e}))
                    }
                }
                json!({"command":"list","status":if failures.is_empty()&&comps.len()<=MAX_COMPONENTS{"complete"}else{"partial"},"selected_period_id":pid,"discovery_http_status":status,"component_root_shape":shape(&v),"component_record_field_types":field_types(&v),"components":selected,"fetched":fetched,"failures":failures,"skipped_components":comps.len().saturating_sub(MAX_COMPONENTS)})
            }
        }
    };
    println!("{}", result);
    if result["status"] == "complete" {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
