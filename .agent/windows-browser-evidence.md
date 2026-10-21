# Windows browser supervisor evidence

## Scope and safety

This change covers browser discovery, process-tree ownership, capture cancellation, and the Windows production compile boundary. It does not launch a browser, sign in, call BINUS APIs, access Credential Manager, copy credentials, or alter the existing store/lock implementation. No reboot, commit, push, or publication was performed. Native Windows execution remains pending until the Windows boot.

## Documentation and bindings checked

- `CreateProcessW`: explicit `lpApplicationName`, mutable UTF-16 command-line buffer, and `bInheritHandles = FALSE`: [Microsoft CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw).
- `CREATE_SUSPENDED` prevents the primary thread from running until `ResumeThread`: [process creation flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags), [ResumeThread](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-resumethread).
- Job membership and kill-on-close behavior, including default descendant association: [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects), [SetInformationJobObject](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-setinformationjobobject), [AssignProcessToJobObject](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-assignprocesstojobobject), [TerminateJobObject](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-terminatejobobject), [QueryInformationJobObject](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-queryinformationjobobject).
- Known-folder IDs for Program Files (x86), Program Files, and Local AppData and the documented path behavior: [KNOWNFOLDERID](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid), [SHGetKnownFolderPath](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath), [CoTaskMemFree](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/nf-combaseapi-cotaskmemfree).
- Tokio's Windows listeners document Ctrl-C, Ctrl-Break, console-close, logoff, and shutdown notifications: [Tokio Windows signals](https://docs.rs/tokio/latest/tokio/signal/windows/index.html).
- Verified the repository's actual `windows-sys 0.61.2` declarations in the Cargo registry, including `CreateProcessW`, `PROCESS_INFORMATION`, `STARTUPINFOW`, `CREATE_SUSPENDED`, job-object structures/constants, and the signatures for assign/query/terminate/resume/wait. Enabled the corresponding `Win32_System_JobObjects`, `Win32_System_Threading`, `Win32_System_Com`, and `Win32_UI_Shell` features.

## Implementation and lifecycle

