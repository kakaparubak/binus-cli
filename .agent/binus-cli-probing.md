---
tags: [binus, cli, api-research, authentication]
source: "BINUS CLI live probes; imported from project PROBING.md"
related_to: ["[[binus-cli]]"]
---

> Canonical findings and evidence map for [[binus-cli]]. Update this note as probes run. Raw responses, browser profiles, and credentials stay outside the synced vault and repository.

# BINUS CLI feasibility investigation

## Scope

Read-only, one-shot CLI consumed by AI agents. Open-source distribution is intended. No submissions, replies, uploads, attendance actions, or gradebook/notification features are authorized by this investigation.

These are observations of the current LMS web application and one student's account, not a supported public API contract.

## Verified in the earlier probes

Private evidence directory: `/home/abdel/.local/share/binus-browser/` (contains student data and browser state; do not commit).

- `direct-read-results.json`: standalone Python requests returned HTTP 200 for course sessions, assessment opening/instructions, and forum thread content, matching the captured browser responses. No browser cookies were sent.
- Course-session responses expose timing, delivery mode, resources, due dates, resource progress, and stable identifiers.
- Assessment opening responses expose start/end dates, instructions, submission status, attempts, group metadata, and attachment fields. An empty attachment array does not establish attachment-download coverage.
- Forum thread responses expose text, attachments, dates, author and thread metadata.
- Resource metadata retrieval did not change the sampled document's progressStatus, progressStamp, timesAccessed, resourceStatus, or lastUpdatedDate.
- `additional-results.json`: a ranged document request returned HTTP 206, 1,024 bytes, PowerPoint content type and ZIP signature. This proves partial document retrieval, not full-file integrity or every resource type.
- Removing authorization on a sampled academic-period read returned HTTP 500 with an empty body. Do not classify every 500 as expired authentication; real server errors may look similar.
- `probe-schedule-replay.json`: schedule retrieval was replayed from Python. The schedule API uses POST for a read operation.

## Continuation: executed checks

Evidence: `continuation-results.json`; runnable exploratory script: `continue_probe.py` in the private evidence directory.

### Forum comments

- A populated Database Technology GSLC thread returned HTTP 200.
- Read endpoint: `POST {forumBase}/Comment/PagingV20221122/{page}`.
- Payload structure was checked against the downloaded frontend's offline forum-caching implementation, with `forumId` set to the class ID and `parentId`/`threadId` set to the thread ID. Sort value `LatestPost` is present in the frontend.
- Requested five comments per page. Actual page sizes: 5, 5, 5, 5, 3.
- Collected 23 top-level comments with 23 unique IDs, matching the API's declared total of 23. Each page returned HTTP 200. No cookies were sent.
- Some comments included two nested replies. The schema exposes `repliesContinuousToken`, `nextReply`, `totalReply`, `totalNextReply`, and `totalPreviousReply`. At this stage nested-reply completeness was unverified; the later coverage probe verified six embedded replies against declared counts. Multi-page nested continuation remains unverified.
- Fields include comment text, attachment, author ID/role, posting dates, IDs, replies, and counts. Do not dump classmates' content into general agent context when a count or own-response status suffices.

### Authentication and headers

- Current BINUS access-token claims declare a lifetime of 86,400 seconds.
- Current BINUS refresh-token claims declare a lifetime of 604,800 seconds.
- These are token claim lifetimes, not proof of renewable unattended access.
- The academic-period GET returned HTTP 200 with (a) full browser role context, (b) bearer token alone, and (c) bearer token plus organization ID. This does NOT establish that role context is optional on other endpoints.
- Frontend inspection identifies Microsoft token exchange through `/AccessAndRefreshToken/AzureAD`, followed by separate BINUS access and refresh tokens.
- Frontend inspection identifies `/AccessAndRefreshToken/Refresh` with an `accessToken`/`refreshToken` JSON body.
- After explicit user authorization, `refresh_probe.py` executed one live refresh from standalone Python. HTTP 200; no browser cookies were sent. Both access and refresh token values changed.
- The new access token declares a 24-hour lifetime and the new refresh token declares a 7-day lifetime. Both expiry timestamps advanced by 1,213 seconds relative to the previous pair, consistent with reissuance from the time of refresh.
- Verified the new access token with a separate `GET /AcademicPeriod/Student`: HTTP 200 and a JSON array containing three periods, without cookies.
- Preserved the returned token pair atomically outside the repository at `/home/abdel/.local/share/binus-browser/refreshed-auth.private.json`, mode 0600. Never commit or print this file. Sanitized evidence: `refresh-results.json` in the private research directory.
- No refresh request was sent during the initial failed invocation: the research browser's CDP endpoint was unavailable. The existing browser profile was restarted before the single live refresh attempt.
- Revocation of the previous pair, renewal after access-token expiry, refresh-token expiry behavior, and long-term rolling renewal remain untested. Do not equate a successful refresh of an unexpired pair with proven indefinite unattended access.

