use std::process::Command;
#[cfg(unix)]
use std::process::Stdio;

#[test]
fn plaintext_consent_is_required() {
    let output = Command::new(env!("CARGO_BIN_EXE_binus-auth-spike"))
        .args(["auth", "login"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("explicit_plaintext_consent_required")
    );
}

#[test]
fn check_without_credentials_is_unsuccessful_and_status_is_not_logged_in() {
    let temp = tempfile::tempdir().unwrap();
    let bin = env!("CARGO_BIN_EXE_binus-auth-spike");
    let check = Command::new(bin)
        .args(["auth", "check"])
        .env("XDG_CONFIG_HOME", temp.path())
        .output()
        .unwrap();
    assert!(!check.status.success());
    assert!(String::from_utf8_lossy(&check.stdout).contains("not_logged_in"));
    let status = Command::new(bin)
        .args(["auth", "status"])
        .env("XDG_CONFIG_HOME", temp.path())
        .output()
        .unwrap();
    assert!(status.status.success());
}

#[test]
fn courses_usage_precedes_credentials_and_missing_credentials_are_enveloped() {
    let temp = tempfile::tempdir().unwrap();
    let bin = env!("CARGO_BIN_EXE_binus");
    let missing = Command::new(bin)
        .args(["courses", "list"])
        .env("XDG_CONFIG_HOME", temp.path())
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2));
    let no_creds = Command::new(bin)
        .args(["courses", "periods"])
        .env("XDG_CONFIG_HOME", temp.path())
        .output()
        .unwrap();
    assert_eq!(no_creds.status.code(), Some(1));
    let body: serde_json::Value = serde_json::from_slice(&no_creds.stdout).unwrap();
    assert_eq!(body["schema_version"], "1");
    assert_eq!(body["status"], "error");
    assert_eq!(body["errors"][0]["code"], "not_logged_in");
    let fetched = body["fetched_at"].as_str().unwrap();
    assert!(fetched.ends_with('Z'));
    assert!(fetched.starts_with(&format!("{}-", chrono_year())));
    let unsafe_period = Command::new(bin)
        .args(["courses", "list", "--period", "../x"])
        .env("XDG_CONFIG_HOME", temp.path())
        .output()
        .unwrap();
    assert_eq!(unsafe_period.status.code(), Some(2));
    assert!(unsafe_period.stdout.is_empty());
}

