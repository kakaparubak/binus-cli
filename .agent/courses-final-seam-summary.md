Completed the scoped correction in [courses.rs](/home/abdel/Documents/Coding/binus-cli/spikes/001-auth/src/courses.rs). The loopback origin and configurable test deadline are test-only; release builds retain fixed production URLs and timeouts.

Added the requested duplicate-row, partial-data, atomic-schema-failure, and shared-deadline regressions. All checks passed: 38 library tests, 8 CLI tests, formatting, clippy, debug build, and release build. No live API verification was performed.

Detailed evidence is in [courses-final-seam-evidence.md](/home/abdel/Documents/Coding/binus-cli/.agent/courses-final-seam-evidence.md).