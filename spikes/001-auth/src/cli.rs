#[cfg(unix)]
use crate::capture;
use crate::{
    CredentialRecord, academic_period_request, load_preferred_record, promote_verified_candidate,
    remove_spike_artifacts, sanitized_status, store_record,
};
use clap::{Parser, Subcommand};
use reqwest::redirect::Policy;
use serde_json::json;
use std::{path::PathBuf, process::ExitCode, time::Duration};

#[derive(Parser)]
#[command(
    name = "binus-auth-spike",
    about = "Experimental BINUS authentication feasibility spike"
)]
struct Cli {
    #[cfg(not(windows))]
    #[arg(long, global = true)]
    allow_plaintext_file: bool,
    #[arg(long, global = true, default_value_t = 300)]
    timeout_seconds: u64,
    #[command(subcommand)]
    command: Command,
    #[arg(skip)]
    program: Option<String>,
}
#[derive(Subcommand)]
enum Command {
    Auth {
        #[command(subcommand)]
        action: Action,
    },
    Courses {
        #[command(subcommand)]
        action: CoursesAction,
    },
    Forums {
        #[command(subcommand)]
        action: ForumsAction,
    },
}
#[derive(Subcommand)]
enum ForumsAction {
    Threads {
        #[command(subcommand)]
        action: ForumThreadsAction,
    },
}
#[derive(Subcommand)]
enum ForumThreadsAction {
    List {
        #[arg(long, required=true, value_parser=parse_class)]
        class: String,
        #[arg(long,required=true,value_parser=parse_session_id)]
        session: String,
    },
    Show {
        #[arg(value_parser=parse_session_id)]
        thread_id: String,
        #[arg(long,required=true,value_parser=parse_class)]
        class: String,
    },
}
#[derive(Subcommand)]
enum CoursesAction {
    Periods,
    List {
        #[arg(long, required = true, value_parser = parse_period)]
        period: String,
    },
    Sessions {
        #[command(subcommand)]
        action: SessionsAction,
    },
}
#[derive(Subcommand)]
enum SessionsAction {
    List {
        #[arg(long, required=true, value_parser=parse_class)]
        class: String,
    },
    Show {
        #[arg(value_parser = parse_session_id)]
        session_id: String,
    },
}
#[derive(Subcommand)]
enum Action {
    Login,
    Status,
    Check,
    Logout,
}

fn credentials_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    base.join("binus-auth-spike").join("credentials.json")
}
fn parse_period(value: &str) -> Result<String, String> {
    if crate::courses::safe_segment(value) {
        Ok(value.to_owned())
    } else {
        Err("period must be a safe catalog identifier".into())
    }
}
fn parse_class(value: &str) -> Result<String, String> {
    if crate::courses::safe_segment(value) {
        Ok(value.into())
    } else {
        Err("class must be a safe catalog identifier".into())
    }
}
fn parse_session_id(value: &str) -> Result<String, String> {
    if crate::courses::safe_segment(value) {
        Ok(value.to_owned())
    } else {
        Err("session ID must be a safe catalog identifier".into())
    }
}

