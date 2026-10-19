# Session schema helper evidence

## Scope and implementation

Added the isolated example `spikes/001-auth/examples/session_schema.rs`. It accepts exactly `--class ID`; malformed input and path-like IDs are rejected before credential loading. IDs are limited to 128 ASCII alphanumeric, hyphen, or underscore characters. `--help` returns before credential access or network setup.

For a valid ID, the helper obtains the existing preferred protected credential record in-process, calls `academic_period_request` to retain its bearer token and all five role headers, rewrites only the URL to the fixed production route `https://apim-bm7-prod-web.azure-api.net/func-bm7-course-prod/ClassSession/Class/{classId}/Student`, and sends a GET. Redirects are disabled; the client has a 20-second request timeout and an overall 30-second request/body deadline. Response bodies are capped at 8 MiB while reading. There is no cookie jar customization, refresh, login, write request, browser navigation, or alternate host input.

Success output is JSON containing HTTP status, root shape/counts, union field/type metadata, observed nested-array paths/counts, and field names/types that resemble totals or continuation metadata. It does not emit response scalar values, IDs, names, topics, or body contents. Errors are sanitized and do not include response bodies or URLs.

The schema walker aggregates types across all rows and nested array elements. Synthetic tests cover ID injection rejection and ensure representative scalar values are absent from the generated report.

## Build and checks

- `cargo fmt --all` — passed.
- `cargo build --example session_schema` — passed.
- `cargo test --example session_schema` — 2 passed.
- `cargo clippy --all-targets -- -D warnings` — passed.
- `cargo test` — 37 passed, 1 failed. The pre-existing `courses::tests::shared_absolute_budget_includes_headers_discovery_and_all_components` failed at `src/courses.rs:1017`: observed status `error`, expected `partial`. It failed on two consecutive full-suite runs. No Courses code or tests were changed in this slice.

No live endpoint call, credential read, or live verification was performed in this environment.

## Parent-run command

From `/home/abdel/Documents/Coding/binus-cli/spikes/001-auth`, run the already-built helper outside the sandbox, using only a class ID selected from the accepted Courses CLI output held in memory:

```sh
cargo run --release --example session_schema -- --class CLASS_ID
```

Replace `CLASS_ID` with one explicitly selected accepted class ID. Run once for a LEC class and once for a LAB class if available; do not enumerate/fetch all classes. Each invocation makes one GET to the fixed route above, with no redirects, a 20-second request timeout, 30-second overall deadline, and 8 MiB response cap. The helper prints structural JSON only. Do not paste raw response bodies or credentials into the workspace.

This is a schema diagnostic prerequisite only; it does not implement a production sessions command.
