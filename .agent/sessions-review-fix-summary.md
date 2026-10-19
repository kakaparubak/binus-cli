Implemented the two fixes and the requested regression coverage. Session requests now share one absolute 20-second deadline across headers and body reads; recognized continuation indicators prevent false `complete` results and return sanitized errors.

Verification passed: formatting, all-target tests (50 library, 9 CLI, 2 example), clippy with warnings denied, and debug and release builds. No live API/browser verification, commit, or push was performed.

Reports: [detailed evidence](/home/abdel/Documents/Coding/binus-cli/.agent/sessions-review-fix-evidence.md) · [summary](/home/abdel/Documents/Coding/binus-cli/.agent/sessions-review-fix-summary.md)