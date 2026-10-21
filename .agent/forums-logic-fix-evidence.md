# Forum logic fix batch 1/2 evidence

Scope was limited to the three requested corrections. Existing forum and session-detail work and the git index were left in place; no credentials, live endpoints, or browser were used.

## 1. Pagination convention consistency

- Added an actual `list_inner` fixture sequence with page 1 index `1`, then page 2 index `1` carrying a different thread ID and total `2`.
- RED before the implementation change: `cargo test --lib pagination_rejects_convention_switch_and_accepts_zero_based -- --nocapture` failed at `src/forums.rs:775`: left `complete`, expected `partial`.
- GREEN: the same command passed after the convention was inferred from page 1 and held fixed. The test asserts that the response is partial, only page 1's `thread-A` remains, `pages_fetched == 1`, and the error is `pagination_mismatch` on page 2.
- The same test also verifies zero-based `0` then `1` completes, alongside the existing one-based `1` then `2` fixture.

## 2. SHOW credential failures

- Updated both-binary CLI integration coverage for absent credentials and an unreadable credential path (credential path intentionally made a directory). Both require exit 1, the original `not_logged_in` / `storage_error` codes, `data: null`, `coverage.scope == "selected_thread"`, incomplete coverage, and a current UTC timestamp.
- Added missing and unsafe SHOW argument cases before credentials are consulted; both require usage exit 2 and empty stdout.
- RED before the CLI change: `cargo test --test cli forums_cli_help_missing_and_unsafe_ids_precede_credentials_for_both_binaries -- --nocapture` failed because SHOW emitted an empty array instead of null.
- GREEN: that command passed after making only the forum credential failure envelope action-aware. The test also asserts LIST still emits `data: []` with `selected_session` coverage and its existing credential code.

## 3. Shared operation deadline

- The operation deadline now uses production `OP` and the test target's `total` budget, then passes that single deadline through each page request. Production per-request and total timeout constants remain unchanged.
- Reworked `operation_deadline_spans_pages_and_retains_page_one` to use a 1.8-second operation budget, a 3-second per-request cap, and 0.8-second / 1.2-second response delays. Each delay fits a fresh operation budget; their combined delay exceeds the shared budget.
- RED with the original production-only deadline: the test failed with left `complete`, expected `partial` after about 2.07 seconds.
- GREEN: the test passed with partial status, page 1 retained, one page fetched, timeout error, and incomplete coverage. The fixture accept loop has a 5-second bound; response writes after client timeout are ignored.

## Checks

- Targeted regression tests passed after fixes: pagination convention test, shared operation deadline test, and forum CLI integration test.
- `cargo test --all-targets -- --test-threads=1`: passed (83 library tests, 11 CLI integration tests, and all example tests).
- `cargo fmt --check`: passed (after `cargo fmt` applied formatting).
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo build --release`: passed.
- `git diff --check`: passed.

## Explicitly left for test-only batch 2

This batch does not claim coverage for LIST shared serializer behavior, exact LIST headers/body/differing IDs, a listening honeypot with zero observers, cumulative overflow over multiple sub-cap chunks, or parameterized 401/403/500, closed-port, and existing invalid-JSON assertions.
