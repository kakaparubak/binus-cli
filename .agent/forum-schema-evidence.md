# Forum schema helper evidence

## Scope

Updated only `spikes/001-auth/examples/forum_schema.rs` and this evidence. No production CLI sources, credentials, browser, or live API were touched in this correction. The saved live evidence remains in `.agent/forum-schema-live-evidence.json`; this edit does not reinterpret its flat nested field union as root schema.

## Reporting changes

The helper preserves its prior output and adds direct root field types for detail, direct `datas` row field type unions for list pages, and path-qualified detail field types such as `$.attachments[].name`. Nested array counts are also path-qualified. Traversal remains depth bounded, arrays sample at most 1,000 entries, and structural names remain ASCII alphanumeric/underscore/hyphen up to 64 bytes.

Detail facts are booleans only: `id` equals the selected `threadId`, `forumId` equals requested class ID, `threadContentText` is string/nonempty/probably HTML, and whether a URL query contains `sig`, `token`, `access_token`, `refresh_token`, or `signature`. URL strings are examined in memory only. Across fetched list pages, the report includes collected row count, unique valid `threadId` count, duplicate count, conflict boolean, and whether numeric `totalData` equals the unique count. No identifiers, content, URLs, or query values are emitted.

Request behavior remains fixed origin and endpoint, max two list POSTs and first valid thread detail GET, no redirects, 20-second request timeout, 60-second overall limit, and 8 MiB response cap.

## Checks

- `cargo test --manifest-path spikes/001-auth/Cargo.toml --example forum_schema` — passed, 5 tests.
- `cargo fmt --manifest-path spikes/001-auth/Cargo.toml -- --check` — passed.
- `cargo clippy --manifest-path spikes/001-auth/Cargo.toml --all-targets -- -D warnings` — passed.
- `cargo build --manifest-path spikes/001-auth/Cargo.toml --example forum_schema` — passed.

Synthetic coverage verifies nested/root separation, path reporting, no content or URL leakage, context comparisons, HTML flagging, signed query detection, and selecting `threadId` rather than row `id`.
