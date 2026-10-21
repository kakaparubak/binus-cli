Implemented the forum thread list and detail slice with the requested commands:

```sh
binus forums threads list --class ID --session ID
binus forums threads show THREAD_ID --class ID
```

Listing follows the declared total, deduplicates by `threadId`, and preserves validated rows when a later page fails. Detail output validates the requested thread and class, converts HTML to visible text, redacts credential-bearing URLs, and exposes attachment metadata without links. HTML parsing uses html5ever’s [`parse_document`](https://docs.rs/html5ever/latest/html5ever/driver/fn.parse_document.html) API.

**Checks:** formatting, Clippy with warnings denied, and release build passed. The full suite passed serially: 82 library tests, 11 integration tests, and the example targets. One default parallel test run hit the existing Courses 220 ms timing assertion; it passed in isolation and in the serial full run. No production timing limits were changed.

This was fixture-only validation; no live forum request was made. The evidence report records test coverage and remaining gaps: [.agent/forums-threads-evidence.md](/home/abdel/Documents/Coding/binus-cli/.agent/forums-threads-evidence.md). The implementation summary is [.agent/forums-threads-summary.md](/home/abdel/Documents/Coding/binus-cli/.agent/forums-threads-summary.md).