### Clean first-login test (completed through LMS access)

- Created a new isolated Chromium browser context with no imported cookies or application storage. Existing authenticated browser context and saved refreshed pair were left intact.
- Navigating to `https://lms.binus.ac.id/login` reached Microsoft sign-in. The user completed sign-in manually, reaching the central BINUSMAYA dashboard.
- The portal's authorization request uses client ID `fbfc3dff-b8a9-42fe-8952-69be308dbb79`, response type `code`, response mode `fragment`, and redirect URI `https://binusmaya.binus.ac.id/home`. Requested scopes are `user.read openid profile mail.read calendars.read offline_access`.
- Clicking the actual `CONTINUE TO LMS` button reached `https://lms.binus.ac.id/lms/dashboard` without another user sign-in in this test.
- Observed LMS authentication exchange: `POST https://apim-bm7-prod-web.azure-api.net/func-bm7-identity-prod/AccessAndRefreshToken/AzureAD`, HTTP 200. Payload keys: `accessToken`, `continuePath`, `refreshToken`. The response body was unavailable to the capture, so response schema is not claimed from that event.
- Verified authenticated LMS Redux state contains an access/refresh pair and active role context. Preserved it outside the repository in `clean-lms-state.private.json`, mode 0600, within the private research directory.
- A separate Python process loaded that saved state without any browser/CDP calls and successfully read `/AcademicPeriod/Student`: HTTP 200, JSON array, no cookies. Token claims declare 86,400-second access and 604,800-second refresh lifetimes.
- Evidence: `clean-login-initial.json` and `clean-login-results.json`. The handoff guard reported no blocked mutations during this dashboard transition.
- The portal code-based authorization differs from the legacy LMS helper using `response_type=token` and client ID `ad768ba0-0059-46cf-ac71-c4e627361377`. These can be separate stages rather than competing explanations. The completed test proves the user's portal-to-LMS journey and reusable LMS credentials, not every intermediate Microsoft token request.
- Browser-assisted onboarding is feasible for this account. A standalone CLI callback, own registered OAuth client, device-code login, secure credential-store integration, and setup on other students' machines are NOT yet implemented or verified.

### Class/session discovery and nested replies

- Standalone Python read the active-class list and session lists for all 10 returned class components (LEC and LAB are separate classes). Retrieved 135 sessions in total; session IDs were unique within each class. This covers the returned active classes, not historical periods.
- Verified session-to-thread discovery using `POST /Thread/Class/{classId}/Session/{sessionId}/Paging/{page}` with the frontend's `TotalDataPerPage` payload field.
- Three sampled sessions (Database Technology, Algorithm Design and Analysis, Artificial Intelligence) returned five threads altogether. ADA and AI each declared two threads; requesting one per page returned two pages with two unique `threadId` values, matching the totals.
- IMPORTANT: thread-list `id` and `threadId` differ. Use `threadId` for thread content and comment requests. An initial local comparison against `id` incorrectly failed to recognize the known GSLC thread; inspecting both fields resolved this, and the probe/check result was corrected.
- The GSLC thread's 23 top-level comments contain six nested replies in total, matching all per-comment `totalReply` values, with unique reply IDs. This sample's replies were fully embedded; no continuation token was needed.
- Inspected comments in the other four discovered sample threads for a real nested continuation token; none was present. The frontend documents read-only `POST /Reply` continuation and `/Reply/Paging/Next`, but actual multi-page nested continuation remains UNVERIFIED. Do not claim pagination was exercised from these embedded-only replies.
- Both coverage batches returned HTTP 200 for all 22 requests and used direct API calls rather than browser navigation. Main-batch request logging confirmed no cookies sent; the extra batch likewise used explicit bearer/role headers without a cookie jar.
- Evidence: `coverage-results.json`, `coverage-request-log.json`, `extra-coverage-results.json`, and `extra-coverage-log.json` in the private research directory. Raw session/thread/reply records remain there, not in this repository.

