Completed the focused mutex correction and stopped after the requested checks.

Production mutex names now include the canonical SID from the same effective token used for the owner and DACL. The Linux naming regression passed after first failing because the helper was absent. Native tests now use a separate owner thread for contention and check later acquisition after guard drop and abandonment, while an independent handle preserves mutex identity.

The requested Linux test suite, formatting, Clippy, release check, Windows all-target check, and a fresh Windows `--tests` type check passed. **Windows runtime tests, cross-session behavior, and native cleanup effects remain pending**; they were not run here.

Details: [evidence](/home/abdel/Documents/Coding/binus-cli/.agent/windows-lock-fix1-evidence.md) · [summary](/home/abdel/Documents/Coding/binus-cli/.agent/windows-lock-fix1-summary.md)