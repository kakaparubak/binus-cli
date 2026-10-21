# Streamed response cap test evidence

- Test: `session_details::tests::streamed_oversize_response_is_rejected_after_cumulative_bytes_cross_cap`
- Request path: invokes `show_inner(&record(), "s-1", Some(target))` and passes the returned value through `crate::cli::courses_output`; uses the existing synthetic credential and primary fixture.
- Server framing: first response is the valid primary fixture with `Content-Length`; second response omits `Content-Length` and uses HTTP/1.1 `Transfer-Encoding: chunked`, with correctly sized hexadecimal chunks and the terminal zero chunk. Its body is valid JSON (`[]` followed by whitespace) totaling 2048 bytes; it is sent in 512-byte chunks, each below the 1024-byte test cap. The listener has a bounded 4-second accept deadline; request reads are bounded by the existing 1-second timeout; expected write failure after client cap rejection is ignored.
- Assertions: exit code 3, status `partial`, additional error `body_too_large` sourced from `additional`, primary topic and one primary resource retained, additional resources null, primary coverage `complete`, additional coverage `failed`, and overall coverage `complete == false`.
- Declared-size coverage: preserved as `declared_oversize_response_is_rejected_before_reading_body`, using the existing `Content-Length` server and asserting `body_too_large` and partial coverage.

## Capability check

- Before temporary mutation, SHA-256 of the production prefix (all bytes before the first `#[cfg(test)]` in `session_details.rs`): `537d1b98d05b299c03855d6a8136a2ca47cd583e698f55871ceb5a2ef190e5da`.
- Temporary mutation: replaced only the accumulated-byte comparison `bytes.len().saturating_add(chunk.len()) > target.cap()` with `false`. No other production source was changed.
- Red command: `cargo test --manifest-path spikes/001-auth/Cargo.toml streamed_oversize_response_is_rejected_after_cumulative_bytes_cross_cap --lib`.
- Red result: failed as expected; output became complete (exit code 0), while assertion expected partial exit code 3. Cargo exit code 101.
- Restoration: immediately restored `session_details.rs` from its pre-mutation byte-for-byte copy.
- After restoration and formatting, production-prefix SHA-256: `537d1b98d05b299c03855d6a8136a2ca47cd583e698f55871ceb5a2ef190e5da` (identical).
- Green command: same focused command; passed (1 passed, 0 failed).

## Verification

- `cargo test --manifest-path spikes/001-auth/Cargo.toml --all-targets`: passed; 68 library, 10 CLI integration, and 4 example tests passed (82 total).
- `cargo fmt --manifest-path spikes/001-auth/Cargo.toml -- --check`: passed after rustfmt formatting.
- `cargo clippy --manifest-path spikes/001-auth/Cargo.toml --all-targets -- -D warnings`: passed.
- `cargo build --manifest-path spikes/001-auth/Cargo.toml --release`: passed.
