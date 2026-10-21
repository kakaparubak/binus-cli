# SHOW request-boundary test evidence

Added tests in `spikes/001-auth/src/session_details.rs`, using the actual `show_inner` request pipeline and `crate::cli::courses_output` serialization.

- `show_inner_observes_exactly_two_gets_and_never_fetches_embedded_url`: observes the primary and additional GETs through return, asserts exactly two requests, exact paths/query behavior, all five role header values, bearer authorization, no Cookie/body, and zero requests to a local listener embedded in the primary resource URL.
- `primary_redirect_is_reported_without_following_or_forwarding_authorization`: primary returns an actual local HTTP 302 with a second local listener as Location; asserts sanitized primary `upstream_error`/302, retained successful additional resources/partial coverage, and zero destination requests.
- `refused_additional_connection_keeps_valid_primary_result`: accepts the primary connection, closes its listener, then responds successfully on the accepted stream; the subsequent additional connection is refused. Asserts `network_error` for additional and retained topic/primary resources with null additional resources and partial coverage.

Validation from `spikes/001-auth`:

- `cargo test --lib session_details::tests::` — 14 passed.
- `cargo test --all-targets` — 64 library, 10 CLI, 4 example tests passed; 78 total, 0 failed.
- `cargo fmt --check` — passed.
- `cargo clippy --all-targets -- -D warnings` — passed.

All request boundary fixtures use loopback listeners and synthetic credentials. Deadline/timing coverage is deferred to batch B.
