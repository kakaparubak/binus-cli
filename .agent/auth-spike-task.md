# Approved task: Rust BINUS auth spike on Omarchy

Use gpt-6-luna. All project source, scripts, docs and tests MUST live under /home/abdel/Documents/Coding/binus-cli on this machine. Do not develop a second project on the VPS. User explicitly requests Codex implementation here. Repo is initialized and originally empty. Read /home/abdel/AGENTS.md and applicable project instructions first. Do not commit or push.

Scope is ONE authentication feasibility spike, NOT production full CLI. Build standalone runnable Rust crate under spikes/001-auth/ with README verdict, root pointer README and .gitignore. Do not implement course/forum/assessment commands, SQLite tracking, classification, OS keyring, a scheduler, or submissions. A desktop credential file backend is permitted for this explicitly opted-in experimental spike; do not silently choose plaintext. User approved headless v1 file credentials, not broad security downgrades.

## Environment

Codex CLI 0.162.1, authenticated via ChatGPT; model slug gpt-6-luna confirmed from model metadata. /usr/bin/chromium and /usr/bin/brave-origin exist. There is Wayland socket /run/user/1000/wayland-1. Rust/cargo/rustup not on PATH. You may install a user-local minimal Rust toolchain if necessary, without sudo or system package changes. Read actual rustup/crate docs first; no guessed current versions. ~/.cargo and ~/.rustup are allowed for toolchain caches; project code stays in named workspace. Network access for docs/crates/toolchain is expected. Do not change user browser profiles, Hyprland/GPU config, or existing auth. No reading ~/.codex/auth.json or unrelated secrets. Do not dump credentials or broad environment variables.

## Feasibility already demonstrated on research VPS

BINUS website login -> Microsoft interactive sign-in/MFA -> https://binusmaya.binus.ac.id portal -> CONTINUE TO LMS -> https://lms.binus.ac.id/lms/dashboard. Fresh isolated browser context required only one Microsoft sign-in. Actual portal request uses code/fragment and redirect https://binusmaya.binus.ac.id/home; do NOT invent own OAuth redirect/device code from website client IDs.

Downstream authentication exchange observed:
POST https://apim-bm7-prod-web.azure-api.net/func-bm7-identity-prod/AccessAndRefreshToken/AzureAD
query includes subscription-key (not provided; resolve actual request, never log full secret query).
Body keys: accessToken, continuePath, refreshToken. HTTP200 observed; response token body capture was unavailable in that research run. A working fallback read BINUS pair and role context from authenticated LMS Redux state. Never extract Microsoft passwords/codes; user does interactive login.

Previously tested frontend snapshot bundle: https://lms.binus.ac.id/static/js/main.794de131.chunk.js (may change; inspect current page/script if needed). Page-local research webpack accessor technique worked:
window['webpackJsonpbinus-lms-web'].push([[98768], {98768: function(m,e,r){window.__binusResearchRequire=r}}, [[98768]]]);
const r=window.__binusResearchRequire; const a=r(34).b.getState().auth.res;
// a.token, a.refreshToken, a.activeRole, a.activeRoleOrganizationId; r(3).n.course is course base.
This is unstable frontend coupling, not public contract. Use only narrow validated in-memory fallback in dedicated BINUS page if network capture cannot supply necessary context. Do not expose values via logs/debug/exception strings. Network response capture route needs real validation; be honest if not proven.

Verified safe read:
GET https://apim-bm7-prod-web.azure-api.net/func-bm7-course-prod/AcademicPeriod/Student
Authorization: Bearer <BINUS token>
rOId: activeRoleOrganizationId
academicCareer: activeRole.academicCareer
institution: activeRole.institution
roleName: activeRole.name
roleId: activeRole.roleId
HTTP200 expected JSON array; no browser cookies needed. Observed issuer BinusServices, audience Nexus.IdentityService. 24h/7day token lifetimes observed, not promised universal. JWT decoding isn't signature validation. Missing auth may yield empty HTTP500; don't label every500 auth expired.