pub async fn run() -> ExitCode {
    let mut cli = Cli::parse();
    let program = std::env::args()
        .next()
        .and_then(|s| {
            std::path::PathBuf::from(s)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "binus".into());
    cli.program = Some(program.clone());
    let path = credentials_path();
    match cli.command {
        Command::Forums {
            action: ForumsAction::Threads { action },
        } => {
            let is_show = matches!(action, ForumThreadsAction::Show { .. });
            let (class, scope) = match &action {
                ForumThreadsAction::List { class, session } => {
                    (class.clone(), Some(session.clone()))
                }
                ForumThreadsAction::Show { class, .. } => (class.clone(), None),
            };
            if !crate::courses::safe_segment(&class)
                || scope
                    .as_deref()
                    .is_some_and(|v| !crate::courses::safe_segment(v))
            {
                return emit(
                    json!({"schema_version":"1","status":"error","data":[],"errors":[{"code":"unsafe_identifier","stage":"request"}],"warnings":[]}),
                    false,
                );
            }
            let (record, _) = match load_preferred_record(&path) {
                Ok(v) => v,
                Err("not_logged_in") => return emit_forum_missing("not_logged_in", is_show),
                Err(_) => return emit_forum_missing("storage_error", is_show),
            };
            match action {
                ForumThreadsAction::List { class, session } => {
                    emit_forum(crate::forums::list(&record, &class, &session).await)
                }
                ForumThreadsAction::Show { class, thread_id } => {
                    emit_forum(crate::forums::show(&record, &class, &thread_id).await)
                }
            }
        }
        Command::Courses { action } => {
            if let CoursesAction::List { ref period } = action
                && !crate::courses::safe_segment(period)
            {
                return emit_courses_error("unsafe_identifier");
            }
            if let CoursesAction::Sessions {
                action: SessionsAction::List { ref class },
            } = action
                && !crate::courses::safe_segment(class)
            {
                return emit_session_error(class, "unsafe_identifier", "request");
            }
            if let CoursesAction::Sessions {
                action: SessionsAction::Show { ref session_id },
            } = action
                && !crate::courses::safe_segment(session_id)
            {
                return emit_detail_error(session_id, "unsafe_identifier", "request");
            }
            let (record, _) = match load_preferred_record(&path) {
                Ok(v) => v,
                Err("not_logged_in") => {
                    return match action {
                        CoursesAction::Sessions {
                            action: SessionsAction::List { class },
                        } => emit_session_error(&class, "not_logged_in", "credentials"),
                        CoursesAction::Sessions {
                            action: SessionsAction::Show { session_id },
                        } => emit_detail_error(&session_id, "not_logged_in", "credentials"),
                        _ => emit_courses_error("not_logged_in"),
                    };
                }
                Err(_) => {
                    return match action {
                        CoursesAction::Sessions {
                            action: SessionsAction::List { class },
                        } => emit_session_error(&class, "storage_error", "credentials"),
                        CoursesAction::Sessions {
                            action: SessionsAction::Show { session_id },
                        } => emit_detail_error(&session_id, "storage_error", "credentials"),
                        _ => emit_courses_error("storage_error"),
                    };
                }
            };
            match action {
                CoursesAction::Periods => emit_courses(crate::courses::periods(&record).await),
                CoursesAction::List { period } => {
                    emit_courses(crate::courses::list(&record, &period).await)
                }
                CoursesAction::Sessions {
                    action: SessionsAction::List { class },
                } => emit_sessions(crate::sessions::list(&record, &class).await),
                CoursesAction::Sessions {
                    action: SessionsAction::Show { session_id },
                } => emit_sessions(crate::session_details::show(&record, &session_id).await),
            }
        }
        Command::Auth {
            action: Action::Login,
        } => {
            #[cfg(not(windows))]
            if !cli.allow_plaintext_file {
                return emit(
                    json!({"status":"blocked","blocker":"explicit_plaintext_consent_required"}),
                    false,
                );
            }
            let record = match capture_with_signals(Duration::from_secs(cli.timeout_seconds)).await
            {
                Ok(r) => r,
                Err(e) => return emit(json!({"status":e}), false),
            };
            let candidate = path.with_file_name("credentials.captured.json");
            if store_record(&candidate, &record).is_err() {
                return emit(json!({"status":"storage_error"}), false);
            }
            if check_record(&record).await {
                if promote_verified_candidate(&path, &candidate, &record).is_err() {
                    return emit(
                        json!({"status":"stored_unverified","stored":true,"live_verified":true,"blocker":"storage_error"}),
                        false,
                    );
                }
                emit(
                    json!({"status":"live_verified","stored":true,"live_verified":true}),
                    true,
                )
            } else {
                emit(
                    json!({"status":"stored_unverified","stored":true,"live_verified":false,"blocker":"safe_read_failed","record":"credentials.captured.json"}),
                    false,
                )
            }
        }
        Command::Auth {
            action: Action::Status,
        } => match load_preferred_record(&path) {
            Ok((_, true)) => emit(
                json!({"status":"stored_unverified","stored":true,"live_verified":false,"pending_candidate":true}),
                true,
            ),
            Ok((_, false)) => emit(
                serde_json::to_value(sanitized_status(true, false, None)).unwrap(),
                true,
            ),
            Err("not_logged_in") => emit(
                serde_json::to_value(sanitized_status(false, false, None)).unwrap(),
                true,
            ),
            Err(e) => emit(json!({"status":"storage_error","error":e}), false),
        },
        Command::Auth {
            action: Action::Check,
        } => check(&path).await,
        Command::Auth {
            action: Action::Logout,
        } => match remove_spike_artifacts(&path) {
            Ok(true) => emit(
                json!({"status":"not_logged_in","removed":"spike_credentials_only"}),
                true,
            ),
            Ok(false) => emit(json!({"status":"not_logged_in","removed":"none"}), true),
            Err(_) => emit(
                json!({"status":"storage_error","error":"storage_error"}),
                false,
            ),
        },
    }
}

fn emit_detail_error(session_id: &str, code: &str, stage: &str) -> ExitCode {
    let v = json!({"schema_version":"1","status":"error","data":null,"errors":[{"code":code,"stage":stage,"session_id":session_id}],"warnings":[],"coverage":{"scope":"selected_session","selected_sessions":1,"discovery_complete":false,"complete":false,"primary":"failed","additional":"failed"},"fetched_at":crate::courses::timestamp()});
    println!("{v}");
    ExitCode::from(1)
}

fn emit_forum<T: serde::Serialize>(value: T) -> ExitCode {
    let (v, c) = courses_output(value);
    println!("{v}");
    c
}
fn emit_forum_missing(code: &str, show: bool) -> ExitCode {
    let data = if show {
        serde_json::Value::Null
    } else {
        json!([])
    };
    let scope = if show {
        "selected_thread"
    } else {
        "selected_session"
    };
    let mut output = json!({"schema_version":"1","status":"error","data":data,"coverage":{"scope":scope,"complete":false},"errors":[{"code":code,"stage":"credentials"}],"warnings":[],"fetched_at":crate::courses::timestamp()});
    if !show {
        output["coverage"]["pages_fetched"] = json!(0);
        output["coverage"]["declared_total"] = serde_json::Value::Null;
    }
    println!("{output}");
    ExitCode::from(1)
}

fn emit_courses<T: serde::Serialize>(value: T) -> ExitCode {
    let (v, code) = courses_output(value);
    println!("{v}");
    code
}
pub(crate) fn courses_output<T: serde::Serialize>(value: T) -> (serde_json::Value, ExitCode) {
    let v=serde_json::to_value(value).unwrap_or_else(|_|json!({"schema_version":"1","status":"error","data":[],"coverage":{"discovery_complete":false,"complete":false,"components":[]},"errors":[{"code":"serialization_error","stage":"output"}],"warnings":[],"fetched_at":crate::courses::timestamp()}));
    let code = match v["status"].as_str() {
        Some("complete") => ExitCode::SUCCESS,
        Some("partial") => ExitCode::from(3),
        _ => ExitCode::from(1),
    };
    (v, code)
}
fn emit_courses_error(code: &str) -> ExitCode {
    println!(
        "{}",
        json!({"schema_version":"1","status":"error","data":[],"coverage":{"discovery_complete":false,"complete":false,"components":[]},"errors":[{"code":code,"stage":"credentials"}],"warnings":[],"fetched_at":crate::courses::timestamp()})
    );
    ExitCode::from(1)
}
fn emit_sessions<T: serde::Serialize>(value: T) -> ExitCode {
    let (v, c) = courses_output(value);
    println!("{v}");
    c
}
fn emit_session_error(class: &str, code: &str, stage: &str) -> ExitCode {
    println!(
        "{}",
        json!({"schema_version":"1","status":"error","data":[],"coverage":{"discovery_complete":false,"complete":false,"classes":[{"class_id":class,"status":"failed"}]},"errors":[{"code":code,"stage":stage,"class_id":class}],"warnings":[],"fetched_at":crate::courses::timestamp()})
    );
    ExitCode::from(1)
}

async fn check(path: &std::path::Path) -> ExitCode {
    let candidate = path.with_file_name("credentials.captured.json");
    let (record, from_candidate) = match load_preferred_record(path) {
        Ok(v) => v,
        Err("not_logged_in") => return emit(json!({"status":"not_logged_in"}), false),
        Err(_) => {
            return emit(
                json!({"status":"storage_error","error":"storage_error"}),
                false,
            );
        }
    };
    let client = match reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return emit(
                json!({"status":"upstream_error","error":"client_setup_failed"}),
                false,
            );
        }
    };
    let result = match academic_period_request(&record) {
        Ok(request) => client.execute(request).await,
        Err(_) => {
            return emit(
                json!({"status":"upstream_error","error":"request_failed"}),
                false,
            );
        }
    };
    let response = match result {
        Ok(r) => r,
        Err(_) => {
            return emit(
                json!({"status":"upstream_error","error":"request_failed"}),
                false,
            );
        }
    };
    match response.status().as_u16() {
        200 => match response.json::<serde_json::Value>().await {
            Ok(v) if v.is_array() => {
                if from_candidate && promote_verified_candidate(path, &candidate, &record).is_err()
                {
                    return emit(
                        json!({"status":"stored_unverified","blocker":"storage_error"}),
                        false,
                    );
                }
                emit(
                    json!({"status":"live_verified","stored":true,"live_verified":true,"academic_period_count":v.as_array().map_or(0, Vec::len)}),
                    true,
                )
            }
            _ => emit(
                json!({"status":"upstream_error","error":"unexpected_response"}),
                false,
            ),
        },
        401 | 403 => emit(
            json!({"status":"reauth_required","http_status":response.status().as_u16()}),
            false,
        ),
        status => emit(
            json!({"status":"upstream_error","http_status":status}),
            false,
        ),
    }
}

