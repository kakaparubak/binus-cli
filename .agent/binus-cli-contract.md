---
tags: [binus, cli, rust, api-design]
status: draft
source: "Proposed command/output contract following approved retrieval decisions"
related_to: ["[[binus-cli]]"]
---

# BINUS CLI contract proposal

This is a DESIGN DRAFT for Abdel to review, not an implemented CLI or approved implementation plan. All sample IDs/data below are illustrative, not new API results. Confirmed decisions remain in [[binus-cli]].

## Agreed constraints

Rust; read-only Courses, Forum, Assessment, Schedule. Agent-facing one-shot commands; no TUI. List summaries, show selected details, download files explicitly. Return successful data with explicit failed-course identification on partial retrieval. Show normalized submission state plus original evidence/conflicts, prioritizing positive submitted flags at their actual scope. Assessment list does not fan out to question/opening reads or claim verified submission.

## Proposed commands

```text
binus auth login
binus auth status
binus auth logout

binus courses list
binus courses sessions list --class <class-id>
binus courses sessions show <session-ref>

binus schedule list --from YYYY-MM-DD --to YYYY-MM-DD

binus assessments list [--class <class-id>]
binus assessments show <assessment-ref>

binus forums threads list --class <class-id> [--session <session-id>]
binus forums threads show <thread-ref>
binus forums replies list --thread <thread-ref>

binus attachments download <attachment-ref> --output <path>
```

- `auth login` is the user-interactive exception to ordinary noninteractive data commands. Browser-assisted bootstrap is the candidate, not a settled technical implementation.
- `auth logout` means remove this CLI's local credentials only, not revoke Microsoft/BINUS sessions or log out the user's regular browser.
- A class is a specific LEC/LAB enrollment, not merely a course code/title. `courses list` exposes that distinction. Use `--class` to avoid pretending a course code identifies one class.
- List output returns stable opaque refs usable verbatim in show/download commands. It also retains relevant upstream IDs separately. Refs must carry or resolve class/session/content context without depending on list ordering or secret URLs. Exact encoding/resolution is an implementation decision still open.
- An assessment ref must resolve class ID and classSessionContentId as required by the opening route, not only assessmentId.
- Forum content uses upstream threadId, not the different thread-list id.
- Thread show returns the original lecturer/post text and attachment metadata, not all replies. Replies are separately requested; do not expose peers' identifiers merely because present upstream.
- Session show returns topic, timing/delivery information, and material/activity metadata without progress/attendance actions.
- SQLite persistence and automatic refresh are approved. Ordinary list/show commands attempt to refresh their requested scope, with list fetching listings only and show fetching selected content. `--offline` is the proposed explicit local-read option. Failed refresh may return a saved snapshot only with visible stale/partial status, the last successful fetch time and failure details; without a usable saved snapshot return an error. Coordinate concurrent refreshes of the same resource. Cached-body freshness must not be inferred from metadata unless the change marker is verified. Exact output fields, coordination mechanism, and stale exit behavior remain to be specified. Credential storage remains a separate design decision.
- No dashboard command, submission command, gradebook, attendance, or announcements in this v1 proposal.

## Proposed common JSON envelope

Successful or partially successful data commands emit one JSON object on stdout. Data can be a list or object according to the command. Progress/logs go to stderr, never mixed into JSON. No signed URLs, cookies, tokens, or raw auth payloads in ordinary output.

```json
{
  "schema_version": "1",
  "status": "partial",
  "data": [],
  "coverage": {
    "discovery_complete": true,
    "complete": false,
    "classes": [
      {"class_id": "example-class-a", "status": "complete"},
      {"class_id": "example-class-b", "status": "failed"}
    ]
  },
  "errors": [
    {
      "code": "upstream_timeout",
      "class_id": "example-class-b",
      "course_name": "Example Course",
      "stage": "assessment_list",
      "message": "Assessment list could not be retrieved.",
      "retryable": true
    }
  ],
  "warnings": [],
  "fetched_at": "<RFC3339 timestamp>"
}
```

- Complete means all requested sources/pages within the command's filters were retrieved, NOT that BINUS contains no hidden/other coursework or that submission was verified.
- If class discovery fails or is incomplete, report discovery_complete false; cannot claim all courses checked or name unknown missing classes.
- Empty success has status complete, data [], complete coverage, and no errors. All targets failing is status error, not empty success. At least one successful target with failures is partial, even if the successful targets yielded no items.
- Mixed/paginated failures retain available data and identify failed class, stage, and any item/session context. Pagination exhaustion or any explicit request cap truncation must make coverage incomplete.
- Unknown statuses/unsupported types are visible warnings or item errors; never silently disappear from an apparently complete result.
- `fetched_at` describes retrieval time, not source update time; retain source timestamps separately where available.

### Assessment list item (illustrative)

