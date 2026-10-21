use binus_auth_spike::{academic_period_request, load_preferred_record};
use reqwest::{Method, Request};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    process::ExitCode,
    time::{Duration, Instant},
};
use url::Url;

const ORIGIN: &str = "https://apim-bm7-prod-web.azure-api.net";
const TOTAL: Duration = Duration::from_secs(60);
const MAX_BODY: usize = 8 * 1024 * 1024;
const PER_PAGE: u64 = 1;

fn credentials_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    base.join("binus-auth-spike").join("credentials.json")
}

fn valid_id(s: &str) -> bool {
    binus_auth_spike::courses::safe_segment(s)
}

#[derive(Clone, Copy)]
enum Endpoint<'a> {
    List {
        class: &'a str,
        session: &'a str,
        page: u8,
    },
    Detail {
        class: &'a str,
        thread: &'a str,
    },
}

fn request(
    record: &binus_auth_spike::CredentialRecord,
    endpoint: Endpoint<'_>,
) -> Result<Request, &'static str> {
    let (segments, method) = match endpoint {
        Endpoint::List {
            class,
            session,
            page,
        } => {
            if !valid_id(class) || !valid_id(session) || !(1..=2).contains(&page) {
                return Err("invalid_id");
            }
            (
                vec![
                    "func-bm7-forum-prod",
                    "Thread",
                    "Class",
                    class,
                    "Session",
                    session,
                    "Paging",
                    if page == 1 { "1" } else { "2" },
                ],
                Method::POST,
            )
        }
        Endpoint::Detail { class, thread } => {
            if !valid_id(class) || !valid_id(thread) {
                return Err("invalid_id");
            }
            (
                vec!["func-bm7-forum-prod", "Forum", class, "Thread", thread],
                Method::GET,
            )
        }
    };
    let mut req = academic_period_request(record).map_err(|_| "request_error")?;
    *req.method_mut() = method.clone();
    *req.url_mut() = Url::parse(&format!("{ORIGIN}/")).map_err(|_| "request_error")?;
    {
        let mut path = req
            .url_mut()
            .path_segments_mut()
            .map_err(|_| "request_error")?;
        path.clear();
        for s in segments {
            path.push(s);
        }
    }
    req.headers_mut().remove(reqwest::header::COOKIE);
    if method == Method::POST {
        *req.body_mut() = Some(json!({"TotalDataPerPage":PER_PAGE}).to_string().into());
        req.headers_mut().insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().map_err(|_| "request_error")?,
        );
    }
    Ok(req)
}

fn key_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
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

fn collect(
    v: &Value,
    depth: usize,
    fields: &mut BTreeMap<String, BTreeSet<String>>,
    arrays: &mut BTreeMap<String, BTreeSet<usize>>,
    bodies: &mut BTreeMap<String, BTreeSet<String>>,
) {
    if depth > 8 {
        return;
    }
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if !key_ok(k) {
                    continue;
                }
                fields.entry(k.clone()).or_default().insert(ty(x).into());
                let low = k.to_ascii_lowercase();
                if ["body", "content", "text", "description", "message", "post"]
                    .iter()
                    .any(|n| low.contains(n))
                {
                    bodies.entry(k.clone()).or_default().insert(ty(x).into());
                }
                if let Value::Array(a) = x {
                    arrays.entry(k.clone()).or_default().insert(a.len());
                }
                collect(x, depth + 1, fields, arrays, bodies);
            }
        }
        Value::Array(a) => {
            for x in a.iter().take(1000) {
                collect(x, depth + 1, fields, arrays, bodies);
            }
        }
        _ => (),
    }
}

fn direct_fields(v: &Value) -> BTreeMap<String, BTreeSet<String>> {
    v.as_object()
        .into_iter()
        .flat_map(|m| m.iter())
        .filter(|(k, _)| key_ok(k))
        .fold(BTreeMap::new(), |mut out, (k, v)| {
            out.entry(k.clone())
                .or_insert_with(BTreeSet::new)
                .insert(ty(v).into());
            out
        })
}

fn collect_paths(
    v: &Value,
    path: &str,
    depth: usize,
    fields: &mut BTreeMap<String, BTreeSet<String>>,
    arrays: &mut BTreeMap<String, BTreeSet<usize>>,
) {
    if depth > 8 {
        return;
    }
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if key_ok(k) {
                    let p = format!("{path}.{k}");
                    fields.entry(p.clone()).or_default().insert(ty(x).into());
                    if let Value::Array(a) = x {
                        arrays.entry(p.clone()).or_default().insert(a.len());
                        for y in a.iter().take(1000) {
                            collect_paths(y, &format!("{p}[]"), depth + 1, fields, arrays);
                        }
                    } else {
                        collect_paths(x, &p, depth + 1, fields, arrays);
                    }
                }
            }
        }
        Value::Array(a) => {
            for x in a.iter().take(1000) {
                collect_paths(x, &format!("{path}[]"), depth + 1, fields, arrays);
            }
        }
        _ => (),
    }
}

