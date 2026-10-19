Added real local HTTP orchestration tests for course discovery, deduplication, empty results, failure coverage, response classification, deadlines, redirects, and shared CLI serialization/exit codes. Unsafe periods are rejected before credential loading, and both binary names retain their missing-credentials behavior.

Verification passed: **35 library tests and 8 CLI integration tests**, formatting check, Clippy with warnings denied, build, and both binaries’ help commands. No live API request or real credential was used; post-change live verification remains with the parent.

The focused record is in [courses-slice-result.md](/home/abdel/Documents/Coding/binus-cli/.agent/courses-slice-result.md). The README documents the fixed production endpoint and test transport.