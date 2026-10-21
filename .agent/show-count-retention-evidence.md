# SHOW count-retention verification evidence

## Scope and preservation

Resumed the saved implementation in `spikes/001-auth`; no production logic was rewritten. Ran `cargo fmt` to format the saved test edits. The README already contained the short contract note, so it needed no further change. Existing edits were preserved.

## Checks

Commands were run from `spikes/001-auth`:

- `cargo fmt` — passed.
- `cargo fmt --check` — passed.
- `cargo clippy --all-targets -- -D warnings` — passed.
- `cargo build --release` — passed.
- `cargo test --all-targets` — first run had 69 passed and one failure in the unrelated timing-sensitive `courses::tests::absolute_deadline_covers_body_and_redirect_is_never_followed` assertion (`elapsed < 220 ms`) while four Cargo checks ran concurrently and contended for the build lock. Rerunning `cargo test --all-targets` alone passed: 70 library tests, 10 CLI integration tests, and 9 example tests (89 total; no failures).

The saved count-retention coverage includes `session_details::tests::count_disagreement_retains_valid_primary_and_reports_both_counts` and `session_details::tests::malformed_primary_row_with_count_disagreement_still_fails_atomically`; both passed in the final suite.

## Contract evidence

The README documents that otherwise-valid primary metadata/resources are retained on `totalResource` versus raw `resources` row-count disagreement; `source_count_mismatch` includes both numeric counts; primary coverage is partial; the command exits 3; and no meaning is inferred for `totalResource`. Other primary schema/row failures remain atomic.

No live API call was made. The prior original red-run transcript is unavailable in this resumed context, so no TDD red output is claimed. The parent reports having observed the prechange live LEC mismatch (declared total 6, raw rows 5) reject the source; this is historical parent observation, not a reproduced check in this run. The implementation and passing saved tests are available for parent review and bounded live recheck outside the known sandbox.
