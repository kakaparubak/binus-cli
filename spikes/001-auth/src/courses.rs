use crate::{CredentialRecord, academic_period_request};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashSet,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[cfg(test)]
use url::Url;

const SERVICE: &str = "func-bm7-course-prod";
const TOTAL: Duration = Duration::from_secs(60);

#[derive(Clone)]
enum RequestTarget {
    Production,
    #[cfg(test)]
    Loopback {
        origin: String,
        total: Duration,
    },
}
impl RequestTarget {
    fn total(&self) -> Duration {
        match self {
            Self::Production => TOTAL,
            #[cfg(test)]
            Self::Loopback { total, .. } => *total,
        }
    }
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
#[derive(Serialize)]
struct Coverage {
    discovery_complete: bool,
    complete: bool,
    components: Vec<ComponentCoverage>,
}
#[derive(Serialize)]
struct ComponentCoverage {
    component: String,
    status: &'static str,
}
#[derive(Serialize, Debug)]
struct ApiError {
    code: &'static str,
    stage: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    http_status: Option<u16>,
}
#[derive(Serialize)]
pub struct Period {
    period: String,
    description: String,
    academic_period_status: bool,
    is_active_running_period: bool,
    is_running_period: bool,
    term_begin_date: String,
    term_end_date: String,
}
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Class {
    class_id: String,
    course_id: String,
    course_code: String,
    course_name: String,
    class_code: String,
    period: String,
    component: String,
}

pub(crate) fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}Z", chrono_like(secs))
}
fn chrono_like(secs: u64) -> String {
    let days = secs / 86400;
    let rem = secs % 86400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += if m <= 2 { 1 } else { 0 };
    (y, m, d)
}
fn envelope<T>(
    status: &'static str,
    data: Vec<T>,
    discovery: bool,
    complete: bool,
    components: Vec<ComponentCoverage>,
    errors: Vec<ApiError>,
) -> Envelope<T> {
    Envelope {
        schema_version: "1",
        status,
        data,
        coverage: Coverage {
            discovery_complete: discovery,
            complete,
            components,
        },
        errors,
        warnings: vec![],
        fetched_at: timestamp(),
    }
}
fn err(
    code: &'static str,
    stage: &'static str,
    component: Option<String>,
    http_status: Option<u16>,
) -> ApiError {
    ApiError {
        code,
        stage,
        component,
        http_status,
    }
}
fn array<'a>(v: &'a Value, stage: &'static str) -> Result<&'a Vec<Value>, ApiError> {
    v.as_array()
        .ok_or_else(|| err("invalid_schema", stage, None, None))
}
fn string<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k)?.as_str().filter(|s| !s.is_empty())
}

fn map_periods(v: &Value) -> Result<Vec<Period>, ApiError> {
    let rows = array(v, "periods")?;
    rows.iter()
        .map(|r| {
            Ok(Period {
                period: string(r, "academicPeriod")
                    .ok_or_else(|| err("invalid_schema", "periods", None, None))?
                    .into(),
                description: string(r, "academicPeriodDescription")
                    .ok_or_else(|| err("invalid_schema", "periods", None, None))?
                    .into(),
                academic_period_status: r
                    .get("academicPeriodStatus")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| err("invalid_schema", "periods", None, None))?,
                is_active_running_period: r
                    .get("isActiveRunningPeriod")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| {
                    err("invalid_schema", "periods", None, None)
                })?,
                is_running_period: r
                    .get("isRunningPeriod")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| err("invalid_schema", "periods", None, None))?,
                term_begin_date: string(r, "termBeginDate")
                    .ok_or_else(|| err("invalid_schema", "periods", None, None))?
                    .into(),
                term_end_date: string(r, "termEndDate")
                    .ok_or_else(|| err("invalid_schema", "periods", None, None))?
                    .into(),
            })
        })
        .collect()
}