```json
{
  "ref": "example-assessment-ref",
  "class_id": "example-class-a",
  "title": "Example assignment",
  "due_at": "2027-01-14T23:59:00+07:00",
  "source_status": "Completed",
  "submission": {
    "state": "unknown",
    "verification": "not_checked"
  },
  "source_url": "https://lms.binus.ac.id/<non-secret-source-path>"
}
```

Keep inexpensive source evidence such as a first-submission timestamp when returned by the catalog, but do not equate an unverified card label with verified submission. `unknown + not_checked` does NOT mean no submission. Exact source-link generation must be verified rather than inventing routes.

### Assessment detail submission (illustrative)

```json
{
  "state": "submitted",
  "verification": "checked",
  "scope": "assessment",
  "evidence": {
    "questions": [{"source_question_id": "example-question", "is_submitted": true}],
    "first_submitted_at": "<source timestamp>",
    "source_card_status": "Completed",
    "source_opening_status": "Not Started",
    "source_card_total_attempted": 0,
    "source_opening_total_attemp": 1
  },
  "conflicts": ["opening_status_differs_from_submission_evidence"]
}
```

This sample assumes a single required question and sufficient evidence of its submission. Do not promote one submitted question to assessment-wide submitted when other required questions are missing/unknown. Proposed states: submitted, partially_submitted, not_submitted, unknown. Proposed verification: not_checked, checked, insufficient_evidence, blocked. A successful detail GET does not automatically mean checked/known. Preserve per-question/attempt/group scopes. Positive current or prior-attempt evidence means something was submitted; missing/false flags on a next attempt do not erase history. Grading and revision obligations are separate from submission.

- Detail also returns instructions, question text where read-only access is available, and attachment refs/names/types without automatically downloading files.
- Blocked group allocation remains a structured item error (e.g. group_membership_required); available catalog metadata may still be returned with partial status. Do not claim missing instructions are an empty successful read.
- Exact normalization hierarchy, latest versus previous attempt handling, and completed-group behavior require implementation tests and review. Preserve unknown for unsupported cases.

## Deadlines, dates, and attachments

- Preserve structured deadlines separately from lecturer prose. A forum closing date is not an assignment deadline. No LLM extraction or guessed deadline in the CLI v1 proposal; agents interpret original text.
- Preserve raw source date plus timezone provenance when timezone is absent. Normalize to RFC3339 only when the source timezone is established; never guess from the machine timezone. Year-0001 sentinels become null with retained source evidence as needed.
- Downloads default to refusing an existing destination. Use a temporary file and atomic rename on success; output JSON metadata after completion, not binary bytes on stdout. No file overwrite without explicit user choice.
- Do not forward BINUS Authorization headers to arbitrary attachment hosts or redirects. Signed links remain private and can be re-resolved from attachment refs; do not make a signed URL the public ID.

## Proposed process exit codes

| Code | Meaning |
|---|---|
| 0 | Complete requested read/download |
| 2 | Partial data available; coverage/errors explain missing parts |
| 1 | No usable result or global operation failure |
| 64 | Invalid arguments/usage |

JSON status remains authoritative; document that exit 2 still has usable stdout. This requires aligning argument-parser behavior rather than blindly accepting defaults. The auth status command must distinguish stored credentials from a live-validated session; exact behavior remains to be designed.

## Forum workflow amendment (pending contract revision)

Abdel reports forum threads are the usual assignment-submission surface, not the formal Assessment section. Titles such as Sesi 12 cannot distinguish information from assignments; routine discovery must make post bodies accessible.

The agreed boundary is now agent-supplied classification with CLI-managed local SQLite state, not an AI classifier embedded in Rust. SQLite stores source snapshots and separately attributed interpretations, scoped to the current account. Bounded batch body retrieval remains a proposal; exact command/limits and source rechecking policy remain undecided. This supersedes the earlier no-persistent-data-cache proposal; persistence is agreed, but cache-serving defaults are not.

### Proposed local classification command

```text
binus forums threads classify <thread-ref> \
  --kind assignment \
  --content-version <version-returned-by-show> \
  --reason "The lecturer requests an answer and gives a submission deadline."
```

This command is a proposed interface, not executable yet. Accepted kind values are proposed as assignment, information, uncertain; an absent record means unclassified, not information. A classification write inserts or updates local interpretation only, returning JSON read-back of what was stored. It never edits the forum, posts a reply, or marks coursework submitted.

Proposed safeguards: validate the account/thread ref and kind; require the interpreted source version; reject writes if that version differs from the latest locally stored source snapshot; atomically persist interpretation, source version, actor/provenance and audit history. If a subsequent source read detects changed content, retain historical interpretation but mark it stale/pending reconsideration. Version comparison only covers content already refreshed locally, not edits never fetched. An explicit refresh/recheck policy and concurrent-classification conflict behavior remain to be designed.

A future pending-work filter can expose unclassified/stale threads without conflating them with uncertain classifications or items already recorded in a calendar/vault. Command name and whether this is a list filter or a local queue command remain open. Extracted-deadline writes and destination-record writes are separate operations; exact interfaces are not yet specified. Fetching alone never marks any of them handled.

