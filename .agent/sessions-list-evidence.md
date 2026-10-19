# Sessions list implementation evidence

Date: 2026-10-19

## Scope and verified source contract

Implemented only `binus courses sessions list --class <class-id>`, retaining the existing Courses period/list behavior. The verified endpoint is `GET https://apim-bm7-prod-web.azure-api.net/func-bm7-course-prod/ClassSession/Class/{classId}/Student`. The payload parser requires a named root object with `sessions` as an array; a lecturers array does not substitute for `sessions`. Empty `sessions` is a complete empty result.

Each session exposes `id`, `classSessionNumber`, `courseTopicId`, `dateEnd`, `hasForumContent`, and integer `status`. Output preserves number strings and source order; exact duplicate IDs are deduplicated, and conflicting duplicates or any malformed row fail the class atomically. The `0001-01-01` date sentinel becomes `date_end: null`, while `source_date_end` preserves the source string. No title, topic text, lecturer identity, raw metadata, student identifier, or nested resource is emitted or fetched. Status remains `source_status` without interpretation.

Class identifiers use the existing documented `courses::safe_segment` policy (ASCII alphanumeric, hyphen, underscore, and dot; no empty value, slash/query, `.` or `..`). This accepts GUIDs and period-containing identifiers while path-segment encoding remains in place.

## Transport and output

The production request uses the existing saved-credential request builder for bearer and role headers, has no query/body/cookie flow, disables redirects, has a 30-second overall client timeout, 20-second request/body-read deadlines, and an 8 MiB streamed response cap. Test origin and deadline/cap overrides exist only in `#[cfg(test)]` code and validate loopback hosts. HTTP 401/403 report `reauth_required`; other non-200 statuses report `upstream_error` with status. Error strings contain no response body or credential values.

Success and failure use schema version `1`, `status`, `data`, `coverage` with explicit class scope, `errors`, `warnings`, and actual UTC `fetched_at`. Complete results exit 0; errors exit 1; malformed/missing/unsafe CLI usage exits 2 before credential loading. No partial status is synthesized for a one-class atomic read.

## Files changed

- `spikes/001-auth/src/sessions.rs`: service, safe response mapping, bounded HTTP transport, and pipeline fixtures.
- `spikes/001-auth/src/cli.rs`: nested command, pre-credential validation, missing-credential envelope and exit selection.
- `spikes/001-auth/src/lib.rs`: sessions module export.
- `spikes/001-auth/tests/cli.rs`: usage-before-credentials and missing-credentials coverage.
- `spikes/001-auth/src/courses.rs`: stabilized only the shared-budget timing fixture: 350 ms + 350 ms + 500 ms header delays under a 1 s shared budget. The first two requests have clear scheduling margin, and the last still crosses the shared deadline while remaining below the per-request allowance.

## Validation run

Commands run from the workspace root:

```text
cargo fmt --manifest-path spikes/001-auth/Cargo.toml -- --check
cargo test --manifest-path spikes/001-auth/Cargo.toml
cargo test --manifest-path spikes/001-auth/Cargo.toml courses::tests::shared_absolute_budget_includes_headers_discovery_and_all_components  # three consecutive passes
cargo clippy --manifest-path spikes/001-auth/Cargo.toml --all-targets -- -D warnings
cargo build --manifest-path spikes/001-auth/Cargo.toml
cargo build --manifest-path spikes/001-auth/Cargo.toml --release
cargo run --quiet --manifest-path spikes/001-auth/Cargo.toml --bin binus -- --help
cargo run --quiet --manifest-path spikes/001-auth/Cargo.toml --bin binus -- courses --help
cargo run --quiet --manifest-path spikes/001-auth/Cargo.toml --bin binus -- courses sessions list --help
```

The last complete test run passed 44 library tests and 9 CLI integration tests. Clippy passed with warnings denied. Debug and release builds and all three help commands passed. The shared-budget regression test passed three consecutive runs. Sessions pipeline tests exercise real reqwest calls over loopback with synthetic credentials: path, headers, no cookie/query/body, empty complete payload, redirect suppression, 401/403/upstream classification, invalid JSON, body cap, timeout, schema shape, date sentinel, source-order/duplicate behavior, and malformed/conflicting rows.

## Remaining verification

No live BINUS call was made in the sandbox, and no credentials were inspected or used here. Parent-host release verification is still needed: run accepted `courses list --period 2610`, choose the same independently selected first LEC/LAB class IDs, then invoke `courses sessions list --class ...` for those IDs and compare the returned counts/deduplication and class coverage with the already observed schema-helper counts. The previous verified schema reads and counts are recorded in `.agent/sessions-schema-evidence.md`.
