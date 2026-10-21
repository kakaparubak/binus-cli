# Windows portability preflight evidence

Date: 2026-10-20 (Asia/Jakarta)
Scope: compiler-backed audit only. No product source or dependency edits were made for this preflight. No native Windows runtime claim is made.

## Compiler evidence

Environment before setup: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1 (5f94df478 2026-08-27)`, installed target only `x86_64-unknown-linux-gnu`, and no `x86_64-w64-mingw32-gcc` on PATH.

Commands and results:

1. `rustup target add x86_64-pc-windows-msvc` — standard library installation completed; `rustup target list --installed` then showed both `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`. The initial component download emitted no progress for about a minute; it subsequently completed. This is target setup, not a compiler error.
2. An initial premature `cargo check --manifest-path spikes/001-auth/Cargo.toml --target x86_64-pc-windows-msvc --all-targets` while target installation was still pending failed with `E0463: can't find crate for core` and the rustc hint that the target may not be installed. This result is superseded and is not the project portability finding.
3. Re-ran the exact check after target installation: `cargo check --manifest-path spikes/001-auth/Cargo.toml --target x86_64-pc-windows-msvc --all-targets`. Cargo fetched/compiled dependencies, then failed in `binus-auth-spike` (exit 101): library: 27 errors; lib test: 47 errors. Dependency resolution succeeded. The output is concrete project compiler failure, not a linker/toolchain setup blocker.

Representative first-order errors and locations:

- `src/lib.rs:3`: unresolved `rustix::fs`; rustix 1.1.5 source notes the module is `cfg(not(windows))`.
- `src/lib.rs:14`, `104`, `352`, `378`: `std::os::fd`/`OwnedFd` absent on Windows.
- `src/lib.rs:146`, `174`, `226`, `297`, `343`, `355`, `381`, `395`, `400`: Unix `rustix::process`/filesystem symbols are absent, including `geteuid`, `flock`, and errno constants; related fd and raw-fd conversions also fail (`189`, `230`).
- `src/lib.rs:651-652`: `std::os::unix::process::CommandExt` and `Command::process_group` are unavailable.
- `src/cli.rs:466`: `tokio::signal::unix` is unavailable on Windows.
- `src/lib.rs:947`, `1236-1245`, `1263+`, `1327+`, `1341+`, `1385+`, `1475+`, `1614-1620`: test module imports Unix `PermissionsExt`, checks Unix permission bits, creates Unix symlinks, and uses Unix `process_group`; these also fail under `--all-targets`.
- `src/lib.rs:272-273` is a further runtime-only Linux dependency: `/proc/self/fd/<fd>` enumeration for artifact cleanup. It does not itself form a separate diagnostic in the current compile because preceding fd names already fail.

No source was edited to suppress these errors or to make a placeholder backend compile.

## Platform assumption inventory

| Area | Existing assumption and locations | Portability consequence |
|---|---|---|
| Storage root | `src/cli.rs:93-100` resolves `XDG_CONFIG_HOME`, otherwise `$HOME/.config`; no Windows known-folder resolution. | Needs a defined Windows base path and missing/invalid environment behavior. Microsoft recommends `SHGetKnownFolderPath` for new known-folder code; `%APPDATA%` is the documented default for `FOLDERID_RoamingAppData`. |
| Storage security | `src/lib.rs:104-156` walks from `/` with `openat`/`NOFOLLOW`, checks Unix uid/mode/sticky bits; `159-201` uses `fchmod`, exclusive no-follow temp creation, rename and fsync; `203-254` opens no-follow and checks owner/mode; `256-361` removes records and checks `st_uid`/mode; `378-401` opens `.auth-spike.lock` and takes `flock`. | These are security properties, not portable permission syntax. Windows needs its own owner/DACL/reparse-point/atomic-write/locking design or OS credential store. Do not map `0700`/`0600` to `Permissions` or accept inherited ACLs without validation. |
| Cleanup | `src/lib.rs:256-313`, especially `272-280`, lists entries by `/proc/self/fd/<dirfd>` and removes only validated names through dirfd-relative operations. | Must be redesigned around a Windows handle-based directory enumeration/deletion approach or the file artifact design should be avoided. |
| Browser discovery/launch | `src/lib.rs:645-669`: defaults to `/usr/bin/chromium`; supports `BINUS_CHROMIUM`; creates a temp profile; sets `process_group(0)` and hard-coded Wayland `XDG_RUNTIME_DIR=/run/user/1000`, `WAYLAND_DISPLAY=wayland-1`; starts CDP and polls `DevToolsActivePort` thereafter. | Windows needs Chrome/Edge executable discovery or an explicit binary setting, Windows argument/path handling, no Linux GUI env, a loopback-only debugging endpoint, and cleanup that actually terminates the browser tree. |
| Browser cleanup | `src/lib.rs:442-453` kills Linux process group with TERM then KILL, followed by child kill/wait; group creation is only at `651-652`. | `Child::kill` targets only the direct child. The Windows process-tree equivalent needs a Job Object and failure-safe assignment/close semantics. |
| Signals | `src/cli.rs:463-472`: cancellation races capture against Ctrl-C and Unix SIGTERM. | Retain Ctrl-C and define Windows console close/cancel semantics; no Unix SIGTERM API exists. |
| Unit tests | `src/lib.rs:944-...`: tests assert Unix file modes and symlink/no-follow behavior; `1612-1628` uses `/bin/sh`, `sleep`, and process groups. | Keep Linux security tests Linux-specific; add Windows DACL, reparse-point, atomic replacement, locking, and job cleanup tests rather than weakening/skipping the guarantees. |
| CLI integration tests | `tests/cli.rs:21+` override `XDG_CONFIG_HOME`; Unix-specific fixtures at `204+`, `228+`, `278+`, `284+` use permission bits, shell scripts, signals and Unix symlinks. | Split shared CLI behavior from platform fixtures; Windows tests need native executable/console and storage-path fixtures. |
| Cargo | `Cargo.toml:20` enables Tokio `signal`; `:24` unconditionally includes rustix `fs, process`; current source imports both unconditionally. | Gate platform-specific modules/dependencies only after adding a real backend; gating alone is not proof of Windows protections. `rustix` itself compiled as a dependency but does not expose `fs`/`process` on Windows. |

