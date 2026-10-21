# BINUS CLI: private Windows native-test checkpoint

This is a development source checkpoint, NOT a Windows executable or an alpha release. It contains no saved credentials or browser profiles. Source/compile review passed; native Windows behavior is unverified.

## Before testing

1. Boot Windows 11 and sign into your normal Windows desktop account.
2. Extract this archive into a new directory. Do not merge it into an unrelated project.
3. Use a normal, non-administrator PowerShell window for Credential Manager tests. An SSH/network logon can lack the credential set used by the interactive desktop; do not treat that as a proven application defect.
4. Check prerequisites without installing anything:

```powershell
rustc --version
cargo --version
```

The MSVC Rust toolchain also needs Microsoft C++ build tools/Windows SDK to link. Their availability has not been checked on this Windows installation. If the commands are missing or linking fails, stop and share the sanitized error; no automatic installation is part of this bundle.

## First native check (synthetic fixtures only)

From the extracted directory containing Cargo.toml:

```powershell
cargo test --lib -- --test-threads=1
cargo build --release --bin binus
.\target\release\binus.exe --help
```

The library tests include isolated synthetic Windows Credential Manager fixtures, the Windows mutex tests, and inert process-tree fixtures. They are intended to touch only their own exact test credentials/processes. Do not run them as administrator. These tests have been typechecked, not run on Windows yet. Preserve any failure output for diagnosis.

Do not run the entire CLI integration suite as a substitute for this first native check: some CLI fixture assumptions are still platform-specific, and the normal application storage target must not be used as an empty test vault.

## Live own-account smoke test (only after native tests pass)

```powershell
.\target\release\binus.exe auth login
.\target\release\binus.exe auth status
.\target\release\binus.exe auth check
.\target\release\binus.exe courses periods
```

You enter your own Microsoft password/MFA in the browser, never in chat or logs. Windows uses Credential Manager; the Linux-only plaintext consent flag is not a Windows option. Auth status is a local stored-state check, not proof of live validity. The live sequence, selected course/session/forum commands, Ctrl-C cancellation, browser/profile cleanup and logout still require supervised verification before Windows support can be claimed.

Do not paste tokens, accounts, role IDs, signed URLs, private forum bodies, or raw browser data into bug reports. Share only sanitized error codes and test names. Do not publish this checkpoint as a release.
