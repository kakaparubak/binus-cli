Implemented `binus courses sessions show <session-id>` with separate primary and additional resource reads, sanitized metadata output, partial-result handling, and safe identifier validation. The existing `sessions list` output is unchanged.

All 68 tests passed. `cargo fmt --check`, strict clippy, debug and release builds, and both binaries’ SHOW help commands passed. Evidence and summary are in [session-show-evidence.md](/home/abdel/Documents/Coding/binus-cli/.agent/session-show-evidence.md) and [session-show-summary.md](/home/abdel/Documents/Coding/binus-cli/.agent/session-show-summary.md).

Live verification remains for the parent to run outside this sandbox; I made no live API or credential requests.