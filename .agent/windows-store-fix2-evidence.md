# Windows store fixture cleanup correction — evidence

## Scope and starting state

- Task: final focused correction for test-build cleanup ownership in `spikes/001-auth/src/windows_store.rs`.
- Windows Credential Manager writes, real API calls, network/browser activity, reboot, commits, and system installation were not performed.
- Windows 11 native execution is unavailable in this session; native effect tests remain **PENDING**.
- Starting SHA-256 `windows_store.rs`: `4bbf647b09406a14ea53e1d2e031f628ca50de44a93763dd0a4b5ce7eab15992`.
- Starting SHA-256 `lib.rs`: `47d56239a1a11ef0efeed7734268517a659f357d01f08d42e5fdc206aee79a91`.
- Existing unrelated modified/untracked files were preserved.

## Initial review finding

`WinApi::write` unconditionally recorded every successful test-build target in `created`, including fixed production targets when `namespace` was `None`. `Drop` unconditionally deleted every recorded target. The production route in test builds therefore could delete a production credential after the temporary adapter dropped. The correction is limited to fixture ownership tracking and cleanup.

## Regression-first

Pending: add and run a Linux-executable ownership policy regression before applying the fix.

## Fix and verification

Pending.

The focused regression was run first against the old unconditional policy and failed at the non-isolated registration assertion (exit 101). After introducing the namespace ownership helpers and connecting both the real test-build `WinApi::write` registration and `Drop` cleanup to them, the same regression passed (1 passed, 94 filtered out). It uses only synthetic strings and does not call WinCred. The Linux test covers both isolated and non-isolated registration/cleanup metadata. Native Credential Manager effect tests remain **PENDING** on Windows.

## Final checks

- `cargo test --manifest-path spikes/001-auth/Cargo.toml --all-targets -- --test-threads=1` — PASS after final helper adjustment: 95 library tests, 11 CLI integration tests, and all example tests. The focused regression is among the 95.
- `cargo fmt --manifest-path spikes/001-auth/Cargo.toml -- --check` — PASS.
- `cargo clippy --manifest-path spikes/001-auth/Cargo.toml --all-targets -- -Dwarnings` — PASS.
- `cargo build --manifest-path spikes/001-auth/Cargo.toml --release` — PASS (actual Linux release build).
- `RUSTFLAGS=-Dwarnings cargo check --manifest-path .agent/windows-store-check/Cargo.toml --target x86_64-pc-windows-msvc --all-targets --tests` — PASS. This checks the actual store source, including native test source, under the Windows target.
- Native Windows Credential Manager runtime tests — **PENDING**; Windows 11 is unavailable here. No native test result is claimed.

Only `spikes/001-auth/src/windows_store.rs` was changed by this correction. `lib.rs` was left untouched and remains at SHA-256 `47d56239a1a11ef0efeed7734268517a659f357d01f08d42e5fdc206aee79a91`; its existing working-tree changes were preserved. The store source SHA-256 is now `5db7138cc229eff382e67f924fac6a25eee1ad5e837db39223c4f76bc6fd781c` (before: `4bbf647b09406a14ea53e1d2e031f628ca50de44a93763dd0a4b5ce7eab15992`). No user credentials or API values were accessed or emitted.

No other source modules, storage behavior, lock behavior, selectors, or production target names changed.
