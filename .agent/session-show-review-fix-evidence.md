# SHOW independent review correction evidence

Scope: bounded metadata-only correction in `spikes/001-auth/src/session_details.rs`. No live API, browser, credential, storage, commit, or push actions were performed.

## Source changes

- `map_primary` now requires and maps `ssrComponent` into `component`; `classDeliveryMode` remains a separately validated source field. The fixture deliberately has `ssrComponent: LAB` and `classDeliveryMode: HYBRID`.
- `map_resources` validates optional non-null source facts before copying: integer fields (`contentLength`, `progressStatus`, `progressStamp`, `timesAccessed`) and string fields (`resourceStatus`, `lastUpdatedDate`). A malformed value returns `invalid_schema` and is never copied to output.
- Duplicate IDs compare original JSON rows. Exact copies dedupe; differences in hidden fields conflict. Primary `totalResource` is checked against raw rows before deduplication.
- Removed the dead accumulated error vector; errors continue to be represented as sanitized source-level error codes by the service.

## Added acceptance tests

- `verified_shapes_map_to_separate_safe_arrays`: component mapping and output redaction.
- `exact_source_duplicates_dedupe_but_hidden_field_conflicts_fail`: exact deduplication and hidden URL conflict.
- `malformed_progress_fact_fails_closed`: nested progress object rejected.
- `partial_pipeline_retains_whichever_source_succeeded_and_redacts_bad_rows`: service -> serializer path, primary success/additional failure and malformed primary with additional retained; secret-like strings absent from serialized result.
- `empty_additional_and_zero_primary_resources_are_complete`: empty additional success and valid zero-resource primary.
- Existing SHOW service tests cover route paths, bearer header/no cookie, complete serialization, upstream failure, and total failure.

## Verification

- `cargo test --all-targets` — passed: 58 library, 10 CLI, 4 example tests.
- `cargo fmt --check` — passed.
- `cargo clippy --all-targets -- -D warnings` — passed.
- `cargo build` and `cargo build --release` — passed.
- Both binaries' `courses sessions --help` — passed.

## Remaining acceptance coverage

This pass does not complete the entire requested matrix. SHOW-specific loopback coverage is still absent for both 401 and 403, invalid JSON, connection failure, redirect to a second listener with explicit no-hit observation, declared-length and streamed body cap overflow, phased header/body deadlines and the cumulative two-source budget, wrong root/row shape permutations, date and raw-progress semantics, role-header/body assertions, and a strict unexpected-request observer. Existing sibling `sessions`/`courses` tests cover some analogous mechanisms but are not counted as SHOW acceptance tests. Host-side post-fix LAB/LEC live verification was not performed in this sandbox; prior reported live observations are not fresh verification of this correction.