## Secure backend recommendation

Recommend Windows Credential Manager generic credentials as the first Windows backend, behind a small storage interface, while retaining the existing Linux file backend and its explicit plaintext-file consent unchanged. This is less platform-security code than recreating Unix dirfd protections with Windows DACL and reparse-point-safe handle operations. Microsoft documents `CredWriteW` credentials as associated with the current token's logon session, and generic credentials support long-term storage; it also says generic credentials can be read and written by user processes. Treat it as OS-managed storage within the current-user trust boundary, not protection from malicious code already running as that user.

Important fit limit: Microsoft caps `CredentialBlobSize` at `CRED_MAX_CREDENTIAL_BLOB_SIZE`, 5*512 = 2560 bytes. Serialized token pairs may exceed this; measure only synthetic/realistic size fixtures in implementation tests (never inspect a user's vault). Reject oversize data with a clear error; do not spill to plaintext or silently switch backend. If a valid BINUS token pair exceeds the limit, stop and present the ACL-backed file option for a deliberate decision rather than silently designing around it. Credential Manager would store the JSON payload as an opaque generic blob; no claim is made here about encryption implementation or protection from same-user processes.

Sources checked (Microsoft primary documentation):

- [CredWriteW](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew) — write/replace and current-token logon session behavior.
- [CREDENTIALW](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentialw) — 2560-byte blob cap and generic blob semantics.
- [Kinds of Credentials](https://learn.microsoft.com/en-us/windows/win32/secauthn/kinds-of-credentials) — generic credential long-term storage and user-process read/write scope.
- [Known Folders](https://learn.microsoft.com/en-us/windows/win32/shell/known-folders) and [KNOWNFOLDERID](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid) — use `SHGetKnownFolderPath`; Roaming AppData default is `%APPDATA%`.
- [File Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/fileio/file-security-and-access-rights) — ACL inheritance and explicit security descriptors.
- [CreateFile](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea) and [Reparse Point Operations](https://learn.microsoft.com/en-us/windows/win32/fileio/reparse-point-operations) — use `FILE_FLAG_OPEN_REPARSE_POINT` when operating on reparse points.
- [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects) — `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` can terminate associated processes when the last job handle closes.

## Native acceptance status

Native Windows execution is pending parent access. Configured Windows SSH host `100.68.214.97` timed out from the parent; this preflight did not retry or contact other hosts. No Windows machine writes, login, browser launch, live API, or credential directories/vault were accessed. Cross-target `cargo check` is compile evidence only and does not establish runtime, ACL, credential-manager, browser, or login security.

## Short ordered next batch

1. Add a narrow `CredentialStore` boundary; retain Linux implementation and its existing consent/security behavior. Implement Windows generic Credential Manager with bounded blob size, strict error propagation, scoped names, save/load/delete, and no plaintext fallback. Resolve the oversized-token behavior before enabling login on Windows.
2. Port Windows path selection and browser runtime: known-folder path for any non-secret profile metadata, Chrome/Edge discovery/override, loopback CDP, and Job Object based child-tree cleanup. Preserve the temporary isolated browser profile and ensure teardown before profile deletion.
3. Add target-specific storage, reparse-point, cancellation, browser cleanup and CLI acceptance tests; run the existing Linux `cargo test --all-targets -- --test-threads=1` and formatting check unchanged, then run Windows native tests and an interactive login/logout/status acceptance before claiming Windows support. Package Windows x64 only after those pass.

Behavior decisions to obtain before implementation: (a) accept Credential Manager as Windows's protected default without the Linux plaintext-consent flag, while retaining that flag only for explicit plaintext-file backend use; (b) require fresh login rather than automatically importing any existing credentials JSON into Credential Manager; (c) if token data exceeds 2560 bytes, pause Windows login for a reviewed ACL-backed design instead of falling back.
