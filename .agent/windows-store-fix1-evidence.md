# Windows store correction 1/2 evidence

Date: 2026-10-20 (Asia/Jakarta). Scope: Unix storage cfg cleanup and isolated Windows Credential Manager operation-test source only.

## Baseline captured before edits

The following working-file SHA-256 values were recorded before editing and match the supplied parent snapshot exactly:

| File | Before SHA-256 |
| --- | --- |
| `spikes/001-auth/src/lib.rs` | `60930982cc5cc8f46a1942f8111cb372c53f0f8303374d88f0b02eb82fdd8008` |
| `spikes/001-auth/src/cli.rs` | `423f20caf20aaca17963283a980131c1d2ac3c72faa97cc55f6eea3699cd3f87` |
| `spikes/001-auth/src/credential_record.rs` | `4c9742d4c9bd8268e5642cf54a5d8a0d9c8f5786fcaa3b2936e9b4f3ed630db4` |
| `spikes/001-auth/src/windows_store.rs` | `bdcb2c373bfd0e788b415e1c59bcb2e9dc6bd02aa746176f4eb4c870939719ae` |

The shared worktree was already dirty. That did not prevent capturing these hashes. Earlier implementation evidence incorrectly said otherwise: those earlier pre-task snapshots were simply not recorded. The correction to that claim and the previous Windows error attribution is appended to `.agent/windows-store-impl-evidence.md`; its earlier entries remain as historical evidence, not current conclusions.

## Changes in this correction

Code files changed by this correction:

- `spikes/001-auth/src/lib.rs`: gated rustix and fd/io imports, the Unix storage facade functions, and private Unix filesystem/locking helpers with `cfg(not(windows))`. `Path` remains Unix-only; `PathBuf` remains available to Unix code and tests; Windows Unix-filesystem test fixtures retain their test-only `std::fs` import. The Linux filesystem function bodies were not changed.
- `spikes/001-auth/src/windows_store.rs`: added `cfg(test)` namespace routing within the real `WinApi`, with no production target override. Production still maps only `Key::Primary` and `Key::Candidate` to the existing fixed versioned names. Added three native tests for real adapter save/load/delete, candidate preference and verified promotion, and logout preserving a separate synthetic credential in the same generic Credential Manager keyspace. Each test gets a pid/time/atomic-unique namespace; exact test targets are checked absent before use. The test adapter records only its successful exact writes and its `Drop` deletes only those exact names. No enumeration, prefix delete, environment override, or arbitrary production target was added.

The memory mock field is now named `unrelated_sentinel`; it is an out-of-keyspace sentinel, not a Credential Manager entry. The native logout test separately creates and checks the unrelated synthetic API-keyspace credential through `CredWriteW`/`CredReadW` and verifies it survives the actual logout orchestration.

The three new native test functions are `native_save_load_delete_roundtrip_uses_isolated_credentials`, `native_candidate_preference_verified_promotion_and_cleanup`, and `native_logout_preserves_unrelated_test_credential_in_same_keyspace`. Each calls `native::locked_isolated`, which obtains the actual reviewed `windows_lock::acquire` guard and invokes the existing engine (`store`, `load`, `preferred`, `promote`, `logout`) against `WinApi`. `WinApi::read/write/delete` still make the real WinCred calls. The unrelated test helper uses the same `CREDENTIALW`, `CredWriteW`, `CredReadW`, and exact-name `CredDeleteW` bindings.

No Linux-executable regression was applicable to the new Windows-only code. Native tests are **typechecked, not run**; no red/green claim is made for Windows runtime behavior. Existing Linux tests exercise the shared engine. The exact `windows-sys 0.61.2` binding declarations were inspected in its installed `Win32/Security/Credentials/mod.rs` (`CredReadW`, `CredWriteW`, `CredDeleteW`, `CredFree`, and `CREDENTIALW`). Microsoft WinCred docs reviewed: [CredReadW](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credreadw), [CredWriteW](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew), [CredDeleteW](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-creddeletew), [CredFree](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credfree), and [CREDENTIALW persistence](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentialw). No new handwritten FFI was introduced.

## Checks and results

