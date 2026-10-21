#[cfg(any(windows, test))]
use std::path::{Path, PathBuf};

/// Encode one argument using the Windows CRT command-line parsing rules.
#[cfg(any(windows, test))]
pub(crate) fn quote_windows_argument(value: &str) -> String {
    if !value.is_empty() && !value.chars().any(|c| c.is_whitespace() || c == '"') {
        return value.to_owned();
    }
    let mut out = String::from("\"");
    let mut slashes = 0usize;
    for ch in value.chars() {
        match ch {
            '\\' => slashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
                out.push('"');
                slashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', slashes));
                slashes = 0;
                out.push(ch);
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', slashes * 2));
    out.push('"');
    out
}

#[cfg(any(windows, test))]
pub(crate) fn select_executable(
    override_path: Option<&Path>,
    candidates: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(path) = override_path {
        return (path.is_absolute() && path.is_file()).then(|| path.to_owned());
    }
    candidates
        .iter()
        .find(|path| path.is_absolute() && path.is_file())
        .cloned()
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LaunchState {
    Suspended,
    Assigned,
    Running,
}

#[cfg(any(windows, test))]
pub(crate) fn assigned_to_job(state: LaunchState) -> Option<LaunchState> {
    (state == LaunchState::Suspended).then_some(LaunchState::Assigned)
}

#[cfg(any(windows, test))]
pub(crate) fn resumed(state: LaunchState) -> Option<LaunchState> {
    (state == LaunchState::Assigned).then_some(LaunchState::Running)
}

#[cfg(any(windows, test))]
pub(crate) const fn profile_cleanup_allowed(confirmed: bool) -> bool {
    confirmed
}

#[cfg(windows)]
mod native;

#[cfg(windows)]
pub(crate) use native::Browser;

#[cfg(windows)]
pub(crate) fn resolve_executable() -> Result<PathBuf, &'static str> {
    native::resolve_executable()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn windows_quoting_handles_spaces_unicode_quotes_and_trailing_slashes() {
        assert_eq!(
            quote_windows_argument(r#"C:\Program Files\Edge\msedge.exe"#),
            r#""C:\Program Files\Edge\msedge.exe""#
        );
        assert_eq!(
            quote_windows_argument("路径 with spaces\\"),
            "\"路径 with spaces\\\\\""
        );
        assert_eq!(quote_windows_argument("a\\\"b"), "\"a\\\\\\\"b\"");
        assert_eq!(quote_windows_argument("plain"), "plain");
    }

    #[test]
    fn explicit_override_is_single_path_and_never_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let explicit = dir.path().join("browser path.exe");
        let candidate = dir.path().join("installed.exe");
        fs::write(&candidate, b"candidate").unwrap();
        assert_eq!(
            select_executable(Some(&explicit), std::slice::from_ref(&candidate)),
            None
        );
        fs::write(&explicit, b"explicit").unwrap();
        assert_eq!(
            select_executable(Some(&explicit), &[candidate]),
            Some(explicit)
        );
    }

    #[test]
    fn candidates_keep_declared_edge_before_chrome_order() {
        let dir = tempfile::tempdir().unwrap();
        let edge = dir.path().join("edge.exe");
        let chrome = dir.path().join("chrome.exe");
        fs::write(&edge, b"edge").unwrap();
        fs::write(&chrome, b"chrome").unwrap();
        assert_eq!(select_executable(None, &[edge.clone(), chrome]), Some(edge));
    }

    #[test]
    fn launch_and_cleanup_policy_is_used_by_native_supervisor() {
        assert_eq!(
            assigned_to_job(LaunchState::Suspended),
            Some(LaunchState::Assigned)
        );
        assert_eq!(resumed(LaunchState::Assigned), Some(LaunchState::Running));
        assert_eq!(resumed(LaunchState::Suspended), None);
        assert_eq!(assigned_to_job(LaunchState::Running), None);
        assert!(!profile_cleanup_allowed(false));
        assert!(profile_cleanup_allowed(true));
    }
}
