# Session SHOW implementation evidence

Implementation: `spikes/001-auth/src/session_details.rs`, CLI wiring in `spikes/001-auth/src/cli.rs`, module export in `spikes/001-auth/src/lib.rs`, and CLI entrypoint checks in `spikes/001-auth/tests/cli.rs`.

## Scope implemented

- `binus courses sessions show <session-id>` accepts a safe positional identifier. Clap help and unsafe identifiers resolve before credential loading; missing credentials produce a schema-version-1 error envelope.
- The service uses the existing academic-period request builder for role headers and bearer authorization. It issues only GETs to the fixed API route, with `isWeb=true` on primary only, disables redirects, and never follows resource URLs.
- The two reads have separate source status/error reporting and share one 60-second budget. Each request is limited to 20 seconds and streamed response bodies to 8 MiB.
- The mapper emits topic/subtopic/outcome text and selected course/session metadata, two named material arrays, stable IDs, `has_link`, and selected raw source facts. It excludes URL/token/redirect/join/lecturer/attendance data. Dates keep raw values and do not infer a timezone; year-0001 sentinel dates normalize to null.
- Exact duplicate resource rows deduplicate within a source; conflicting duplicates, wrong session context, malformed roots/rows, unsupported continuation indicators, and primary resource count mismatches fail that endpoint atomically.
- A failed source retains the successful source's data and marks unknown primary fields null; both failed sources produce `data: null`. Successful empty additional-resource arrays remain valid.

## Local verification

Commands run from `spikes/001-auth`:

```text
cargo test --all-targets
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build
cargo build --release
./target/debug/binus courses sessions show --help
./target/debug/binus-auth-spike courses sessions show --help
```

Final test totals: 54 library tests, 10 CLI integration tests, and 4 schema example tests passed (68 total). Formatter check and clippy passed. Debug and release builds passed. Both help commands exited 0 and showed the positional `<SESSION_ID>`.

New test coverage includes verified-shape mapping and secret-field omission, sentinel handling, separate source arrays, nullable `resourceId` distinct from `id`, unknown resource type preservation, count/pagination/context/conflicting-duplicate rejection, actual two-request loopback pipeline and captured paths/headers, partial and dual failure serialization/status codes, and both binary names' help/unsafe/missing-credentials behavior.

No live API, browser, or credential was used inside Codex. Production release verification against the two parent-selected sessions remains pending outside this sandbox. Local tests do not establish universal upstream semantics for source progress fields.