### Assessment attachments and submission semantics (2026-10-18)

#### Catalog coverage and read endpoints

- Queried course-evaluation groups for all 10 returned active class components, then followed each nonempty group's assessment-card continuation chain. Found eight assessment cards: three `Completed`, four `Not Started`, one `In Progress`; four individual and four group. Card counts matched the course-evaluation totals; no continuation remained.
- Assessment base: `https://apim-bm7-assessment-prod.azure-api.net/func-bm7-assessment-prod`.
- Group discovery: `GET /Assessment/CourseEvaluation/Student/Course/{courseId}/Class/{classId}?academicPeriod={period}&courseCode={code}&ssrComponent={component}`. IMPORTANT: omitting those query parameters returned HTTP 200 and empty lists, including a class known to contain an assessment. Correcting the request from frontend code restored the expected results. An HTTP 200 empty result alone is not proof of no assignments.
- Card query: `POST /Assessment/Class/{classId}/CourseEvaluation/Cards/Student`; payload includes `continuationToken`, `lamType`, empty status/feedback/availability/session/subject/type filters, `orderBy: asc`, `sortBy: title`.
- Opening read: `GET /Assessment/ClassId/{classId}/ClassSessionContent/{contentId}/Opening/Platform/Web`. Six returned HTTP 200. Two group assessments returned HTTP 400 with `errorGroupSet: true`, requiring group allocation, not expired login.
- Authentic question/attachment read: `GET /ClassId/{classId}/Assessment/{assessmentId}/Question/Answer/Review/{versionId}`, using the opening's version ID. All five accessible authentic-assessment samples returned HTTP 200, including unstarted assignments. The completed objective quiz was not opened for question/answer review; no need to retrieve quiz answers to establish instruction/status access.

#### Status inconsistencies and observed evidence

| Sample | Card | Opening | Corroborating evidence |
|---|---|---|---|
| Computational Physics Tugas 1 | Completed; totalAttempted 0 | status Not Started; assessmentStatus 2; totalAttemp 1 | first-submission timestamp present, last attempt awaiting instructor assessment, question isSubmitted true |
| OOP GSLC | Completed; totalAttempted 0 | status Not Started; assessmentStatus 2; totalAttemp 1 | first-submission timestamp present, last attempt awaiting instructor assessment, question isSubmitted true |
| CB objective quiz | Completed; isScored false | status Not Started; assessmentStatus 2; totalAttemp 1 | first-submission timestamp present, lastAttempt.status says work has been assessed |
| Database Technology group AOL | In Progress | status Incomplete; assessmentStatus 1; totalAttemp 0; no lastAttempt | no first-submission timestamp, group-submitted flag false |
| AI group AOL and Physics individual AOL | Not Started | status Not Started; assessmentStatus 1; totalAttemp 0; no lastAttempt | no first-submission timestamp, no submitted answer |

- Do not use opening `status` alone as overall submission state. It can describe a fresh/next attempt even when a previous submission exists; that interpretation is plausible, not a universal documented contract.
- Do not use card `totalAttempted == 0` to conclude unsubmitted. All three completed cards had zero there, while the opening reported one attempt.
- Do not equate Completed with graded, or card `isScored == false` with definitely ungraded. Preserve the distinct source facts and report conflicts rather than silently resolving them.
- `0001-01-01T00:00:00` appears as the first-submission sentinel on unsubmitted cards. Treat it as absent submission evidence, not an actual date.
- `groupSubmissionOneAnswer` also appeared true on an individual-mode AOL. Interpret it together with `assessmentMode`, not as the sole indicator that an assessment is a group task.
- Actual group responses expose participants, `groupSubmissionOneAnswer`, `isGroupSubmissionOneAnswerSubmitted`, and group-answer fields. Both accessible group samples were unsubmitted. No completed-group sample exists in this catalog, so another member's completed submission satisfying the user's obligation remains UNVERIFIED.
- Missing group allocation blocks assessment detail access for ADA AOL and Computer Networks AOL. Card metadata and deadline remain available. Preserve this as a per-item blocked/partial result, not an empty assignment or global auth failure. No group was joined or changed.
- Dates include naive due/start timestamps and UTC submission timestamps; preserve this distinction. Previously captured LMS card UI labels times GMT+7. Do not apply machine-local timezone interpretation blindly.
- Late/resubmission controls are separate: two completed authentic samples allow late submission but disallow reattempt-before-scoring; other samples expose maxAttempt 3 or 2. `maxAttempt: -1` occurs but its exact operational meaning was not tested. No late submission or reattempt was executed.