fn chrono_year() -> String {
    // Derive the UTC year through the standard library without pinning this test to a calendar year.
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let days = (seconds / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    year.to_string()
}

fn assert_current_timestamp(value: &serde_json::Value) {
    let fetched = value.as_str().unwrap();
    assert!(fetched.ends_with('Z'));
    assert!(fetched.starts_with(&format!("{}-", chrono_year())));
}

#[test]
fn sessions_usage_precedes_credentials_and_missing_credentials_are_enveloped() {
    let temp = tempfile::tempdir().unwrap();
    let bin = env!("CARGO_BIN_EXE_binus");
    let usage = Command::new(bin)
        .args(["courses", "sessions", "list"])
        .env("XDG_CONFIG_HOME", temp.path())
        .output()
        .unwrap();
    assert_eq!(usage.status.code(), Some(2));
    let unsafe_id = Command::new(bin)
        .args(["courses", "sessions", "list", "--class", "../x"])
        .env("XDG_CONFIG_HOME", temp.path())
        .output()
        .unwrap();
    assert_eq!(unsafe_id.status.code(), Some(2));
    let missing = Command::new(bin)
        .args(["courses", "sessions", "list", "--class", "abc-GUID"])
        .env("XDG_CONFIG_HOME", temp.path())
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
    let body: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(body["schema_version"], "1");
    assert_eq!(body["errors"][0]["code"], "not_logged_in");
    assert_eq!(body["coverage"]["classes"][0]["class_id"], "abc-GUID");
    assert_current_timestamp(&body["fetched_at"]);
}

#[test]
fn show_help_and_unsafe_id_precede_credentials_for_both_binary_names() {
    let temp = tempfile::tempdir().unwrap();
    for bin in [
        env!("CARGO_BIN_EXE_binus"),
        env!("CARGO_BIN_EXE_binus-auth-spike"),
    ] {
        let help = Command::new(bin)
            .args(["courses", "sessions", "show", "--help"])
            .env("XDG_CONFIG_HOME", temp.path())
            .output()
            .unwrap();
        assert_eq!(help.status.code(), Some(0));
        assert!(String::from_utf8_lossy(&help.stdout).contains("Usage:"));
        let usage = Command::new(bin)
            .args(["courses", "sessions", "show"])
            .env("XDG_CONFIG_HOME", temp.path())
            .output()
            .unwrap();
        assert_eq!(usage.status.code(), Some(2));
        let unsafe_id = Command::new(bin)
            .args(["courses", "sessions", "show", "../bad"])
            .env("XDG_CONFIG_HOME", temp.path())
            .output()
            .unwrap();
        assert_eq!(unsafe_id.status.code(), Some(2));
        assert!(unsafe_id.stdout.is_empty());
        let missing = Command::new(bin)
            .args(["courses", "sessions", "show", "safe-id"])
            .env("XDG_CONFIG_HOME", temp.path())
            .output()
            .unwrap();
        assert_eq!(missing.status.code(), Some(1));
        let body: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
        assert_eq!(body["status"], "error");
        assert_eq!(body["errors"][0]["code"], "not_logged_in");
        assert_eq!(body["data"], serde_json::Value::Null);
    }
}

#[test]
fn both_binary_names_keep_auth_missing_credentials_behavior() {
    let temp = tempfile::tempdir().unwrap();
    for bin in [
        env!("CARGO_BIN_EXE_binus"),
        env!("CARGO_BIN_EXE_binus-auth-spike"),
    ] {
        let output = Command::new(bin)
            .args(["auth", "status"])
            .env("XDG_CONFIG_HOME", temp.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(body["status"], "not_logged_in");
        let check = Command::new(bin)
            .args(["auth", "check"])
            .env("XDG_CONFIG_HOME", temp.path())
            .output()
            .unwrap();
        assert_eq!(check.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&check.stdout).contains("not_logged_in"));
    }
}

#[test]
fn missing_browser_is_a_sanitized_capture_failure() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_binus-auth-spike"))
        .args([
            "--timeout-seconds",
            "2",
            "auth",
            "login",
            "--allow-plaintext-file",
        ])
        .env("XDG_CONFIG_HOME", temp.path())
        .env("BINUS_CHROMIUM", temp.path().join("missing-browser"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("browser_launch_failed"));
}

#[cfg(unix)]
#[test]
fn capture_times_out_when_devtools_never_appears() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let browser = temp.path().join("slow-browser");
    std::fs::write(&browser, "#!/bin/sh\nexec sleep 10\n").unwrap();
    std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_binus-auth-spike"))
        .args([
            "--timeout-seconds",
            "1",
            "auth",
            "login",
            "--allow-plaintext-file",
        ])
        .env("XDG_CONFIG_HOME", temp.path())
        .env("BINUS_CHROMIUM", browser)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("capture_timeout"));
}

#[cfg(unix)]
fn interrupted_login_is_cleaned_up(signal: rustix::process::Signal) {
    use std::{
        os::unix::fs::PermissionsExt,
        thread,
        time::{Duration, Instant},
    };
    let temp = tempfile::tempdir().unwrap();
    let browser = temp.path().join("synthetic-browser");
    let profile_marker = temp.path().join("profile-path");
    let late_marker = temp.path().join("descendant-survived");
    std::fs::write(&browser, "#!/bin/sh\nprofile=''\nfor arg in \"$@\"; do case \"$arg\" in --user-data-dir=*) profile=${arg#--user-data-dir=};; esac; done\nprintf '%s' \"$profile\" > \"$BINUS_TEST_PROFILE_MARKER\"\n(sleep 0.5; touch \"$BINUS_TEST_LATE_MARKER\") &\nwait\n").unwrap();
    std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o700)).unwrap();
    let cli = Command::new(env!("CARGO_BIN_EXE_binus-auth-spike"))
        .args([
            "--timeout-seconds",
            "30",
            "auth",
            "login",
            "--allow-plaintext-file",
        ])
        .env("XDG_CONFIG_HOME", temp.path())
        .env("BINUS_CHROMIUM", browser)
        .env("BINUS_TEST_PROFILE_MARKER", &profile_marker)
        .env("BINUS_TEST_LATE_MARKER", &late_marker)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !profile_marker.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(profile_marker.exists(), "synthetic browser did not start");
    let profile = std::path::PathBuf::from(std::fs::read_to_string(&profile_marker).unwrap());
    let pid = rustix::process::Pid::from_raw(cli.id() as i32).unwrap();
    rustix::process::kill_process(pid, signal).unwrap();
    let output = cli.wait_with_output().unwrap();
    assert!(!output.status.success());
    thread::sleep(Duration::from_millis(650));
    assert!(
        !late_marker.exists(),
        "browser descendant survived cancellation"
    );
    assert!(
        !profile.exists(),
        "temporary browser profile survived cancellation"
    );
}