Defer AI interpretation of whether a reply actually fulfills the task in v1. Abdel's practice is questions go directly to lecturers or separate threads, so presence of his own reply can be a practical response/submission proxy for an assignment-classified thread. Preserve the raw own-reply fact and scope, not a universal claim of academic completion. False requires a successful adequate check; masked, incomplete, or failed retrieval must yield unknown. A cheap own-response field/filter may be preferable to downloading all classmates' replies but its reliability must be verified first.

Attachment metadata belongs in assessment/thread show, with reply attachments in separately requested replies. Expose stable attachment refs, name, available type/size, and parent question/post context; keep unknown metadata null and signed URLs private.

## Authentication and credential proposal (not yet approved)

### Login bootstrap

Recommend browser-assisted login using the BINUS website's actual flow, as exercised in research. `auth login` launches a dedicated controlled browser session/profile, lets the user complete Microsoft sign-in/MFA, follows the portal-to-LMS handoff, obtains the resulting BINUS token pair and role context, and verifies a safe direct API read before reporting success. Protect source-side read-only boundaries during the handoff and expose no passwords/codes to the agent. Ordinary data commands use direct HTTP afterward, not a running browser.

This is not a custom OAuth redirect or device-code claim: Microsoft requires registered redirect URIs/application configuration, and support for an independent CLI client has not been established. Arbitrarily reusing a website client ID for a localhost redirect is not the plan. A packaged controlled-browser bootstrap may require a browser dependency; distribution is not guaranteed one dependency-free binary. Dedicated login profiles must be disposed safely after credentials are secured unless a separately justified retention policy is agreed.

### Storage recommendation and headless tradeoff

Keep BINUS access/refresh tokens outside the coursework SQLite database. Prefer an OS credential-store integration on desktops, chosen using current Rust keyring ecosystem/backend documentation. Do not assume a graphical Linux Secret Service or an unlocked keyring is available to SSH/cron. Exact crate/backends and builds remain unselected.

For headless servers, propose an EXPLICITLY opted-in protected-file backend: owner-only credential file (0600 on Unix) inside an owner-only directory (0700), outside repo/vault/data exports, using atomic replacement. Windows would require appropriate ACL handling, not a Unix chmod assertion. This fallback is not encrypted at rest and does not protect against processes running as the same user or root. Never silently fall back to it when a keyring fails. An alternative encrypted backend needs a real independent unlock/key-management strategy; storing an encryption key beside the file does not solve this threat.

Save only the BINUS token pair plus necessary identity/role metadata; do not retain the Microsoft password, MFA codes, or Microsoft refresh credentials merely for convenience. Never emit secrets in JSON/logs. Namespace secrets and SQLite state by account. SQLite records credential references and nonsecret expiry metadata only, not raw tokens or signed attachment links.

### Renewal, status and logout

Before an online request, inspect expiry metadata and coordinate renewal per account. One process refreshes, persists the whole returned pair atomically, verifies a safe read, and wakes waiters; others reload the newest saved pair. Do not blindly retry a refresh after an ambiguous transport failure because rotation may have occurred. Reauthentication-required, credential-store locked/unavailable, upstream errors, and unknown refresh outcome must remain distinct. No Microsoft sign-in/MFA prompts from ordinary agent data commands; explicit auth login is interactive. Expired-access renewal remains unverified despite successful unexpired-pair refresh.

Propose auth status exposing stored account, storage backend, expiry metadata and whether live verification was performed, with an explicit check option for a safe online test. Presence of saved credentials is not proof they still work. Local logout removes this CLI's secrets and any retained CLI login profile, does not revoke unrelated browser sessions, and does not automatically delete coursework/history. Purging local student data would be a separate explicit operation.

### Decision and remaining validation

Abdel approved the explicitly opted-in, unencrypted permission-restricted credential-file backend for headless v1, accepting that same-user/root processes can read it. Stronger storage improvements are deferred to v2; baseline permissions, explicit consent, atomic replacement and secret-safe output are still mandatory. Desktop keyring availability, packaged browser capture, and headless browser-login UX require targeted validation. The existing research noVNC setup is evidence, not the public CLI onboarding implementation. No authentication code or backend has been installed by this design approval.

Sources consulted:
- https://docs.rs/keyring/latest/keyring/ (current docs distinguish simple compatibility mode from keyring-core plus selected credential-store backends)
- https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-auth-code-flow (registered redirect/flow requirements)

## Review checkpoint

Proposed additions needing review: JSON as default; stable context-bearing refs; freshness/refresh defaults for approved SQLite persistence; common completeness/error envelope; separate thread replies; normalized-state vocabulary; distinct exit codes; no-overwrite downloads; precise local classification command and concurrency rules. Approval of this draft is a design decision, not blanket authorization to implement every command.

Next implementation scope should be one vertical slice after approval, not a full scaffold claiming all four data domains work. Authentication bootstrap/storage and the initial slice must be agreed first.
