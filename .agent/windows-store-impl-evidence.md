# Windows Credential Manager backend implementation evidence

Date: 2026-10-20 (Asia/Jakarta)

## Scope and invariants

Implemented the Credential Manager store and narrow storage facade. The Linux directory-fd, UID, no-follow, fsync, rename, and flock implementation remains on its existing non-Windows path. The shared workspace already had unrelated accepted course/session/forum changes; those were left intact. No credentials were read from the real store, and no login, browser, live API, reboot, native Windows execution, commit, or publication occurred.

The Windows adapter stores exactly one JSON `CredentialRecord` per generic credential blob. It uses fixed versioned targets, accepts only the `credentials.json` and `credentials.captured.json` logical selectors, sets `CRED_PERSIST_LOCAL_MACHINE`, bounds data to 2,560 bytes, and wraps returned `CredReadW` memory in `CredFree` RAII before any return path. Only `ERROR_NOT_FOUND` maps to absence; all other native failures are sanitized storage errors. Each facade operation acquires the reviewed SID-scoped lock synchronously. Preferred selection, full candidate compare/write/delete promotion, and both logout deletes are inside that same lock.

Credential records and validation moved to `credential_record.rs` with public `CredentialRecord`/`RoleContext` re-exports preserved. `Debug` now redacts account, tokens, and all role values. On Windows the plaintext-consent CLI option is cfg-omitted; this is storage wiring only and does not imply Windows login/browser portability.

## Regression and executable tests

- Initial focused engine test command: `cargo test --manifest-path spikes/001-auth/Cargo.toml windows_store::tests --lib -- --test-threads=1` — compile failed because the test API's closed selector key lacked `Hash`. Added the derive; the focused engine tests then passed. Later focused run: 6 passed.
- Engine coverage uses the actual orchestration consumed by the Windows adapter with an in-memory `Api`: roundtrip; malformed JSON, token, role, empty and oversized blobs; actual serialized JSON exactly 2,560 bytes accepted and 2,561 rejected before write; old record retained; candidate exact promotion, mismatch, primary write failure and candidate-delete failure; preferred fallback only for absence; API error propagation; both logout deletes after one failure; unrelated mock entry untouched; invalid selector rejected; lock held across candidate read/compare, primary write and candidate deletion.
- `cargo test --manifest-path spikes/001-auth/Cargo.toml --all-targets -- --test-threads=1` — PASS, 94 library tests plus CLI and example test targets.
- The lock-scope test uses the same generic `with_lock` helper as the production Windows wrapper. Native mutex execution remains pending on Windows.

The implementation was developed iteratively. A separate red transcript was not preserved for every individual acceptance assertion; the saved red/green record above is the actual focused compile regression and its correction, followed by the final passing executable suites.

## Native API verification

Microsoft documents `CredReadW` as reading the current token's credential set, with `ERROR_NOT_FOUND` indicating absence and the returned allocation freed by `CredFree`: [CredReadW](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credreadw). `CredWriteW` creates/replaces a target and associates it with the current token: [CredWriteW](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew). `CredDeleteW` deletes by target and type: [CredDeleteW](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-creddeletew). Microsoft documents `CRED_PERSIST_LOCAL_MACHINE` as persisting across logons for the same user on the same computer, without roaming it to other computers: [CREDENTIALW persistence](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentialw).

Checked the actual pinned `windows-sys 0.61.2` bindings and enabled `Win32_Security_Credentials`. Adapter calls use binding-provided `CredReadW`, `CredWriteW`, `CredDeleteW`, `CredFree`, `CREDENTIALW`, `CRED_TYPE_GENERIC`, and `CRED_PERSIST_LOCAL_MACHINE`; no handwritten FFI declarations.

## Commands and results