fn detail_facts(v: &Value, thread: &str, class: &str) -> Value {
    let content = v.get("threadContentText").and_then(Value::as_str);
    let mut signed = false;
    let mut urls = BTreeMap::new();
    let mut counts = BTreeMap::new();
    collect_paths(v, "$", 0, &mut urls, &mut counts);
    fn scan(v: &Value, depth: usize, signed: &mut bool) {
        if depth > 8 {
            return;
        }
        match v {
            Value::Object(m) => {
                for x in m.values() {
                    scan(x, depth + 1, signed);
                }
            }
            Value::Array(a) => {
                for x in a.iter().take(1000) {
                    scan(x, depth + 1, signed);
                }
            }
            Value::String(s) => {
                if let Ok(u) = Url::parse(s) {
                    for (k, _) in u.query_pairs() {
                        if ["sig", "token", "access_token", "refresh_token", "signature"]
                            .contains(&k.to_ascii_lowercase().as_str())
                        {
                            *signed = true;
                        }
                    }
                }
            }
            _ => (),
        }
    }
    scan(v, 0, &mut signed);
    let html = content.is_some_and(|s| {
        let l = s.to_ascii_lowercase();
        l.contains("<html")
            || l.contains("<p")
            || l.contains("<div")
            || l.contains("<br")
            || l.contains("<span")
    });
    json!({"root_id_equals_selected_threadId":v.get("id").and_then(Value::as_str).map(|s|s==thread),"root_forumId_equals_requested_classId":v.get("forumId").and_then(Value::as_str).map(|s|s==class),"threadContentText_is_string":content.is_some(),"threadContentText_nonempty":content.is_some_and(|s|!s.trim().is_empty()),"threadContentText_is_probably_html":content.map(|_|html),"contains_signed_url_parameters":signed})
}

fn list_shape(v: &Value) -> (Value, usize, Option<u64>, Option<String>, Option<bool>) {
    let mut fields = BTreeMap::new();
    let mut arrays = BTreeMap::new();
    let mut bodies = BTreeMap::new();
    collect(v, 0, &mut fields, &mut arrays, &mut bodies);
    let rows = v
        .get("datas")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let total = v.get("totalData").and_then(Value::as_u64);
    let row_fields = rows.iter().fold(
        BTreeMap::<String, BTreeSet<String>>::new(),
        |mut out, row| {
            let mut f = BTreeMap::new();
            let mut a = BTreeMap::new();
            let mut b = BTreeMap::new();
            collect(row, 0, &mut f, &mut a, &mut b);
            for (k, ts) in f {
                out.entry(k).or_default().extend(ts);
            }
            out
        },
    );
    let first = rows.first();
    let thread = first
        .and_then(|r| r.get("threadId"))
        .and_then(Value::as_str)
        .filter(|s| valid_id(s))
        .map(str::to_owned);
    let equal = first.and_then(|r| Some(r.get("id")? == r.get("threadId")?));
    let direct_rows = rows.iter().fold(
        BTreeMap::<String, BTreeSet<String>>::new(),
        |mut out, row| {
            for (k, ts) in direct_fields(row) {
                out.entry(k).or_default().extend(ts);
            }
            out
        },
    );
    (
        json!({"root_field_types":fields,"datas_row_field_types":row_fields,"root_direct_field_types":direct_fields(v),"datas_row_direct_field_types":direct_rows,"datas_count":rows.len(),"totalData":total,"continuation_field_types":fields.iter().filter(|(k,_)|{let n=k.to_ascii_lowercase();["page","next","continuation","hasnext","hasmore"].iter().any(|x|n.contains(x))}).map(|(k,v)|(k.clone(),v.clone())).collect::<BTreeMap<_,_>>() }),
        rows.len(),
        total,
        thread,
        equal,
    )
}

fn safe_key_union(v: &Value) -> Value {
    let mut fields = BTreeMap::new();
    let mut arrays = BTreeMap::new();
    let mut bodies = BTreeMap::new();
    collect(v, 0, &mut fields, &mut arrays, &mut bodies);
    let mut paths = BTreeMap::new();
    let mut path_arrays = BTreeMap::new();
    collect_paths(v, "$", 0, &mut paths, &mut path_arrays);
    json!({"root_field_types":fields,"root_direct_field_types":direct_fields(v),"path_field_types":paths,"attachment_or_nested_array_counts":arrays,"path_array_counts":path_arrays,"body_bearing_field_types":bodies})
}

