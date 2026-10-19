use std::process::{Command, Stdio};

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
    assert!(body["fetched_at"].as_str().unwrap() > "2025-01-01T00:00:00Z");
    let unsafe_period = Command::new(bin)
        .args(["courses", "list", "--period", "../x"])
        .env("XDG_CONFIG_HOME", temp.path())
        .output()
        .unwrap();
    assert_eq!(unsafe_period.status.code(), Some(2));
    assert!(unsafe_period.stdout.is_empty());
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
    assert!(body["fetched_at"].as_str().unwrap() > "2025-01-01T00:00:00Z");
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