Live credential pair and research profile are NOT copied to this machine and must NOT be fabricated or fetched from VPS. Live end-to-end verification will require the user to sign in through new local controlled browser.

## Requested implementation

A minimal Rust CLI, e.g. spike auth login / auth status / auth check / auth logout (names practical, not final contract), using real HTTP/CDP as appropriate. clap/reqwest/serde/tokio candidates, choose minimum dependencies after documentation checks.

1. Dedicated ephemeral browser profile, loopback-only CDP, no opening public ports. Prefer launch with CDP port 0 and read DevToolsActivePort; ensure proper lifecycle cleanup. Browser runs as user and retains normal sandbox; no disabling sandbox unless separately justified/approved. Set needed Wayland runtime environment explicitly for GUI launch from SSH, without modifying user's global settings. No automatic credential entry. Login cancellation/timeout explicit. Do NOT start an unattended login and wait forever during build/test; document exact user command and emit login_required blocker for manual stage.
2. Capture ONLY intended BINUS authentication payload programmatically (prefer response; fallback app state if necessary) and role metadata. Validate exact trusted HTTPS host/path before transmitting credentials; never send to arbitrary user URLs/redirect destinations. Restrict capture; do not collect keyboard, screenshots, Microsoft auth bodies, all response dumps, cookies, raw HAR, full URLs with tokens, or console logs. Mask secret Debug/Error output.
3. Preserve read-only coursework boundary: pages can silently PUT /PostFeed/ClassId/.../ThreadId/... and POST /ToDo/ETextBook/Redeem. Guard source API mutations before navigating app (request interception for applicable targets/popups), allow verified identity exchange and verified read POSTs only. Never call StartAttempt, StartAnswer, save, submit, upload, attendance, group updates. If complete guard/capture cannot be implemented in this bounded spike, explicitly document limits and avoid opening coursework pages; dashboard/auth only. Never claim zero side effects just because GET.
4. File backend requires explicit opt-in such as --allow-plaintext-file. Store one structured secret record atomically, directory0700/file0600 on Unix, outside repo/synced vault by default (XDG app path). Scope by account; reject insecure/symlink/unsafe target handling as appropriate. No raw token fields in status/stdout/logs. Include role context with pair for safe read. Keep newest captured pair recoverable before verification; do not clobber existing working pair accidentally on failed capture. Browser profile must be closed/disposed after appropriate secure save or cancellation.
5. Independent auth check process loads credential file and performs safe AcademicPeriod GET without browser/CDP/cookies. Return sanitized machine-readable JSON. Distinguish stored versus live-verified status, capture timeout, invalid capture, storage errors, upstream error, reauth_required where proven. Never print request URLs containing subscription keys or tokens. No retrying credential-changing operations. Refresh is OUT OF SCOPE for this slice unless absolutely required; don't fake it.
6. Local logout removes only this spike's credentials, not Microsoft sessions or coursework history. For spike distinguish not_logged_in from failure.
7. Strict TDD for logic: write and RUN failing behavioral test, then minimal implementation, then pass, iterate vertically. Use isolated synthetic fixtures for parsing/storage/error/sanitization; label mocks as tests not live evidence. Include tests for permissions/atomic writes, missing or malformed tokens, trusted-origin filtering, secret-free JSON/errors, no-cookie safe reads (local fake server permitted ONLY test-specific so production cannot bypass trusted host), CLI explicit plaintext consent, and cancellation/timeout where practical. Record red-green evidence without secrets. Compile and execute cargo test, fmt --check, clippy with warnings denied, CLI help/status smoke tests. No invented success.

## Finish / handoff

Write README with exact run commands from repo, dependencies installed, security limitations, test commands actually executed, and verdict VALIDATED/PARTIAL/INVALIDATED. Bounded spike is PARTIAL until real BINUS sign-in and independent API read are demonstrated. Do not claim live auth passed from fixtures. State what user must do next. Produce final report with absolute artifact paths and actual test results, blockers, chosen extraction route, and browser behavior. No commits/pushes. If blocked on auth/user input, finish with built artifact + exact blocker; do not ask user via chat.