async fn fetch(req: Request, start: Instant) -> Result<(u16, Value), &'static str> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "network_error")?;
    let remain = TOTAL
        .saturating_sub(start.elapsed())
        .min(Duration::from_secs(20));
    if remain.is_zero() {
        return Err("timeout");
    }
    let mut res = tokio::time::timeout(remain, client.execute(req))
        .await
        .map_err(|_| "timeout")?
        .map_err(|_| "network_error")?;
    let status = res.status().as_u16();
    if status == 401 || status == 403 {
        return Err("auth_error");
    }
    if !(200..300).contains(&status) {
        return Err("http_error");
    }
    if res.content_length().is_some_and(|n| n > MAX_BODY as u64) {
        return Err("schema_error");
    }
    let mut bytes = Vec::new();
    loop {
        let remain = TOTAL.saturating_sub(start.elapsed());
        if remain.is_zero() {
            return Err("timeout");
        }
        let chunk = tokio::time::timeout(remain, res.chunk())
            .await
            .map_err(|_| "timeout")?
            .map_err(|_| "network_error")?;
        let Some(chunk) = chunk else { break };
        if bytes.len().saturating_add(chunk.len()) > MAX_BODY {
            return Err("schema_error");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((
        status,
        serde_json::from_slice(&bytes).map_err(|_| "schema_error")?,
    ))
}

fn args() -> Result<(String, String), &'static str> {
    let mut a = std::env::args().skip(1);
    if a.next().as_deref() != Some("--class") {
        return Err("usage");
    }
    let c = a.next().ok_or("usage")?;
    if !valid_id(&c) {
        return Err("invalid_id");
    }
    if a.next().as_deref() != Some("--session") {
        return Err("usage");
    }
    let s = a.next().ok_or("usage")?;
    if !valid_id(&s) || a.next().is_some() {
        return Err("invalid_id");
    }
    Ok((c, s))
}