- `cargo test --manifest-path spikes/001-auth/Cargo.toml --all-targets -- --test-threads=1` — PASS: 94 library tests, 11 CLI integration tests, and example tests.
- `cargo fmt --manifest-path spikes/001-auth/Cargo.toml -- --check` — PASS.
- `cargo clippy --manifest-path spikes/001-auth/Cargo.toml --all-targets -- -Dwarnings` — PASS.
- `cargo build --manifest-path spikes/001-auth/Cargo.toml --release` — PASS, actual Linux release build.
- `RUSTFLAGS=-Dwarnings cargo check --manifest-path .agent/windows-store-check/Cargo.toml --target x86_64-pc-windows-msvc --all-targets --tests` — PASS. Imports the real record, store, and lock modules; checks the native store test source.
- `RUSTFLAGS=-Dwarnings cargo check --manifest-path .agent/windows-lock-check/Cargo.toml --target x86_64-pc-windows-msvc --all-targets --tests` — PASS.
- `cargo test --manifest-path .agent/windows-store-check/Cargo.toml --target x86_64-pc-windows-msvc --all-targets --no-run` — unable to link because `link.exe` is absent on this Linux host. Windows native tests remain **PENDING** execution on Windows.
- Main `cargo check --manifest-path spikes/001-auth/Cargo.toml --lib --target x86_64-pc-windows-msvc` — exit 101. Storage-origin import/helper errors are gone. Remaining four production diagnostics are the Unix `std::os::unix::process::CommandExt`/`Command::process_group` code in `lib.rs` and Tokio Unix terminate-signal code in `cli.rs`.
- Main `cargo check --manifest-path spikes/001-auth/Cargo.toml --all-targets --target x86_64-pc-windows-msvc` — exit 101. It includes those same production blockers plus legacy Unix-only test fixtures (`std::os::unix`, `PermissionsExt::{mode,from_mode}`, and a test `Command::process_group`). The existing test module was not gated or deleted. Neither command demonstrates a complete Windows CLI build.
- Exact full output and exit markers: `.agent/windows-store-fix1-main-windows.log`.

The isolated `cargo check` commands use the workspace harness target directories, not `/tmp`. No Windows program was run here.

## Before/after hashes

| File | Before SHA-256 | After SHA-256 |
| --- | --- | --- |
| `spikes/001-auth/src/lib.rs` | `60930982cc5cc8f46a1942f8111cb372c53f0f8303374d88f0b02eb82fdd8008` | `47d56239a1a11ef0efeed7734268517a659f357d01f08d42e5fdc206aee79a91` |
| `spikes/001-auth/src/cli.rs` | `423f20caf20aaca17963283a980131c1d2ac3c72faa97cc55f6eea3699cd3f87` | `423f20caf20aaca17963283a980131c1d2ac3c72faa97cc55f6eea3699cd3f87` |
| `spikes/001-auth/src/credential_record.rs` | `4c9742d4c9bd8268e5642cf54a5d8a0d9c8f5786fcaa3b2936e9b4f3ed630db4` | `4c9742d4c9bd8268e5642cf54a5d8a0d9c8f5786fcaa3b2936e9b4f3ed630db4` |
| `spikes/001-auth/src/windows_store.rs` | `bdcb2c373bfd0e788b415e1c59bcb2e9dc6bd02aa746176f4eb4c870939719ae` | `4bbf647b09406a14ea53e1d2e031f628ca50de44a93763dd0a4b5ce7eab15992` |

Correction artifacts: `.agent/windows-store-fix1-evidence.md`, `.agent/windows-store-fix1-summary.md`, and `.agent/windows-store-fix1-main-windows.log`. The earlier evidence/summary files received appended/inline correction notes to preserve their history and correct stale conclusions.

## Windows runtime commands still pending

On the Windows 11 checkout, run from the repository root:

```powershell
cargo test --manifest-path .agent/windows-store-check/Cargo.toml --all-targets -- --test-threads=1
cargo test --manifest-path .agent/windows-lock-check/Cargo.toml --all-targets -- --test-threads=1
```

These exercise the native tests in the real store/lock source harnesses. They use only generated synthetic test credentials and isolated test targets. Main-crate Windows compilation still has the separate browser/process/signal and Unix fixture blockers listed above.
