Implemented the narrow guard correction in [src/lib.rs](/home/abdel/Documents/Coding/binus-cli/spikes/001-auth/src/lib.rs:890) and documented the exceptions and evidence in [README.md](/home/abdel/Documents/Coding/binus-cli/spikes/001-auth/README.md:34).

- Allows only HTTPS default-port GET/HEAD to `login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize`. Regression cases reject lookalike hosts, other methods, paths, schemes, and ports.
- Allows the exact `IsActiveLinkAccountExists` POST. Reset, token-link, OTP, and other tested routes remain blocked; no reset workflow was allowed or replayed.
- Added a synthetic paused-request test confirming the Microsoft redirect continues in its attached session while the unsafe POST remains blocked.
- No OAuth URL parameters, capture expression, interception patterns, or live browser state were changed or inspected. No live authentication success is claimed.

Validation passed: `cargo test` (25 unit, 6 integration), `cargo clippy --all-targets -- -D warnings`, `cargo build`, `cargo fmt`, and `git diff --check`. No commits or pushes.