#[tokio::main]
async fn main() -> ExitCode {
    let (class, session) = match args() {
        Ok(v) => v,
        Err(e) => {
            println!(
                "{{\"status\":\"error\",\"error\":\"{e}\",\"usage\":\"forum_schema --class ID --session ID\"}}"
            );
            return ExitCode::from(2);
        }
    };
    let (record, _) = match load_preferred_record(&credentials_path()) {
        Ok(v) => v,
        Err(_) => {
            println!("{{\"status\":\"error\",\"error\":\"credentials\"}}");
            return ExitCode::from(1);
        }
    };
    let start = Instant::now();
    let mut pages = Vec::new();
    let mut status_codes = Vec::new();
    let mut total = None;
    let mut first_thread = None;
    let mut id_equal = None;
    let mut seen_threads = BTreeMap::<String, ()>::new();
    let mut valid_rows = 0usize;
    let mut duplicate_threads = 0usize;
    let mut conflicting_thread = false;
    for page in 1..=2 {
        let req = match request(
            &record,
            Endpoint::List {
                class: &class,
                session: &session,
                page,
            },
        ) {
            Ok(r) => r,
            Err(e) => {
                println!("{{\"status\":\"error\",\"error\":\"{e}\"}}");
                return ExitCode::from(1);
            }
        };
        match fetch(req, start).await {
            Ok((status, v)) => {
                let (shape, count, t, thread, equal) = list_shape(&v);
                if let Some(rows) = v.get("datas").and_then(Value::as_array) {
                    for row in rows {
                        if let Some(id) = row
                            .get("threadId")
                            .and_then(Value::as_str)
                            .filter(|s| valid_id(s))
                        {
                            valid_rows += 1;
                            if seen_threads.insert(id.to_owned(), ()).is_some() {
                                duplicate_threads += 1;
                                conflicting_thread = true;
                            }
                        }
                    }
                }
                status_codes.push(status);
                pages.push((shape, count));
                total = t;
                if page == 1 {
                    first_thread = thread;
                    id_equal = equal;
                }
                if page == 1 && total.is_none_or(|n| n <= pages[0].1 as u64) {
                    break;
                }
            }
            Err(e) => {
                println!("{{\"status\":\"error\",\"error\":\"{e}\",\"page\":{page}}}");
                return ExitCode::from(1);
            }
        }
    }
    let agreement = pages.len() < 2
        || (pages[0].0.get("root_field_types") == pages[1].0.get("root_field_types")
            && pages[0].0.get("datas_row_field_types") == pages[1].0.get("datas_row_field_types"));
    let detail = if let Some(thread) = first_thread.as_deref() {
        match request(
            &record,
            Endpoint::Detail {
                class: &class,
                thread,
            },
        )
        .and_then(|r| {
            if start.elapsed() >= TOTAL {
                Err("timeout")
            } else {
                Ok(r)
            }
        }) {
            Ok(req) => match fetch(req, start).await {
                Ok((s, v)) => {
                    json!({"http_status":s,"schema":safe_key_union(&v),"facts":detail_facts(&v,thread,&class)})
                }
                Err(e) => json!({"error":e}),
            },
            Err(e) => json!({"error":e}),
        }
    } else {
        json!({"skipped":"no_valid_threadId_on_first_page"})
    };
    let page_report: Vec<Value> = pages
        .iter()
        .enumerate()
        .map(|(i, (s, count))| {
            json!({"page":i+1,"http_status":status_codes[i],"datas_count":count,"schema":s})
        })
        .collect();
    println!(
        "{}",
        json!({"status":"complete","pages":page_report,"declared_totalData":total,"continuation_ambiguity":total.is_none(),"page_schemas_agree":agreement,"id_equals_threadId":id_equal,"collected_rows":valid_rows,"unique_valid_threadId_count":seen_threads.len(),"exact_duplicate_threadId_count":duplicate_threads,"threadId_conflict":conflicting_thread,"total_matches_unique_valid_threadIds":total.map(|n|n as usize==seen_threads.len()),"detail":detail})
    );
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record() -> binus_auth_spike::CredentialRecord {
        serde_json::from_value(json!({"account":"a","access_token":"secret","refresh_token":"r","role":{"organization_id":"org","academic_career":"career","institution":"inst","name":"role","role_id":"rid"}})).unwrap()
    }
    #[test]
    fn invalid_ids_prevent_request_construction() {
        assert!(
            request(
                &record(),
                Endpoint::List {
                    class: "bad/path",
                    session: "s",
                    page: 1
                }
            )
            .is_err()
        );
        assert!(
            request(
                &record(),
                Endpoint::Detail {
                    class: "c",
                    thread: "."
                }
            )
            .is_err()
        );
    }
    #[test]
    fn requests_match_verified_endpoints_and_inherit_headers() {
        let r = record();
        let list = request(
            &r,
            Endpoint::List {
                class: "c",
                session: "s",
                page: 1,
            },
        )
        .unwrap();
        assert_eq!(*list.method(), Method::POST);
        assert_eq!(
            list.url().path(),
            "/func-bm7-forum-prod/Thread/Class/c/Session/s/Paging/1"
        );
        assert_eq!(
            list.body().unwrap().as_bytes().unwrap(),
            br#"{"TotalDataPerPage":1}"#
        );
        assert_eq!(list.headers()["content-type"], "application/json");
        let detail = request(
            &r,
            Endpoint::Detail {
                class: "c",
                thread: "t",
            },
        )
        .unwrap();
        assert_eq!(*detail.method(), Method::GET);
        assert_eq!(detail.url().path(), "/func-bm7-forum-prod/Forum/c/Thread/t");
        for h in [
            "authorization",
            "roid",
            "academiccareer",
            "institution",
            "rolename",
            "roleid",
        ] {
            assert!(list.headers().contains_key(h), "missing {h}");
            assert!(detail.headers().contains_key(h), "missing {h}");
        }
        assert!(!list.headers().contains_key("cookie"));
        assert!(!detail.headers().contains_key("cookie"));
    }
    #[test]
    fn selects_thread_id_and_emits_only_schema() {
        let v = json!({"datas":[{"id":"private-id","threadId":"safe-id","title":"private-title"}],"totalData":2});
        let (report, _, _, thread, equal) = list_shape(&v);
        assert_eq!(thread.as_deref(), Some("safe-id"));
        assert_eq!(equal, Some(false));
        let report = report.to_string();
        assert!(!report.contains("private-id"));
        assert!(!report.contains("private-title"));
    }

    #[test]
    fn direct_fields_and_paths_keep_nested_fields_separate() {
        let v = json!({"id":"root","attachments":[{"id":"nested","name":"secret-name","url":"https://x.test/a?sig=secret"}]});
        let d = direct_fields(&v);
        assert_eq!(d["id"], BTreeSet::from(["string".into()]));
        let mut p = BTreeMap::new();
        let mut a = BTreeMap::new();
        collect_paths(&v, "$", 0, &mut p, &mut a);
        assert!(p.contains_key("$.id"));
        assert!(p.contains_key("$.attachments[].id"));
        assert!(a.contains_key("$.attachments"));
        let out = safe_key_union(&v).to_string();
        assert!(!out.contains("secret-name"));
        assert!(!out.contains("sig=secret"));
    }

    #[test]
    fn detail_facts_compare_context_and_content_without_emitting_values() {
        let v = json!({"id":"th-1","forumId":"c-1","threadContentText":"<p>private body</p>","url":"https://x.test/?access_token=private"});
        let f = detail_facts(&v, "th-1", "c-1");
        assert_eq!(f["root_id_equals_selected_threadId"], true);
        assert_eq!(f["root_forumId_equals_requested_classId"], true);
        assert_eq!(f["threadContentText_is_probably_html"], true);
        assert_eq!(f["contains_signed_url_parameters"], true);
        assert!(!f.to_string().contains("private"));
    }
}
