Completed the three requested corrections and recorded RED/GREEN evidence in [forums-logic-fix-evidence.md](/home/abdel/Documents/Coding/binus-cli/.agent/forums-logic-fix-evidence.md) and [forums-logic-fix-summary.md](/home/abdel/Documents/Coding/binus-cli/.agent/forums-logic-fix-summary.md).

Pagination now enforces the convention inferred from page one; SHOW credential errors emit `data: null` with selected-thread coverage; and LIST pages share one operation deadline. The targeted regressions and all requested checks passed: serial all-target tests, formatting, clippy with warnings denied, release build, and `git diff --check`.

Batch 2 test gaps remain untouched and are listed in the evidence file. The index was left unchanged; no live calls or credential workarounds were used.