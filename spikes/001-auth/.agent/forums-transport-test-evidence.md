# Forum transport test correction — evidence

## Checkpoint

Command: `cargo test --lib forums::tests::list_pipeline_follows_pages_and_uses_verified_route_body_and_headers -- --exact --test-threads=1`

Result: passed (1 test; 82 filtered out).

The request parser now normalizes header names only and preserves value case. The focused actual LIST pipeline verifies exact parsed JSON body, route and method, synthetic Bearer token, all five role header values, and absence of Cookie. Its fixture uses distinct `id` and `threadId` values and checks that the public row retains `threadId`.

Remaining fixture work and serial full-check results will be appended after they are run.