async fn check_record(record: &CredentialRecord) -> bool {
    let client = match reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let request = match academic_period_request(record) {
        Ok(r) => r,
        Err(_) => return false,
    };
    if let Ok(response) = client.execute(request).await
        && response.status().as_u16() == 200
    {
        return response
            .json::<serde_json::Value>()
            .await
            .is_ok_and(|v| v.is_array());
    }
    false
}

async fn capture_with_signals(timeout: Duration) -> Result<CredentialRecord, &'static str> {
    #[cfg(unix)]
    {
        let future = capture(timeout);
        tokio::pin!(future);
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(|_| "signal_setup_failed")?;
        tokio::select! {
            result = &mut future => result,
            _ = tokio::signal::ctrl_c() => Err("capture_cancelled"),
            _ = terminate.recv() => Err("capture_cancelled"),
        }
    }
    #[cfg(windows)]
    {
        let mut ctrl_break =
            tokio::signal::windows::ctrl_break().map_err(|_| "signal_setup_failed")?;
        let mut ctrl_close =
            tokio::signal::windows::ctrl_close().map_err(|_| "signal_setup_failed")?;
        let mut ctrl_logoff =
            tokio::signal::windows::ctrl_logoff().map_err(|_| "signal_setup_failed")?;
        let mut ctrl_shutdown =
            tokio::signal::windows::ctrl_shutdown().map_err(|_| "signal_setup_failed")?;
        let cancellation = async move {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = ctrl_break.recv() => {},
                _ = ctrl_close.recv() => {},
                _ = ctrl_logoff.recv() => {},
                _ = ctrl_shutdown.recv() => {},
            }
        };
        crate::capture_with_cancellation(timeout, cancellation).await
    }
}

fn emit(value: serde_json::Value, success: bool) -> ExitCode {
    println!(
        "{}",
        serde_json::to_string(&value).unwrap_or_else(|_| "{\"status\":\"internal_error\"}".into())
    );
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

#[allow(dead_code)]
fn _store_is_explicitly_opted_in(
    path: &std::path::Path,
    record: &crate::CredentialRecord,
    allowed: bool,
) -> Result<(), &'static str> {
    if !allowed {
        return Err("explicit_plaintext_consent_required");
    }
    store_record(path, record)
}
