# Windows browser correction 1/2 evidence

## Scope and source hashes

Repository: `/home/abdel/Documents/Coding/binus-cli`; package: `spikes/001-auth`.
Only the three requested blockers were changed in `src/browser/native.rs`, `src/lib.rs`, and `src/cli.rs`. `src/browser.rs` was not changed. Other pre-existing working-tree changes were left untouched.

| File | Before SHA-256 | After SHA-256 |
| --- | --- | --- |
| `src/browser.rs` | `ed64afa590e9e9da657e2294ecea3bd6844467365d2940e68a4d2d7f7e863096` | `ed64afa590e9e9da657e2294ecea3bd6844467365d2940e68a4d2d7f7e863096` |
| `src/browser/native.rs` | `6b467b8c3e99fa5318d2f47c7919437027faa99e28c90840f2c2d0ba1556eac3` | `7aa7bd7de6eced63d86679c8fa9e28754a3b5661fd695dbcb1cc50e355aa7b96` |
| `src/lib.rs` | `cbb0242f4094b3901111c4685cc65a19157075679a3a6baafff54f737649bc07` | `f975d8d97e4df07957d01400b7da0326d8cd68311dc2949497068dddc5803ce1` |
| `src/cli.rs` | `25528bcb28197c7dc30997029048614207f8995db5454a2a229c35df7331b67f` | `26d70c8d1776f6b314cd18fc70f48669867e8d42cb6c3475cbb553df0e65fab8` |

## Blockers mapped to proof

1. **Launch teardown and handle ownership:** launch-assignment and `ResumeThread` failure paths now check termination confirmation and return `browser_cleanup_failed` if it is absent. Process/thread/job handles close through RAII; the private profile is retained on unconfirmed teardown. In particular, the resume failure path no longer forgets the job handle, leaving kill-on-job-close available as a fallback. No OS fault injection was available; this is source-reviewed and Windows-typechecked.
2. **Explicit finalization:** Windows constructs `Browser` before entering the timeout/cancellable operation. Its success, error, timeout, and cancellation results all pass through the same `finalize_capture` routine. A cleanup error replaces any operation result with the sanitized `browser_cleanup_failed`. Windows console cancellation is passed into the operation selector, which returns only after dropping inner work; the outer path then awaits finalization. Public `capture(Duration)` remains available. Unix SIGINT/SIGTERM and group cleanup path remains intact.
   - TDD red: temporarily making the finalizer routine skip its closure caused `capture_finalization_covers_results_timeout_and_cancellation` to fail on the expected finalizer call count (`left: 0`, `right: 4`).
   - Green: restored finalization implementation; the focused test passed. It exercises success, early error, deterministic timeout, cancellation, invocation count, and cleanup-error precedence over all four outcomes, with sanitized errors.
3. **Real Windows tree test:** the inert test helper reports root and descendant PIDs through fixture files. The test waits to a deadline, opens and retains synchronization handles for the reported root and descendant, confirms both belong to the owned job using `IsProcessInJob`, and after both explicit cleanup and Drop waits on those exact handles and asserts each profile directory was removed. An unrelated helper is asserted alive and has a bounded RAII cleanup guard. This Windows-only test is typechecked but **not run** on this Linux host.

## Verification results

- `cargo fmt --all -- --check`: passed.
- `cargo test --all-targets -- --test-threads=1`: passed, including 100 library tests, 11 CLI integration tests, and all example tests.
- Focused `cargo test capture_finalization_covers_results_timeout_and_cancellation --lib`: passed after the deliberate red run.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo build --release`: passed for the Linux host.
- `RUSTFLAGS=-Dwarnings cargo check --target x86_64-pc-windows-msvc --lib --bins --tests`: passed, including Windows-only native lifecycle tests at type-check time.
- `cargo check --target x86_64-pc-windows-msvc --all-targets`: passed on the final source with the existing `examples/credential_size.rs` Windows-cfg warnings (two unused imports and three dead-code warnings).
- `git diff --check`: passed.

The first parallel Linux all-target test run hit the existing elapsed-time assertion in `courses::tests::absolute_deadline_covers_body_and_redirect_is_never_followed`. The required serial rerun passed. No native Windows execution occurred; Windows lifecycle runtime evidence remains pending.