fn map_components(v: &Value) -> Result<Vec<String>, ApiError> {
    array(v, "components")?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| safe_segment(s))
                .map(str::to_owned)
                .ok_or_else(|| err("invalid_schema", "components", None, None))
        })
        .collect()
}
pub fn safe_segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && s != "."
        && s != ".."
}
fn map_classes(v: &Value, period: &str, component: &str) -> Result<Vec<Class>, ApiError> {
    let rows = array(v, "classes")?;
    rows.iter()
        .map(|r| {
            let actual = string(r, "ssrComponent")
                .ok_or_else(|| err("invalid_schema", "classes", Some(component.into()), None))?;
            if actual != component {
                return Err(err(
                    "component_conflict",
                    "classes",
                    Some(component.into()),
                    None,
                ));
            }
            Ok(Class {
                class_id: string(r, "classId")
                    .ok_or_else(|| err("invalid_schema", "classes", Some(component.into()), None))?
                    .into(),
                course_id: string(r, "crseId")
                    .ok_or_else(|| err("invalid_schema", "classes", Some(component.into()), None))?
                    .into(),
                course_code: string(r, "courseCode")
                    .ok_or_else(|| err("invalid_schema", "classes", Some(component.into()), None))?
                    .into(),
                course_name: string(r, "courseName")
                    .ok_or_else(|| err("invalid_schema", "classes", Some(component.into()), None))?
                    .into(),
                class_code: string(r, "classCode")
                    .ok_or_else(|| err("invalid_schema", "classes", Some(component.into()), None))?
                    .into(),
                period: period.into(),
                component: component.into(),
            })
        })
        .collect()
}

fn request_at(
    record: &CredentialRecord,
    segments: &[&str],
    _target: &RequestTarget,
) -> Result<reqwest::Request, &'static str> {
    let mut req = academic_period_request(record)?;
    let url = req.url_mut();
    #[cfg(test)]
    if let RequestTarget::Loopback { origin, .. } = _target {
        let parsed = Url::parse(&format!("{origin}/")).map_err(|_| "request_failed")?;
        if !parsed
            .host_str()
            .is_some_and(|host| host == "127.0.0.1" || host == "::1" || host == "localhost")
        {
            return Err("request_failed");
        }
        *url = parsed;
    }
    let mut p = url.path_segments_mut().map_err(|_| "request_failed")?;
    p.clear();
    for s in segments {
        p.push(s);
    }
    drop(p);
    Ok(req)
}
async fn get_at(
    client: &reqwest::Client,
    record: &CredentialRecord,
    segments: &[&str],
    start: Instant,
    target: &RequestTarget,
) -> Result<Value, ApiError> {
    let total = target.total();
    if start.elapsed() >= total {
        return Err(err("timeout", "request", None, None));
    }
    let req = request_at(record, segments, target)
        .map_err(|_| err("request_error", "request", None, None))?;
    let deadline = start + total;
    let request_deadline = (Instant::now() + Duration::from_secs(20)).min(deadline);
    let response = tokio::time::timeout_at(request_deadline.into(), client.execute(req))
        .await
        .map_err(|_| err("timeout", "request", None, None))?
        .map_err(|_| err("network_error", "request", None, None))?;
    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        return Err(err("reauth_required", "request", None, Some(status)));
    }
    if status != 200 {
        return Err(err("upstream_error", "request", None, Some(status)));
    }
    let body_deadline = (Instant::now() + Duration::from_secs(20)).min(deadline);
    let bytes = tokio::time::timeout_at(body_deadline.into(), response.bytes())
        .await
        .map_err(|_| err("timeout", "response", None, None))?
        .map_err(|_| err("response_error", "response", None, None))?;
    serde_json::from_slice(&bytes).map_err(|_| err("invalid_json", "response", None, None))
}

