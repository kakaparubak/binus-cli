# Windows Credential Manager storage evidence

Date: 2026-10-20 (Asia/Jakarta)
Scope: storage backend assessment only. No product source changes were made in this turn.

## Existing API review

Read `.agent/windows-portability-evidence.md`, `spikes/001-auth/src/lib.rs` storage operations (lines 104-425), and CLI call sites in `src/cli.rs`.

- `store_record(path, record)` serializes one `CredentialRecord`; the Linux backend locks then writes the record.
- `load_record(path)` returns `not_logged_in` only for missing storage and `storage_error` for malformed or inaccessible data.
- `load_preferred_record(primary)` checks `credentials.captured.json` first and falls back to primary only when candidate is absent.
- `promote_verified_candidate(primary, candidate, verified)` locks primary, rereads candidate, compares serialized snapshots, writes primary, then removes candidate. Mismatch returns `candidate_changed`; primary write failure preserves candidate.
- `remove_spike_artifacts` is logout cleanup; it has deliberately narrow app artifact ownership. `remove_record_file` accepts only primary or candidate.

The CLI currently has Linux plaintext consent logic and Linux path/browser assumptions. No CLI changes were made; wiring Windows login without the consent flag depends on the backend/path/browser port and must be documented as a later integration step if storage remains isolated.

## Documentation checked

- Microsoft [`CredReadW`](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credreadw): reads the credential set associated with the current token; `ERROR_NOT_FOUND` is absence; returned allocation must be released with `CredFree`.
- Microsoft [`CredWriteW`](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew): creates or replaces a target and associates it with current token; persistence must be set explicitly in the credential.
- Microsoft [kernel object namespaces](https://learn.microsoft.com/en-us/windows/win32/termserv/kernel-object-namespaces): `Global\\` names cross-session objects; unprefixed/`Local\\` names are session scoped.
- Microsoft [synchronization object security](https://learn.microsoft.com/en-us/windows/win32/sync/synchronization-object-security-and-access-rights): named mutexes support an explicit security descriptor; default ACL derives from creator token.
- Microsoft [mutex objects](https://learn.microsoft.com/en-us/windows/win32/sync/mutex-objects): wait reports an abandoned mutex distinctly; callers must handle it.
- Rust binding documentation checked: [`windows-sys::CredReadW`](https://docs.rs/windows-sys/latest/windows_sys/Win32/Security/Credentials/fn.CredReadW.html) and [`windows-sys::CreateMutexW`](https://docs.rs/windows-sys/latest/windows_sys/Win32/System/Threading/fn.CreateMutexW.html). No guessed FFI was introduced.

## Blocker and security decision needed

The existing API requires one lock to serialize storage operations and promotion. Credential Manager local-machine persistence is intended to survive user logons, while a `Local\\` mutex only coordinates processes in one logon session. Using it would leave a race between sessions and would not satisfy the requested promotion/write guarantees.

A plausible implementation is a `Global\\` mutex whose name is namespaced by the current user's SID and whose creation security descriptor grants access only to that SID. Implementing this safely requires a documented SID acquisition and ACL construction path, exact `windows-sys` API/features/types, correct handling when another session has already created the object with an incompatible descriptor, and RAII release/close behavior. This turn did not validate that path or add a binding dependency. Therefore no native adapter or test seam was added; a session-local mutex was not substituted.

This is the requested stop condition for a cross-session mutex design that needs a security decision. Resolve the global mutex ACL/creation policy before implementation. Do not treat this as a completed Windows backend.

## Work and validation status

- No implementation files changed in this turn.
- No TDD red/green steps were run; no test result is claimed.
- No cargo test, fmt, clippy, release, or Windows target check was run in this turn.
- Native Windows execution remains pending; Windows 11 was not booted, and no browser, login, live API, Credential Manager, or real credentials were accessed.
- Existing preflight Windows cross-target failures remain as recorded in `.agent/windows-portability-evidence.md`; they include Unix storage imports, browser process-group code, Unix signals, and Unix-only tests. They are not reclassified as storage implementation failures or successes here.
- No token values or real credential data were read, printed, or copied.

## Required implementation follow-up

After resolving the mutex security descriptor policy, implement and test the backend-neutral validation and orchestration seam, then add the native `CredReadW`/`CredWriteW`/`CredDeleteW`/`CredFree` adapter using actual binding signatures. Keep each primary/candidate as a single JSON blob and reject serialized records over 2560 bytes before any OS write. Do not add compression, chunking, DPAPI, or plaintext fallback. Scope this work to storage; keep browser/process/signal portability as separately reported blockers.
