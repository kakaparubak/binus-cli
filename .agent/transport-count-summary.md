Added the fixed-key `source_counts` projection to successful primary diagnostic rows and a synthetic test confirming it reports a mismatch without exposing nested response values. The count guard is unchanged.

Updated the transport evidence and added [transport-count-summary.md](/home/abdel/Documents/Coding/binus-cli/.agent/transport-count-summary.md) with the parent-provided observations.

All requested checks passed: targeted test, formatting, clippy, all targets (68 library, 10 CLI, 4 schema-example, 5 transport-helper tests), and debug/release helper builds. No production source was changed.