pub async fn periods(record: &CredentialRecord) -> Envelope<Period> {
    periods_with(record, &RequestTarget::Production).await
}
async fn periods_with(record: &CredentialRecord, target: &RequestTarget) -> Envelope<Period> {
    let client = match client() {
        Ok(client) => client,
        Err(e) => return envelope("error", vec![], false, false, vec![], vec![e]),
    };
    let start = Instant::now();
    match get_at(
        &client,
        record,
        &[SERVICE, "AcademicPeriod", "Student"],
        start,
        target,
    )
    .await
    .and_then(|v| map_periods(&v))
    {
        Ok(rows) => envelope("complete", rows, true, true, vec![], vec![]),
        Err(e) => envelope("error", vec![], false, false, vec![], vec![e]),
    }
}
pub async fn list(record: &CredentialRecord, period: &str) -> Envelope<Class> {
    list_with(record, period, &RequestTarget::Production).await
}
async fn list_with(
    record: &CredentialRecord,
    period: &str,
    target: &RequestTarget,
) -> Envelope<Class> {
    if !safe_segment(period) {
        return envelope(
            "error",
            vec![],
            false,
            false,
            vec![],
            vec![err("unsafe_identifier", "components", None, None)],
        );
    }
    let client = match client() {
        Ok(client) => client,
        Err(e) => return envelope("error", vec![], false, false, vec![], vec![e]),
    };
    let start = Instant::now();
    let discovered = match get_at(
        &client,
        record,
        &[
            SERVICE,
            "Course",
            "Period",
            period,
            "ClassComponentList",
            "Student",
        ],
        start,
        target,
    )
    .await
    {
        Ok(v) => match map_components(&v) {
            Ok(v) => v,
            Err(e) => return envelope("error", vec![], false, false, vec![], vec![e]),
        },
        Err(e) => return envelope("error", vec![], false, false, vec![], vec![e]),
    };
    let mut data = Vec::new();
    let mut errors = Vec::new();
    let mut coverage = Vec::new();
    let mut seen = HashSet::new();
    for component in discovered
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let result = get_at(
            &client,
            record,
            &[
                SERVICE,
                "Course",
                "Period",
                period,
                "Component",
                &component,
                "Student",
            ],
            start,
            target,
        )
        .await
        .and_then(|v| map_classes(&v, period, &component));
        match result {
            Ok(rows) => {
                for row in rows {
                    if seen.insert((
                        row.period.clone(),
                        row.component.clone(),
                        row.class_id.clone(),
                    )) {
                        data.push(row);
                    }
                }
                coverage.push(ComponentCoverage {
                    component,
                    status: "complete",
                });
            }
            Err(mut e) => {
                e.component = Some(component.clone());
                coverage.push(ComponentCoverage {
                    component,
                    status: "failed",
                });
                errors.push(e);
            }
        }
    }
    let status = if errors.is_empty() {
        "complete"
    } else if coverage.iter().all(|c| c.status == "failed") {
        "error"
    } else {
        "partial"
    };
    let complete = status == "complete";
    envelope(status, data, true, complete, coverage, errors)
}
fn client() -> Result<reqwest::Client, ApiError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| err("client_setup_failed", "client", None, None))
}

