# SHOW deadline regression evidence

## Scope

Added three `#[tokio::test]` cases in `spikes/001-auth/src/session_details.rs`, using `show_inner`, the real `get`/`get_with_target` reqwest path, and `crate::cli::courses_output`. All requests use synthetic credentials and a loopback TCP listener. No production logic or production timeout configuration was changed.

The delayed listener bounds each accept wait to 4 seconds and socket request reads to 2 seconds. It returns cleanly if a client does not connect and ignores expected disconnects during a delayed response. Tests assert CLI exit code 3, `status: partial`, `coverage.complete: false`, and the specific timeout source/code, plus retained data from the successful source.

## New cases and regression capability

1. `request_deadline_covers_headers_and_body_and_keeps_additional_result`
   - Request cap 1 s; total cap 4 s.
   - Primary response headers arrive after 400 ms and valid fixture body 700 ms later (about 1.1 s combined; each phase is below the request cap). Valid nonempty additional response follows immediately.
   - Asserts primary `timeout`, null primary fields, and one retained additional resource with complete additional coverage.
   - A regression that starts a fresh body timeout after headers allows the valid primary body to complete, so the asserted primary timeout fails.

2. `request_deadline_covers_trickled_body_and_keeps_additional_result`
   - Request cap 1 s; total cap 4 s.
   - Valid primary fixture JSON is delivered in four TCP chunks, each delayed 350 ms (about 1.4 s overall). A valid additional response follows after the primary writer finishes.
   - Asserts primary `timeout`, null primary fields, and one retained additional resource with complete additional coverage.
   - A regression that refreshes the request timeout for each body chunk allows the body to complete, so the asserted primary timeout fails.

3. `shared_total_deadline_covers_both_sources_and_retains_primary`
   - Request cap 3 s; total cap 2.2 s.
   - Valid primary fixture body is delayed 900 ms; valid additional body is delayed 1.7 s. Both fit an independent 3 s request cap; together they take about 2.6 s.
   - Measures `show_inner` elapsed time before joining the fixture thread, asserting a broad 1.9–4 s bound. Asserts primary complete/data retained and additional `timeout`/null with partial exit code 3.
   - A regression that restarts the shared operation budget for the additional source permits its 1.7 s response and makes the asserted additional timeout fail.

The response bodies are existing valid fixture data (`fixture()` and `additional_body()`); classification assertions ensure schema or network errors cannot satisfy the cases. No `Response` mock, fake clock, ignored test, or live API was used.

## Verification

The three new tests passed individually before repetition, then each passed in three consecutive repetitions (six targeted command invocations; three tests per repetition). No timing assertion uses a narrow millisecond threshold.

Commands run from `spikes/001-auth`:

- `cargo test --lib request_deadline` — 3 passed (also matches the existing sessions deadline test)
- `cargo test --lib shared_total_deadline_covers_both_sources_and_retains_primary` — 1 passed
- Three repetitions of `cargo test --lib session_details::tests::request_deadline_covers` and `cargo test --lib session_details::tests::shared_total_deadline_covers_both_sources_and_retains_primary` — all passed
- `cargo test --all-targets` — 67 library tests, 10 CLI integration tests, and 4 example tests passed
- `cargo fmt --all -- --check` — passed
- `cargo clippy --all-targets -- -D warnings` — passed
- `cargo build` — passed
- `cargo build --release` — passed

The regression capability above follows from the scheduled server timings and asserted outcomes; no temporary production mutation was used to manufacture a red run. This batch makes no live-service claim.