- `src/browser.rs` holds the tested Windows command-line quoting, executable selection, launch-state transition, and profile-removal policy shared with the native supervisor.
- `src/browser/native.rs` resolves an explicit `BINUS_CHROMIUM` value as one absolute executable path and rejects an invalid override without fallback. Otherwise it searches known-folder install roots, Edge candidates before Chrome candidates, and does not inspect the current directory/PATH or download a browser.
- The native launcher sets `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, creates the process suspended with an explicit UTF-16 application path and mutable UTF-16 command line, assigns it to a non-inheritable job, then resumes it. No breakaway flag is set. On assignment/resume failure it terminates and waits for the still-suspended root; if termination cannot be confirmed, it retains the profile and handles rather than pretending cleanup completed.
- The browser guard owns the profile directory. Cancellation/drop and normal capture completion terminate the owned job when needed, wait with bounded waits, and query the job until no processes remain before allowing profile removal. Failure to confirm termination returns a sanitized cleanup error; the guard preserves the profile in that case. Closing the job handle remains the abrupt-parent-exit kill fallback; an uncatchable exit cannot guarantee TempDir cleanup.
- `capture_inner` routes Windows through the supervisor while retaining the existing Linux process group, Chromium override/default, Wayland flags, and Wayland environment. Windows receives no Wayland flags or environment. Existing isolated profile, `about:blank`, loopback CDP address, ephemeral port, redirect policy, and CDP guards remain on the existing capture path.
- Windows cancellation listens for Ctrl-C and Ctrl-Break, plus Tokio's console-close/logoff/shutdown events as best-effort notifications. Windows may impose short termination deadlines for those system events; forced OS termination can bypass normal profile cleanup. Unix Ctrl-C/SIGTERM handling is unchanged.
- Native `cfg(test)` source uses only the test executable as an inert helper tree and verifies owned-job cleanup, drop cleanup, and survival of a separate helper process. It also creates a helper descendant under the owned process. Assignment/resume failure injection is intentionally absent; those failure paths are compiled but native runtime fault injection was not performed.

## TDD and checks

Shared policy/argument tests were added before the native supervisor. They exercise UTF-16-compatible CRT quoting cases (spaces, Unicode, embedded quotes, trailing backslashes), explicit-override no-fallback selection, Edge-before-Chrome candidate order, launch state transitions, and profile removal only after confirmed termination.

- `cargo test --manifest-path spikes/001-auth/Cargo.toml --all-targets -- --test-threads=1`: passed (99 library, 11 CLI, and existing example tests; 0 failures).
- `cargo fmt --manifest-path spikes/001-auth/Cargo.toml -- --check`: passed.
- `cargo clippy --manifest-path spikes/001-auth/Cargo.toml --all-targets -- -D warnings`: passed.
- `cargo build --manifest-path spikes/001-auth/Cargo.toml --release`: passed.
- `RUSTFLAGS='-D warnings' cargo check --manifest-path spikes/001-auth/Cargo.toml --lib --bins --target x86_64-pc-windows-msvc`: passed with warnings denied.
- `RUSTFLAGS='-D warnings' cargo check --manifest-path spikes/001-auth/Cargo.toml --tests --target x86_64-pc-windows-msvc`: passed; this typechecks the native helper-process test source without running it.
- `cargo check --manifest-path spikes/001-auth/Cargo.toml --all-targets --target x86_64-pc-windows-msvc`: passed. Existing `credential_size` example code emits five unused/dead-code warnings on Windows because its implementation is Unix-gated; there were no test fixture compile errors after only the individual Unix-specific fixtures/imports were gated.
- Native Windows tests were typechecked by the Windows `--all-targets` check, not executed. Runtime validation of Job Objects and Windows cancellation is still pending Windows boot. Existing Credential Manager and native lock runtime validation also remains pending as previously recorded.
- `git diff --check`: passed.

## File hashes

Pre-edit SHA-256 values were saved before edits for existing files. `tests/cli.rs`'s initial value was reconstructed by reversing only this pass's two-line import gating change; no other changes to that file were made in this pass.

| File | Pre-edit SHA-256 | Current SHA-256 |
|---|---|---|
| `spikes/001-auth/src/lib.rs` | `47d56239a1a11ef0efeed7734268517a659f357d01f08d42e5fdc206aee79a91` | `cbb0242f4094b3901111c4685cc65a19157075679a3a6baafff54f737649bc07` |
| `spikes/001-auth/src/cli.rs` | `423f20caf20aaca17963283a980131c1d2ac3c72faa97cc55f6eea3699cd3f87` | `25528bcb28197c7dc30997029048614207f8995db5454a2a229c35df7331b67f` |
| `spikes/001-auth/Cargo.toml` | `a9f1efed5ba74bb93b90288431b1ab1496072aec377693d416a4440690839d0e` | `69c38b0f4ff88446dcafa589081f9edb3dc0481d1b73a1311c850502a30bb631` |
| `spikes/001-auth/tests/cli.rs` | `b67ebb2119c1ca81423cf42b39ab646c3f64c2de1e3f6f333a457edd084075c0` | `36935f3098b1eea82f88325cd7c15d0bf80711fbfc8b6700fe30f4f5d673c224` |
| `spikes/001-auth/Cargo.lock` | `b7626e180a7374d4bcfaa9afaa89bf7abb037a52ad3f4ee4a0fb5e9190ce9d69` | `b7626e180a7374d4bcfaa9afaa89bf7abb037a52ad3f4ee4a0fb5e9190ce9d69` |
| `spikes/001-auth/src/windows_store.rs` | `5db7138cc229eff382e67f924fac6a25eee1ad5e837db39223c4f76bc6fd781c` | `5db7138cc229eff382e67f924fac6a25eee1ad5e837db39223c4f76bc6fd781c` |
| `spikes/001-auth/src/windows_lock/native.rs` | `d59a49a8838c887604909fcb2c16ab002c3d20e68a43ea64f57cf96ca8d464b3` | `d59a49a8838c887604909fcb2c16ab002c3d20e68a43ea64f57cf96ca8d464b3` |
| `spikes/001-auth/src/browser.rs` | New file | `ed64afa590e9e9da657e2294ecea3bd6844467365d2940e68a4d2d7f7e863096` |
| `spikes/001-auth/src/browser/native.rs` | New file | `6b467b8c3e99fa5318d2f47c7919437027faa99e28c90840f2c2d0ba1556eac3` |
