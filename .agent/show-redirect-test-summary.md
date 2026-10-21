**Batch A is complete.** The three requested tests pass through the actual `show_inner` pipeline. No production code changed.

Validation passed: `cargo test --all-targets` (78 tests), `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`.

Evidence: [show-redirect-test-evidence.md](/home/abdel/Documents/Coding/binus-cli/.agent/show-redirect-test-evidence.md)  
Summary: [show-redirect-test-summary.md](/home/abdel/Documents/Coding/binus-cli/.agent/show-redirect-test-summary.md)

Deadline/timing coverage remains for batch B; this does not claim overall SHOW acceptance.