- `cargo fmt --manifest-path spikes/001-auth/Cargo.toml --check` — PASS.
- `cargo clippy --manifest-path spikes/001-auth/Cargo.toml --all-targets -- -D warnings` — PASS.
- `cargo build --manifest-path spikes/001-auth/Cargo.toml --release` — PASS (actual Linux release build).
- `RUSTFLAGS='-D warnings' cargo check --manifest-path .agent/windows-store-check/Cargo.toml --all-targets --target x86_64-pc-windows-msvc` — PASS. Harness imports actual record types, store source and reviewed lock source.
- `RUSTFLAGS='-D warnings' cargo check --manifest-path .agent/windows-lock-check/Cargo.toml --all-targets --target x86_64-pc-windows-msvc` — PASS.
- The harness's Windows `cfg(test)` native store/lock test code is type-checked by the all-targets Windows check. It has not been executed on Windows.
- `CARGO_TARGET_DIR=.agent/windows-main-check-target cargo check --manifest-path spikes/001-auth/Cargo.toml --all-targets --target x86_64-pc-windows-msvc` — expected failure outside storage: 22 library errors and 42 test-target errors. Diagnostics are confined to Unix-specific `rustix::fs`/`rustix::process`, `std::os::fd`/`std::os::unix`, Unix signal handling, `Command::process_group`, Unix `Errno` constants/`from_raw_fd`, and Unix `PermissionsExt::{mode,from_mode}` tests. The saved full compiler output is `.agent/windows-store-impl-main-windows.log`. No storage adapter or mutex type errors remain. This does not establish whole-CLI Windows compatibility.

## Baseline and hashes

The shared worktree was already dirty at task start (`Cargo.toml`, `Cargo.lock`, `README.md`, `cli.rs`, `lib.rs`, and CLI tests were modified; the reviewed lock source was untracked). Therefore exact pre-task working-tree hashes for those accepted files cannot be reconstructed. To avoid presenting invented baselines, these are the repository `HEAD` blob IDs, not pre-task worktree hashes:

- `lib.rs`: `332841ee0da425613952c1a40c1fe9720d6544e9`
- `cli.rs`: `5e36d8393c60d35419572162eee454eb41da32c4`
- `Cargo.toml`: `593c7443e3c4743451d45362bc84b09d41303264`
- `Cargo.lock`: `bc6a9f9f209ae2c582feafd71d3782010df6e72f`

Current SHA-256 values:

- `lib.rs`: `60930982cc5cc8f46a1942f8111cb372c53f0f8303374d88f0b02eb82fdd8008`
- `cli.rs`: `423f20caf20aaca17963283a980131c1d2ac3c72faa97cc55f6eea3699cd3f87`
- `credential_record.rs`: `4c9742d4c9bd8268e5642cf54a5d8a0d9c8f5786fcaa3b2936e9b4f3ed630db4`
- `windows_store.rs`: `bdcb2c373bfd0e788b415e1c59bcb2e9dc6bd02aa746176f4eb4c870939719ae`
- `windows_lock/native.rs`: `d59a49a8838c887604909fcb2c16ab002c3d20e68a43ea64f57cf96ca8d464b3`
- `Cargo.toml`: `a9f1efed5ba74bb93b90288431b1ab1496072aec377693d416a4440690839d0e`
- `Cargo.lock`: `b7626e180a7374d4bcfaa9afaa89bf7abb037a52ad3f4ee4a0fb5e9190ce9d69`

The only lock follow-up edits were the two requested `#[cfg(test)]` gates on test-only named acquisition helpers and the stale regression placeholder correction in `.agent/windows-lock-fix1-evidence.md`. Reviewed mutex design was not reopened.

## Correction 1/2: Windows store cfg integration and native test source (2026-10-20)

This appended correction preserves the implementation evidence above as a record of that earlier turn and supersedes its Windows-test and main-target error-attribution statements. The original adapter had **no native WinCred operation tests**; its `native_tests` module contained only fixed-target and size assertions. The store/lock test code did type-check, but that did not establish native WinCred test coverage. This correction adds three Windows-only tests that invoke the real `WinApi`, storage engine, and reviewed mutex with test-only per-test targets, plus exact-target RAII cleanup. They are source/typechecked only here, not executed.

The earlier statement that the main Windows errors were all outside storage was too broad. After gating Unix-only storage imports and private helpers, the actual main `--lib` diagnostics are the known Unix `CommandExt::process_group` and Tokio Unix-signal production code. Main `--all-targets` additionally reports the legacy Unix-only permission/symlink fixture errors; those test fixtures were not hidden by gating the whole test module. Exact current compiler output is `.agent/windows-store-fix1-main-windows.log`.

The previous baseline note incorrectly attributed missing snapshots to a dirty tree. A dirty tree does not prevent hashing its working files; those earlier pre-task snapshots simply were not captured. For this correction, hashes were saved before edits and match the supplied parent snapshot exactly. The fresh before/after hashes, exact correction files, checks, source review, native-pending status, and future Windows commands are in `.agent/windows-store-fix1-evidence.md`.