#[cfg(test)]
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
            account: "test".into(),
            access_token: "synthetic-access".into(),
            refresh_token: "synthetic-refresh".into(),
            role: RoleContext {
                academic_career: "UGRD".into(),
                institution: "BINUS".into(),
                name: "Student".into(),
                role_id: "STUDENT".into(),
                organization_id: "ORG1".into(),
            },
        }
    }
    fn fixture(
        routes: Vec<(&'static str, u16, &'static str)>,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let h = thread::spawn(move || {
            let mut seen = Vec::new();
            for (_path, status, body) in routes {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = [0; 8192];
                let n = s.read(&mut buf).unwrap();
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                seen.push(req);
                let reason = if status == 200 { "OK" } else { "Error" };
                write!(s,"HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
            seen
        });
        (format!("http://{addr}"), h)
    }
    fn delayed_fixture(
        routes: Vec<(u16, &'static str, Duration, Duration)>,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let h = thread::spawn(move || {
            for (status, body, headers, body_delay) in routes {
                let mut accepted = None;
                for _ in 0..200 {
                    match listener.accept() {
                        Ok(connection) => {
                            accepted = Some(connection);
                            break;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(_) => break,
                    }
                }
                let Some((mut s, _)) = accepted else {
                    break;
                };
                let mut buf = [0; 8192];
                if s.read(&mut buf).is_err() {
                    break;
                }
                thread::sleep(headers);
                let reason = if status == 200 { "OK" } else { "Error" };
                if write!(s,"HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).is_err() { break; }
                thread::sleep(body_delay);
                let _ = s.write_all(body.as_bytes());
            }
        });
        (format!("http://{addr}"), h)
    }
    #[test]
    fn schema_fixtures() {
        let p = json!([{"academicPeriod":"2610","academicPeriodDescription":"Fall","academicPeriodStatus":true,"isActiveRunningPeriod":false,"isRunningPeriod":false,"termBeginDate":"2026-09-01","termEndDate":"2027-01-01"}]);
        assert_eq!(map_periods(&p).unwrap()[0].period, "2610");
        let c = json!(["LAB", "LEC", "SEM"]);
        assert_eq!(map_components(&c).unwrap(), vec!["LAB", "LEC", "SEM"]);
        let row = json!([{"classId":"x","crseId":"c","courseCode":"A","courseName":"Name","classCode":"01","ssrComponent":"LAB","lecturers":[{"id":"private"}],"progressClass":1}]);
        let mapped = map_classes(&row, "2610", "LAB").unwrap();
        let encoded = serde_json::to_value(mapped[0].clone()).unwrap();
        assert!(encoded.get("lecturers").is_none());
        assert_eq!(mapped[0].course_id, "c");
    }
    #[test]
    fn reject_objects_malformed_and_component_conflicts() {
        assert!(map_components(&json!({"items":["LEC"]})).is_err());
        assert!(map_classes(&json!([{}]), "2610", "LEC").is_err());
        assert!(map_classes(&json!([{"classId":"a","crseId":"c","courseCode":"x","courseName":"n","classCode":"1","ssrComponent":"LAB"}]),"2610","LEC").is_err());
        assert!(!safe_segment("../x"));
        assert!(!safe_segment("x?y"));
    }
    #[test]
    fn generated_timestamp_is_current_utc_and_calendar_conversion_handles_boundaries() {
        let now = timestamp();
        assert!(now.ends_with('Z'));
        assert_eq!(
            &now[0..4],
            &chrono_like(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            )[0..4]
        );
        let current = chrono_like(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
        assert_eq!(&now[0..19], &current[0..19]);
        assert_eq!(chrono_like(0), "1970-01-01T00:00:00");
        assert_eq!(chrono_like(951_782_400), "2000-02-29T00:00:00");
    }

    #[tokio::test]
    async fn list_uses_real_http_orchestration_dedupes_rows_and_records_requests() {
        let (root, h) = fixture(vec![
            ("", 200, r#"["LAB","LEC","SEM","LAB"]"#),
            (
                "",
                200,
                r#"[{"classId":"a","crseId":"same","courseCode":"CS","courseName":"Course","classCode":"01","ssrComponent":"LAB"}]"#,
            ),
            (
                "",
                200,
                r#"[{"classId":"a","crseId":"same","courseCode":"CS","courseName":"Course","classCode":"01","ssrComponent":"LEC"}]"#,
            ),
            ("", 200, r#"[]"#),
        ]);
        let out = list_with(
            &record(),
            "2610",
            &RequestTarget::Loopback {
                origin: root.clone(),
                total: TOTAL,
            },
        )
        .await;
        let v = serde_json::to_value(out).unwrap();
        assert_eq!(v["status"], "complete");
        assert_eq!(v["data"].as_array().unwrap().len(), 2);
        assert_eq!(v["data"][0]["period"], "2610");
        assert_eq!(v["coverage"]["components"].as_array().unwrap().len(), 3);
        let req = h.join().unwrap();
        assert_eq!(req.len(), 4);
        for (r, path) in req.iter().zip([
            "/func-bm7-course-prod/Course/Period/2610/ClassComponentList/Student",
            "/func-bm7-course-prod/Course/Period/2610/Component/LAB/Student",
            "/func-bm7-course-prod/Course/Period/2610/Component/LEC/Student",
            "/func-bm7-course-prod/Course/Period/2610/Component/SEM/Student",
        ]) {
            let lower = r.to_ascii_lowercase();
            assert!(r.starts_with(&format!("GET {path} HTTP/1.1")));
            assert!(lower.contains("authorization: bearer synthetic-access"));
            for (k, val) in [
                ("academiccareer", "ugrd"),
                ("institution", "binus"),
                ("rolename", "student"),
                ("roleid", "student"),
                ("roid", "org1"),
            ] {
                assert!(lower.contains(&format!("{k}: {val}")), "{k}");
            }
            assert!(!lower.contains("cookie:"));
            assert!(!r.contains('?'));
            assert!(r.ends_with("\r\n\r\n"), "GET body must be empty");
        }
    }
    #[tokio::test]
    async fn duplicate_class_rows_are_deduped_but_components_are_retained() {
        let (root, h) = fixture(vec![
            ("", 200, r#"["LAB","LEC"]"#),
            (
                "",
                200,
                r#"[{"classId":"a","crseId":"same","courseCode":"CS","courseName":"Course","classCode":"01","ssrComponent":"LAB"},{"classId":"a","crseId":"same","courseCode":"CS","courseName":"Course","classCode":"01","ssrComponent":"LAB"}]"#,
            ),
            (
                "",
                200,
                r#"[{"classId":"a","crseId":"same","courseCode":"CS","courseName":"Course","classCode":"01","ssrComponent":"LEC"}]"#,
            ),
        ]);
        let v = serde_json::to_value(
            list_with(
                &record(),
                "2610",
                &RequestTarget::Loopback {
                    origin: root.clone(),
                    total: TOTAL,
                },
            )
            .await,
        )
        .unwrap();
        h.join().unwrap();
        assert_eq!(v["status"], "complete");
        assert_eq!(v["data"].as_array().unwrap().len(), 2);
        assert_eq!(v["coverage"]["complete"], true);
        assert_eq!(v["coverage"]["components"].as_array().unwrap().len(), 2);
    }
    #[tokio::test]
    async fn valid_component_data_survives_failed_and_malformed_atomic_sibling() {
        let good = r#"[{"classId":"a","crseId":"c","courseCode":"CS","courseName":"Course","classCode":"01","ssrComponent":"LAB"}]"#;
        let (root, h) = fixture(vec![
            ("", 200, r#"["LAB","LEC"]"#),
            ("", 200, good),
            ("", 500, "{}"),
        ]);
        let v = serde_json::to_value(
            list_with(
                &record(),
                "2610",
                &RequestTarget::Loopback {
                    origin: root.clone(),
                    total: TOTAL,
                },
            )
            .await,
        )
        .unwrap();
        h.join().unwrap();
        assert_eq!(v["status"], "partial");
        assert_eq!(v["data"].as_array().unwrap().len(), 1);
        assert_eq!(v["coverage"]["discovery_complete"], true);
        assert_eq!(v["coverage"]["complete"], false);
        assert_eq!(v["coverage"]["components"][0]["status"], "complete");
        assert_eq!(v["coverage"]["components"][1]["status"], "failed");
        assert_eq!(v["errors"][0]["code"], "upstream_error");
        let malformed = r#"[{"classId":"a","crseId":"c","courseCode":"CS","courseName":"Course","classCode":"01","ssrComponent":"LAB"},{}]"#;
        let (root, h) = fixture(vec![
            ("", 200, r#"["LAB","LEC"]"#),
            ("", 200, malformed),
            (
                "",
                200,
                r#"[{"classId":"b","crseId":"d","courseCode":"MA","courseName":"Math","classCode":"02","ssrComponent":"LEC"}]"#,
            ),
        ]);
        let v = serde_json::to_value(
            list_with(
                &record(),
                "2610",
                &RequestTarget::Loopback {
                    origin: root.clone(),
                    total: TOTAL,
                },
            )
            .await,
        )
        .unwrap();
        h.join().unwrap();
        assert_eq!(v["status"], "partial");
        assert_eq!(v["data"].as_array().unwrap().len(), 1);
        assert_eq!(v["data"][0]["component"], "LEC");
        assert_eq!(v["coverage"]["components"][0]["status"], "failed");
        assert_eq!(v["coverage"]["components"][1]["status"], "complete");
        assert_eq!(v["coverage"]["discovery_complete"], true);
        assert_eq!(v["errors"][0]["code"], "invalid_schema");
    }
    #[tokio::test]
    async fn partial_empty_all_failed_and_discovery_failure_have_explicit_coverage() {
        let (root, h) = fixture(vec![
            ("", 200, r#"["LAB","LEC"]"#),
            ("", 200, "[]"),
            ("", 500, "{}"),
        ]);
        let v = serde_json::to_value(
            list_with(
                &record(),
                "2610",
                &RequestTarget::Loopback {
                    origin: root.clone(),
                    total: TOTAL,
                },
            )
            .await,
        )
        .unwrap();
        h.join().unwrap();
        assert_eq!(v["status"], "partial");
        assert_eq!(v["data"], json!([]));
        assert_eq!(v["coverage"]["complete"], false);
        assert_eq!(v["coverage"]["components"][0]["status"], "complete");
        assert_eq!(v["errors"][0]["code"], "upstream_error");
        let (root, h) = fixture(vec![
            ("", 200, r#"["LAB","LEC"]"#),
            ("", 500, "{}"),
            ("", 503, "{}"),
        ]);
        let v = serde_json::to_value(
            list_with(
                &record(),
                "2610",
                &RequestTarget::Loopback {
                    origin: root.clone(),
                    total: TOTAL,
                },
            )
            .await,
        )
        .unwrap();
        h.join().unwrap();
        assert_eq!(v["status"], "error");
        assert_eq!(v["coverage"]["discovery_complete"], true);
        assert_eq!(v["coverage"]["components"][0]["status"], "failed");
        let (root, h) = fixture(vec![("", 500, "{}")]);
        let v = serde_json::to_value(
            list_with(
                &record(),
                "2610",
                &RequestTarget::Loopback {
                    origin: root.clone(),
                    total: TOTAL,
                },
            )
            .await,
        )
        .unwrap();
        h.join().unwrap();
        assert_eq!(v["coverage"]["discovery_complete"], false);
        assert_eq!(v["coverage"]["components"], json!([]));
    }
    #[tokio::test]
    async fn empty_periods_components_and_class_arrays_are_complete() {
        let (root, h) = fixture(vec![("", 200, "[]")]);
        let v = serde_json::to_value(
            periods_with(
                &record(),
                &RequestTarget::Loopback {
                    origin: root.clone(),
                    total: TOTAL,
                },
            )
            .await,
        )
        .unwrap();
        h.join().unwrap();
        assert_eq!(v["status"], "complete");
        assert_eq!(v["data"], json!([]));
        assert_eq!(v["coverage"]["complete"], true);
        for (discovery, route) in [("[]", None), (r#"["LAB"]"#, Some("[]"))] {
            let mut routes = vec![("", 200, discovery)];
            if let Some(body) = route {
                routes.push(("", 200, body));
            }
            let (root, h) = fixture(routes);
            let v = serde_json::to_value(
                list_with(
                    &record(),
                    "2610",
                    &RequestTarget::Loopback {
                        origin: root.clone(),
                        total: TOTAL,
                    },
                )
                .await,
            )
            .unwrap();
            h.join().unwrap();
            assert_eq!(v["status"], "complete");
            assert_eq!(v["data"], json!([]));
            assert_eq!(v["coverage"]["complete"], true);
        }
    }
    #[tokio::test]
    async fn authorization_server_schema_and_component_row_failures_are_classified() {
        for (status, code) in [
            (401, "reauth_required"),
            (403, "reauth_required"),
            (500, "upstream_error"),
        ] {
            let (root, h) = fixture(vec![("", status, "{}")]);
            let v = serde_json::to_value(
                periods_with(
                    &record(),
                    &RequestTarget::Loopback {
                        origin: root.clone(),
                        total: TOTAL,
                    },
                )
                .await,
            )
            .unwrap();
            h.join().unwrap();
            assert_eq!(v["errors"][0]["code"], code);
            assert_eq!(v["errors"][0]["http_status"], status);
        }
        for (body, code) in [("not json", "invalid_json"), ("{}", "invalid_schema")] {
            let (root, h) = fixture(vec![("", 200, body)]);
            let v = serde_json::to_value(
                periods_with(
                    &record(),
                    &RequestTarget::Loopback {
                        origin: root.clone(),
                        total: TOTAL,
                    },
                )
                .await,
            )
            .unwrap();
            h.join().unwrap();
            assert_eq!(v["status"], "error");
            assert_eq!(v["errors"][0]["code"], code);
        }
        let (root, h) = fixture(vec![
            ("", 200, r#"["LAB","LEC"]"#),
            ("", 200, "[{}]"),
            (
                "",
                200,
                r#"[{"classId":"c","crseId":"x","courseCode":"x","courseName":"n","classCode":"1","ssrComponent":"NO"}]"#,
            ),
        ]);
        let v = serde_json::to_value(
            list_with(
                &record(),
                "2610",
                &RequestTarget::Loopback {
                    origin: root.clone(),
                    total: TOTAL,
                },
            )
            .await,
        )
        .unwrap();
        h.join().unwrap();
        assert_eq!(v["status"], "error");
        assert_eq!(v["data"], json!([]));
        assert_eq!(v["coverage"]["complete"], false);
    }
    #[tokio::test]
    async fn absolute_deadline_covers_body_and_redirect_is_never_followed() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let h = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut b = [0; 2048];
            let _ = s.read(&mut b);
            write!(s, "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n").unwrap();
            thread::sleep(Duration::from_millis(250));
            let _ = s.write_all(b"[]");
        });
        let started = Instant::now();
        let v = serde_json::to_value(
            periods_with(
                &record(),
                &RequestTarget::Loopback {
                    origin: format!("http://{addr}"),
                    total: Duration::from_millis(120),
                },
            )
            .await,
        )
        .unwrap();
        let elapsed = started.elapsed();
        h.join().unwrap();
        assert_eq!(v["errors"][0]["code"], "timeout");
        assert!(elapsed < Duration::from_millis(220));
        let second = TcpListener::bind("127.0.0.1:0").unwrap();
        second.set_nonblocking(true).unwrap();
        let second_addr = second.local_addr().unwrap();
        let first = TcpListener::bind("127.0.0.1:0").unwrap();
        let first_addr = first.local_addr().unwrap();
        let h = thread::spawn(move || {
            let (mut s, _) = first.accept().unwrap();
            let mut b = [0; 2048];
            let n = s.read(&mut b).unwrap();
            let req = String::from_utf8_lossy(&b[..n]).to_string();
            write!(s,"HTTP/1.1 302 Found\r\nLocation: http://{second_addr}/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            req
        });
        let v = serde_json::to_value(
            periods_with(
                &record(),
                &RequestTarget::Loopback {
                    origin: format!("http://{first_addr}"),
                    total: TOTAL,
                },
            )
            .await,
        )
        .unwrap();
        let req = h.join().unwrap();
        assert_eq!(v["errors"][0]["code"], "upstream_error");
        assert!(
            req.to_ascii_lowercase()
                .contains("authorization: bearer synthetic-access")
        );
        assert!(second.accept().is_err());
    }
    #[tokio::test]
    async fn shared_absolute_budget_includes_headers_discovery_and_all_components() {
        let (root, h) = delayed_fixture(vec![(
            200,
            "[]",
            Duration::from_millis(180),
            Duration::ZERO,
        )]);
        let started = Instant::now();
        let v = serde_json::to_value(
            periods_with(
                &record(),
                &RequestTarget::Loopback {
                    origin: root,
                    total: Duration::from_millis(120),
                },
            )
            .await,
        )
        .unwrap();
        assert_eq!(v["errors"][0]["code"], "timeout");
        assert!(started.elapsed() < Duration::from_millis(350));
        h.join().unwrap();

        let (root, h) = delayed_fixture(vec![(
            200,
            "[]",
            Duration::from_millis(70),
            Duration::from_millis(70),
        )]);
        let v = serde_json::to_value(
            periods_with(
                &record(),
                &RequestTarget::Loopback {
                    origin: root,
                    total: Duration::from_millis(115),
                },
            )
            .await,
        )
        .unwrap();
        assert_eq!(v["errors"][0]["code"], "timeout");
        h.join().unwrap();

        let (root, h) = delayed_fixture(vec![
            (
                200,
                r#"["LAB","LEC"]"#,
                Duration::from_millis(350),
                Duration::ZERO,
            ),
            (200, "[]", Duration::from_millis(350), Duration::ZERO),
            (200, "[]", Duration::from_millis(500), Duration::ZERO),
        ]);
        let started = Instant::now();
        let v = serde_json::to_value(
            list_with(
                &record(),
                "2610",
                &RequestTarget::Loopback {
                    origin: root,
                    total: Duration::from_secs(1),
                },
            )
            .await,
        )
        .unwrap();
        assert_eq!(v["status"], "partial");
        assert_eq!(v["coverage"]["discovery_complete"], true);
        assert_eq!(v["coverage"]["complete"], false);
        assert_eq!(v["coverage"]["components"][0]["status"], "complete");
        assert_eq!(v["coverage"]["components"][1]["status"], "failed");
        assert_eq!(v["errors"][0]["code"], "timeout");
        assert!(started.elapsed() < Duration::from_millis(1500));
        drop(h);
    }
    #[tokio::test]
    async fn shared_cli_serializer_preserves_service_status_json_and_exit_codes() {
        for (routes, expected, code) in [
            (
                vec![("", 200, r#"["LAB"]"#), ("", 200, "[]")],
                "complete",
                0,
            ),
            (
                vec![
                    ("", 200, r#"["LAB","LEC"]"#),
                    ("", 200, "[]"),
                    ("", 500, "{}"),
                ],
                "partial",
                3,
            ),
            (vec![("", 500, "{}")], "error", 1),
        ] {
            let (root, h) = fixture(routes);
            let result = list_with(
                &record(),
                "2610",
                &RequestTarget::Loopback {
                    origin: root.clone(),
                    total: TOTAL,
                },
            )
            .await;
            h.join().unwrap();
            let (json, exit) = crate::cli::courses_output(result);
            assert_eq!(json["schema_version"], "1");
            assert_eq!(json["status"], expected);
            assert_eq!(json["coverage"]["complete"], expected == "complete");
            assert_eq!(exit, std::process::ExitCode::from(code));
            let fetched = json["fetched_at"].as_str().unwrap();
            assert!(fetched.ends_with('Z'));
            let now = timestamp();
            assert_eq!(&fetched[0..19], &now[0..19]);
        }
    }
    #[tokio::test]
    async fn closed_local_port_is_network_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let root = format!("http://{addr}");
        let v = serde_json::to_value(
            periods_with(
                &record(),
                &RequestTarget::Loopback {
                    origin: root.clone(),
                    total: TOTAL,
                },
            )
            .await,
        )
        .unwrap();
        assert_eq!(v["errors"][0]["code"], "network_error");
    }
}
