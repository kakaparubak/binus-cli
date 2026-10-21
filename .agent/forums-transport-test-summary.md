Completed the forum transport test-only correction. The request parser now normalizes header names while preserving value case, and the added status, closed-port, malformed-JSON, response-cap, and bounded fixture checks pass.

Serial checks passed from `spikes/001-auth`: all targets (84 library tests, 11 CLI tests, and example targets), formatting, Clippy with warnings denied, and release build. No live API request was made.

Evidence: [detailed results](/home/abdel/Documents/Coding/binus-cli/.agent/forums-transport-test-evidence.md), [summary](/home/abdel/Documents/Coding/binus-cli/.agent/forums-transport-test-summary.md), and [updated forum evidence](/home/abdel/Documents/Coding/binus-cli/.agent/forums-threads-evidence.md).