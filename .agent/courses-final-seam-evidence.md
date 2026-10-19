# Courses final seam correction evidence

Scope: `spikes/001-auth/src/courses.rs` only, plus this evidence note. No production auth/storage/guard changes, schema changes, dependencies, commits, or live API calls.

## Changes

- Replaced the `Option<(&str, Duration)>` test seam with a private `RequestTarget`. Production entrypoints pass `RequestTarget::Production`; its deadline is the existing fixed 60 seconds. Request and body timeouts remain fixed at 20 seconds.
- `RequestTarget::Loopback { origin, total }`, including both configurable fields, exists only under `#[cfg(test)]`. The test-only URL rewrite is under `#[cfg(test)]` and rejects hosts other than `127.0.0.1`, `::1`, or `localhost`. Normal release compilation has no alternate-origin URL parsing or test deadline configuration. The shared `periods_with`/`list_with` orchestration and `get_at` pipeline remain the ones exercised by tests.
- Added exact repeated class-row coverage: duplicate LAB rows collapse to one; same class ID in a distinct LEC component remains present; coverage remains complete.
- Added partial results coverage for valid LAB data with failed LEC request, and atomic malformed LAB component data (valid row followed by malformed row) with successful LEC sibling. The failed component contributes no rows while sibling data remains; discovery and component statuses are asserted.
- Added delayed-header, combined header/body over-budget, and discovery plus multiple component shared-budget tests. Local delayed fixture bounds accept waits, and handles client write closure without panicking. Existing body-stall/redirect test remains.
- Tightened assertions for invalid JSON versus invalid schema, empty captured GET request body, and current fetched timestamps.

## Checks run

From `spikes/001-auth`:

- `cargo test` — passed: 38 library unit tests, 8 CLI integration tests, 0 failures (46 total; doc/binary harnesses had no tests).
- `cargo fmt --check` — passed.
- `cargo clippy --all-targets -- -D warnings` — passed.
- `cargo build` — passed.
- `cargo build --release` — passed.

No post-change live API verification was performed. Parent host-side API review remains separate.
