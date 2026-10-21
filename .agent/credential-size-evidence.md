# Credential size helper evidence

Scope: add `spikes/001-auth/examples/credential_size.rs` and this evidence file only. The helper is a byte-count diagnostic; it performs no network access or writes. Real account credentials were not inspected or loaded during development. Synthetic records are used for tests.

## Files

- `spikes/001-auth/examples/credential_size.rs` — the sizing helper and five synthetic tests.
- `.agent/credential-size-evidence.md` — this evidence.
- `.agent/credential-size-summary.md` — final summary.

## TDD and checks

- Red command: `cargo test --manifest-path spikes/001-auth/Cargo.toml --example credential_size -- --test-threads=1` — failed with exit 101 because `size_record` was not defined yet (expected red).
- Green focused command: same command after implementation — passed, 5 tests.
- Full suite: `cargo test --manifest-path spikes/001-auth/Cargo.toml --all-targets -- --test-threads=1` — passed, including 84 library tests, 11 CLI tests, and all examples (114 tests total).
- Formatting: `cargo fmt --manifest-path spikes/001-auth/Cargo.toml --all -- --check` — passed.
- Clippy: `cargo clippy --manifest-path spikes/001-auth/Cargo.toml --all-targets -- -D warnings` — passed.
- Release example: `cargo build --manifest-path spikes/001-auth/Cargo.toml --release --example credential_size` — passed.

## Acceptance details

- The helper measures UTF-8 byte lengths (`str::len`) and the actual `serde_json::to_vec(&record)` encoding. The emitted success object has exactly `access_token_bytes`, `refresh_token_bytes`, `record_json_bytes`, `max_blob_bytes`, and `record_fits`.
- Synthetic tests check that no synthetic account, token, or role value appears in the report; construct records measured to exactly 2,560 and 2,561 serialized bytes; and check absent and malformed records return identical fixed JSON with failure exit status and no path or error detail.
- A synthetic valid record is written in a temporary directory and read only through public `binus_auth_spike::load_record`. The production diagnostic also calls only that loader. There is no raw credentials-file read path.

## Parent invocation

After code review, from Omarchy, run `cargo run --manifest-path spikes/001-auth/Cargo.toml --release --example credential_size`. This uses the existing config path selected from `XDG_CONFIG_HOME` or `HOME/.config`, under `binus-auth-spike/credentials.json`, and prints byte counts and fit status only. No real invocation was performed here. Windows native runtime remains unavailable and is not claimed; the non-Unix helper path returns a fixed sanitized failure.