#[cfg(unix)]
#[test]
fn sigint_cancels_login_and_removes_browser_group_and_profile() {
    interrupted_login_is_cleaned_up(rustix::process::Signal::INT);
}

#[cfg(unix)]
#[test]
fn sigterm_cancels_login_and_removes_browser_group_and_profile() {
    interrupted_login_is_cleaned_up(rustix::process::Signal::TERM);
}

#[test]
fn forums_cli_help_missing_and_unsafe_ids_precede_credentials_for_both_binaries() {
    let temp = tempfile::tempdir().unwrap();
    for bin in [
        env!("CARGO_BIN_EXE_binus"),
        env!("CARGO_BIN_EXE_binus-auth-spike"),
    ] {
        for args in [
            vec!["forums", "threads", "list", "--help"],
            vec!["forums", "threads", "show", "--help"],
        ] {
            let help = Command::new(bin).args(args).output().unwrap();
            assert_eq!(help.status.code(), Some(0));
            assert!(String::from_utf8_lossy(&help.stdout).contains("Usage:"));
        }
        let missing = Command::new(bin)
            .args(["forums", "threads", "list", "--class", "c"])
            .env("XDG_CONFIG_HOME", temp.path())
            .output()
            .unwrap();
        assert_eq!(missing.status.code(), Some(2));
        assert!(missing.stdout.is_empty());
        let list_missing = Command::new(bin)
            .args([
                "forums",
                "threads",
                "list",
                "--class",
                "class",
                "--session",
                "session",
            ])
            .env("XDG_CONFIG_HOME", temp.path())
            .output()
            .unwrap();
        assert_eq!(list_missing.status.code(), Some(1));
        let list_body: serde_json::Value = serde_json::from_slice(&list_missing.stdout).unwrap();
        assert_eq!(list_body["data"], serde_json::json!([]));
        assert_eq!(list_body["coverage"]["scope"], "selected_session");
        assert_eq!(list_body["errors"][0]["code"], "not_logged_in");
        let unsafe_id = Command::new(bin)
            .args([
                "forums",
                "threads",
                "list",
                "--class",
                "../x",
                "--session",
                "s",
            ])
            .env("XDG_CONFIG_HOME", temp.path())
            .output()
            .unwrap();
        assert_eq!(unsafe_id.status.code(), Some(2));
        assert!(unsafe_id.stdout.is_empty());
        let no_credentials = Command::new(bin)
            .args(["forums", "threads", "show", "thread", "--class", "class"])
            .env("XDG_CONFIG_HOME", temp.path())
            .output()
            .unwrap();
        assert_eq!(no_credentials.status.code(), Some(1));
        let body: serde_json::Value = serde_json::from_slice(&no_credentials.stdout).unwrap();
        assert_eq!(body["errors"][0]["code"], "not_logged_in");
        assert_eq!(body["data"], serde_json::Value::Null);
        assert_eq!(body["coverage"]["scope"], "selected_thread");
        assert_eq!(body["coverage"]["complete"], false);
        assert_current_timestamp(&body["fetched_at"]);

        for args in [
            vec!["forums", "threads", "show"],
            vec!["forums", "threads", "show", "../unsafe", "--class", "class"],
        ] {
            let invalid = Command::new(bin)
                .args(args)
                .env("XDG_CONFIG_HOME", temp.path())
                .output()
                .unwrap();
            assert_eq!(invalid.status.code(), Some(2));
            assert!(invalid.stdout.is_empty());
        }

        let storage = tempfile::tempdir().unwrap();
        let cred_dir = storage.path().join("binus-auth-spike");
        std::fs::create_dir_all(&cred_dir).unwrap();
        std::fs::create_dir(cred_dir.join("credentials.json")).unwrap();
        let storage_error = Command::new(bin)
            .args(["forums", "threads", "show", "thread", "--class", "class"])
            .env("XDG_CONFIG_HOME", storage.path())
            .output()
            .unwrap();
        assert_eq!(storage_error.status.code(), Some(1));
        let body: serde_json::Value = serde_json::from_slice(&storage_error.stdout).unwrap();
        assert_eq!(body["errors"][0]["code"], "storage_error");
        assert_eq!(body["data"], serde_json::Value::Null);
        assert_eq!(body["coverage"]["scope"], "selected_thread");
        assert_current_timestamp(&body["fetched_at"]);
    }
}
