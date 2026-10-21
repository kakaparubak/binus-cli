use binus_auth_spike::{CredentialRecord, load_record};
use serde::Serialize;
use std::{path::PathBuf, process::ExitCode};

const MAX_BLOB_BYTES: usize = 2560;

#[derive(Serialize)]
struct SizeReport {
    access_token_bytes: usize,
    refresh_token_bytes: usize,
    record_json_bytes: usize,
    max_blob_bytes: usize,
    record_fits: bool,
}

fn size_record(record: &CredentialRecord) -> Result<SizeReport, &'static str> {
    let record_json_bytes = serde_json::to_vec(record)
        .map_err(|_| "record_unavailable")?
        .len();
    Ok(SizeReport {
        access_token_bytes: record.access_token.len(),
        refresh_token_bytes: record.refresh_token.len(),
        record_json_bytes,
        max_blob_bytes: MAX_BLOB_BYTES,
        record_fits: record_json_bytes <= MAX_BLOB_BYTES,
    })
}

#[cfg(unix)]
fn credentials_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    base.join("binus-auth-spike").join("credentials.json")
}

#[cfg(unix)]
fn report_path(path: &std::path::Path) -> Result<SizeReport, &'static str> {
    let record = load_record(path)?;
    size_record(&record)
}

fn failure_json() -> &'static str {
    r#"{"error":"record_unavailable"}"#
}

#[cfg(unix)]
fn run_diagnostic(path: &std::path::Path) -> (ExitCode, String) {
    match report_path(path)
        .and_then(|report| serde_json::to_string(&report).map_err(|_| "record_unavailable"))
    {
        Ok(output) => (ExitCode::SUCCESS, output),
        Err(_) => (ExitCode::FAILURE, failure_json().to_owned()),
    }
}

fn main() -> ExitCode {
    #[cfg(unix)]
    let (code, output) = run_diagnostic(&credentials_path());
    #[cfg(not(unix))]
    let (code, output) = (ExitCode::FAILURE, failure_json().to_owned());
    println!("{output}");
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use binus_auth_spike::{RoleContext, store_record};

    fn synthetic_record(account: &str, access: &str, refresh: &str) -> CredentialRecord {
        CredentialRecord {
            account: account.into(),
            access_token: access.into(),
            refresh_token: refresh.into(),
            role: RoleContext {
                academic_career: "synthetic-role".into(),
                institution: "synthetic-institution".into(),
                name: "synthetic-name".into(),
                role_id: "synthetic-role-id".into(),
                organization_id: "synthetic-org-id".into(),
            },
        }
    }

    fn record_at_size(target: usize) -> CredentialRecord {
        let mut record = synthetic_record("synthetic-account", "x", "y");
        let base = serde_json::to_vec(&record).unwrap().len();
        assert!(target >= base);
        record.access_token = "x".repeat(target - base + 1);
        assert_eq!(serde_json::to_vec(&record).unwrap().len(), target);
        record
    }

    #[test]
    fn reports_exact_serialized_and_token_byte_sizes_without_values() {
        let record = synthetic_record("synthetic-account", "é-access", "synthetic-refresh");
        let report = size_record(&record).unwrap();
        assert_eq!(report.access_token_bytes, record.access_token.len());
        assert_eq!(report.refresh_token_bytes, record.refresh_token.len());
        assert_eq!(
            report.record_json_bytes,
            serde_json::to_vec(&record).unwrap().len()
        );
        let output = serde_json::to_string(&report).unwrap();
        for private_value in [
            "synthetic-account",
            "é-access",
            "synthetic-refresh",
            "synthetic-role",
            "synthetic-institution",
            "synthetic-name",
            "synthetic-role-id",
            "synthetic-org-id",
        ] {
            assert!(!output.contains(private_value));
        }
    }

    #[test]
    fn exact_capacity_fits_and_one_byte_over_is_rejected() {
        let exact = record_at_size(MAX_BLOB_BYTES);
        let over = record_at_size(MAX_BLOB_BYTES + 1);
        assert!(size_record(&exact).unwrap().record_fits);
        assert!(!size_record(&over).unwrap().record_fits);
        assert_eq!(size_record(&exact).unwrap().record_json_bytes, 2560);
        assert_eq!(size_record(&over).unwrap().record_json_bytes, 2561);
    }

    #[cfg(unix)]
    #[test]
    fn absent_record_has_sanitized_failure_and_nonzero_exit_code() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp
            .path()
            .join("binus-auth-spike")
            .join("credentials.json");
        let (code, output) = run_diagnostic(&path);
        assert_eq!(code, ExitCode::FAILURE);
        assert_eq!(output, r#"{"error":"record_unavailable"}"#);
        assert!(!output.contains(path.to_str().unwrap()));
        assert!(!output.contains("not_logged_in"));
    }

    #[cfg(unix)]
    #[test]
    fn invalid_record_has_the_same_sanitized_failure() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let app_dir = temp.path().join("binus-auth-spike");
        std::fs::create_dir(&app_dir).unwrap();
        std::fs::set_permissions(&app_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = app_dir.join("credentials.json");
        std::fs::write(&path, b"synthetic-invalid-record").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (code, output) = run_diagnostic(&path);
        assert_eq!(code, ExitCode::FAILURE);
        assert_eq!(output, r#"{"error":"record_unavailable"}"#);
        assert!(!output.contains(path.to_str().unwrap()));
        assert!(!output.contains("synthetic-invalid-record"));
    }

    #[cfg(unix)]
    #[test]
    fn synthetic_record_is_loaded_only_through_public_validated_loader() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp
            .path()
            .join("binus-auth-spike")
            .join("credentials.json");
        store_record(
            &path,
            &synthetic_record("acct", "synthetic-access-16", "synthetic-refresh-16"),
        )
        .unwrap();
        assert!(report_path(&path).is_ok());
    }
}
