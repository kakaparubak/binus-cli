# SHOW transport pass evidence

Scope: focused test and fixture updates in `spikes/001-auth/src/session_details.rs` plus SHOW CLI assertions in `spikes/001-auth/tests/cli.rs`. No production transport/auth/storage/routes changes in this pass. Credentials in fixtures are synthetic.

## Added/extended tests

- `session_details::tests::actual_two_source_pipeline_is_safe_and_serializes_complete_result`: executes both GETs through `show_inner` and the shared CLI serializer; checks exact paths and primary `isWeb=true`, bearer token, all five role header values (`rOId`, `academicCareer`, `institution`, `roleName`, `roleId`), no Cookie, no token/string sentinel, and empty request body.
- `session_details::tests::authorization_failures_are_distinct_from_upstream_and_keep_valid_source`: additional-source 401, 403, and 500 with primary success; checks partial output, source/status/code, preserved primary data, and coverage.
- `session_details::tests::invalid_json_and_wrong_roots_preserve_the_other_source`: invalid JSON and invalid primary/additional roots with the counterpart valid; checks partial results, sanitized code, and coverage.
- `session_details::tests::streamed_and_declared_oversize_responses_are_partial`: declared oversized Content-Length plus chunked response without Content-Length, with valid primary retained; checks `body_too_large`.
- `tests::show_help_and_unsafe_id_precede_credentials_for_both_binary_names`: additionally checks missing session ID exits with usage status 2 before credentials are needed.
- `tests::courses_usage_precedes_credentials_and_missing_credentials_are_enveloped` and `tests::sessions_usage_precedes_credentials_and_missing_credentials_are_enveloped`: fetched timestamp assertions now check current UTC year and `Z`, rather than a pinned historical year.

Fixture helper `server` now reads through the HTTP header terminator and declared body length (with a bounded read timeout), allowing an empty-body assertion. `raw_server` supports a synthetic chunked response.

## Validation

From `spikes/001-auth`:

- `cargo test --all-targets`: passed, 61 library tests, 10 CLI integration tests, 4 example tests; no failures.
- `cargo fmt -- --check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo build`: passed.
- `cargo build --release`: passed.
- `cargo run --quiet --bin binus -- courses sessions show --help`: passed.
- `cargo run --quiet --bin binus-auth-spike -- courses sessions show --help`: passed.

The SHOW test target uses cfg(test) loopback origin and per-test body cap (1024 bytes for overflow cases); no production deadlines were changed.

## Coverage not established in this pass

The current added suite does **not** yet prove an exact two-call upper bound with a loopback honeypot/observer that waits for unexpected calls; role header names are validated through lowercased HTTP serialization; there is no redirect-to-second-listener assertion, delayed header/body or trickled-chunk deadline test, shared-total-budget regression, or closed-port partial-result test. Malformed-row handling is covered by existing SHOW tests but not extended here for every transport error row. Raw naive date/progress preservation have existing fixture/parser assertions but no additional assertions were added in this pass. Do not treat this evidence as completion of the full requested matrix.