#### Actual task-file retrieval

- Every accessible authentic sample had one instructor task attachment under `questions[].attachments`, even though card/opening `instructions.attachment` arrays were empty. An empty instruction-attachment list does NOT mean there are no task files.
- Retrieved all five instructor files fully from the verified `stbm7resourcesprod.blob.core.windows.net` host: HTTP 200, response byte counts match Content-Length, no bearer token or browser cookies forwarded.
- Files: one PDF (PDF header and EOF marker present) and four DOCX packages (`word/document.xml` present; ZIP CRC tests passed). This validates full-byte retrieval and package structure, not full PDF semantic validation or document-content extraction.
- Did not download students' answer attachments. Raw review responses contain own answer metadata and remain in the private evidence directory, outside the synced vault/repository. Do not publish signed attachment URLs.
- Attachment content type was `application/octet-stream` even for PDF/DOCX; do not infer file type solely from Content-Type. Unstarted question IDs were null; stable assessment/content IDs remain usable.

#### Read-only verification and evidence

- No StartAttempt, StartAnswer, save, submit, upload, or group-change endpoint was called. Only known read POSTs queried catalog cards; all opening/review/file operations were GETs.
- Re-reading openings after each of the five authentic review reads confirmed unchanged `assessmentStatus`, `status`, `totalAttemp`, `versionId`, and `lastAttempt`.
- Re-read all eight catalog cards after the review/download probes. IDs and `status`, `firstAttemptSubmittedDate`, `totalAttempted`, `maxAttempt`, and `isScored` remained unchanged.
- Sanitized results: `assessment-catalog-results.json`, `assessment-opening-results.json`, `assessment-question-results.json`, `assessment-attachment-results.json`, `assessment-verification-results.json`. Scripts: `assessment_probe.py`, `assessment_openings.py`, `assessment_questions.py`, `assessment_attachments.py`. All are under the private research directory.
- This bounded probe is complete for available samples. Remaining edge cases are graded-state conflict resolution, completed group submissions, actual late/multiple-attempt behavior, group-blocked details, and objective-question pre-attempt access. These must remain unknown rather than being invented or tested by mutating coursework.

### Hidden browser writes

Opening a forum thread triggered a PUT to `/PostFeed/ClassId/{classId}/ThreadId/{threadId}`. The request guard blocked it. Treat it as a write; its exact server-side effect is unverified. This is why browser navigation is not automatically read-only. Prefer explicit, allowlisted API reads and do not replay every captured request.

## Remaining feasibility work

1. Verify nested-reply pagination against declared reply counts.
2. Extend thread discovery beyond the three sampled sessions and verify historical-period coverage. Session lists for all 10 returned active class components (135 sessions) have already been retrieved.
3. Test renewal after actual access-token expiry and inspect refresh-token expiry/revocation semantics using the securely preserved newest pair. A controlled unexpired-pair refresh and follow-up safe read have passed; do not repeat refresh unnecessarily or reuse stale browser tokens as the canonical CLI credential.
4. Choose and verify distributable browser-assisted CLI onboarding and reauthentication. A clean context, user Microsoft login, portal-to-LMS transition, credential capture, and independent Python read have passed for this account; a packaged CLI login command has not been built.
5. Assessment read probe completed for available samples: eight catalog cards, six accessible openings, five authentic question reads, and five full instructor-file downloads. Completed-group semantics, late/multiple-attempt behavior, objective pre-attempt question access, and group-allocation-blocked details remain unverified. These are explicit limits for the first CLI contract, not grounds to mutate coursework.

## Current conclusion

Browser-independent retrieval, controlled refresh of an unexpired token pair, and browser-assisted first-login onboarding are feasible for the sampled account. A browser need not drive every command. A distributable minimal-setup CLI login and reliable unattended renewal after expiry remain unproven. Keep the investigation separate from production implementation until the onboarding and credential-storage design is agreed.
