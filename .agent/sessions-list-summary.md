Implemented `binus courses sessions list --class <class-id>` with strict `sessions[]` validation, class-scoped JSON coverage, deduplication, and bounded GET transport. Added service and CLI tests, and stabilized the reproduced shared-budget timing test without changing production timeouts.

Validation passed: 44 library tests, 9 CLI tests, formatting, Clippy with warnings denied, debug and release builds, and CLI help. The timing regression test also passed three consecutive runs.

Detailed contract and test evidence: [sessions-list-evidence.md](/home/abdel/Documents/Coding/binus-cli/.agent/sessions-list-evidence.md). Summary: [sessions-list-summary.md](/home/abdel/Documents/Coding/binus-cli/.agent/sessions-list-summary.md).

Live release verification against the selected LEC and LAB classes remains for the parent host; I made no live request, commit, or push.