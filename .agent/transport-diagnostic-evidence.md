# Transport diagnostic helper evidence

Added `spikes/001-auth/examples/transport_diagnostic.rs`. The helper reuses `load_preferred_record` and `academic_period_request`; it does not change production sources or credential handling. Host-side bounded observations were run by the parent; this helper work did not inspect credential contents or make additional live requests.

## Host invocation

Run from `/home/abdel/Documents/Coding/binus-cli/spikes/001-auth` on the trusted parent host, where the existing credential library can access credentials:

```sh
cargo run --example transport_diagnostic -- --class '<CLASS_ID>' --construction inherited
cargo run --release --example transport_diagnostic -- --class '<CLASS_ID>' --construction inherited
```

Replace the placeholder locally. Do not save or relay class/session IDs. Run the same selected class and construction in both profiles. For the optional bounded details sequence, append `--details`; the helper then takes the first valid `sessions[].id` from a successful class response and independently attempts primary and additional GETs.

Usage: `transport_diagnostic --class ID [--construction inherited|reset] [--details]`. Construction defaults to `inherited`. `--help` performs no credential or network access. Class and discovered session IDs use the existing safe-segment rule. Origin is fixed to `https://apim-bm7-prod-web.azure-api.net`; redirects are disabled. Only the known class, primary, and additional GET paths are constructed. No embedded resource URL is requested.

There is a 60-second shared budget and each request is bounded to 20 seconds, covering headers and streamed body. Bodies are capped at 8 MiB. Non-2xx bodies are not read. 401/403 are reported as `auth_error`; they do not trigger login or refresh.

## Output contract

The helper emits one JSON object. Top-level keys are `status`, `construction`, `request_comparisons_equal`, `requests`, `details`, and `total_elapsed_ms`. `request_comparisons_equal` has `class`, `primary`, and `additional` entries, each containing only the booleans `url_equal`, `method_equal`, and `headers_equal`.

Each request row uses static stage/status labels and elapsed milliseconds. It reports `http_status` only when received, `body_bytes` only as a count, and for reqwest transport errors only the flags `is_timeout`, `is_connect`, `is_request`, `is_body`, `is_decode`, `is_builder`, `is_redirect`, `is_status`, a fixed `io_error_kind` if found, and bounded `error_chain_depth` (maximum 8). A successful primary response adds `source_counts` with exactly `declared_total_resource` (u64 or null), `raw_resource_rows` (u64 or null), and `count_matches` (boolean or null). This is a diagnostic projection only; it does not relax a count guard. No raw error text, URL, identifier, header, response body, credential, or arbitrary response value is serialized. `details` reports `not_requested`, `skipped_class_failure`, `skipped_no_valid_session_id`, or `attempted`.

Exit codes: 0 complete (including help), 3 partial, 1 failure, 2 usage/invalid class. Missing credentials are reported only as `credentials_unavailable`.

## Local verification

From `spikes/001-auth`:

- `cargo test --example transport_diagnostic primary_count_projection_is_numeric_and_excludes_response_content` — passed.
- `cargo test --all-targets` — passed: 68 library tests, 10 CLI integration tests, 4 existing schema-example tests, and 5 transport-helper tests.
- `cargo fmt -- --check` — passed.
- `cargo clippy --all-targets -- -D warnings` — passed.
- `cargo build --example transport_diagnostic` — passed.
- `cargo build --release --example transport_diagnostic` — passed.

Synthetic tests cover argument rejection/defaults, equality of exact URL/method/header request fields under both constructions, secret-free comparison serialization, rejection of an unsafe discovered ID, reqwest error projection without serializing raw error text, and numeric primary count projection without serializing synthetic nested values. These checks establish helper behavior only; they do not identify the cause of the live LEC transport failures.
