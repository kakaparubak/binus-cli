
## Final serial checks

Commands run from `spikes/001-auth`:

1. `cargo test --all-targets -- --test-threads=1` — passed; 84 library tests, 11 CLI integration tests, and all example test targets passed.
2. `cargo fmt --check` — passed.
3. `cargo clippy --all-targets -- -D warnings` — passed.
4. `cargo build --release` — passed.

Focused checkpoint commands also passed: the exact LIST request-capture test and `upstream_statuses_and_closed_port_are_serialized_as_errors`.
