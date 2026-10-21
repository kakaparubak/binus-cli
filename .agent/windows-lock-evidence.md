# Windows cross-session mutex evidence

## Scope and source basis

Implemented only the Windows mutex prerequisite. No credential backend, browser, storage integration, process-signal port, credential access, filesystem ACL work, or live API operation was performed. The Windows runtime is not booted on this Linux host.

Authoritative API references supplied and reviewed for the implementation:

- [Kernel object namespaces](https://learn.microsoft.com/en-us/windows/win32/termserv/kernel-object-namespaces): `Global\\` naming spans sessions; the stated `SeCreateGlobalPrivilege` requirement applies to global file mappings and symbolic links, not mutexes.
- [CreateMutexExW](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createmutexexw): explicit security attributes, access request, existing-object behavior, and incompatible object type failure.
- [Synchronization object security and access rights](https://learn.microsoft.com/en-us/windows/win32/sync/synchronization-object-security-and-access-rights), [GetSecurityInfo](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo), [ConvertStringSecurityDescriptorToSecurityDescriptorW](https://learn.microsoft.com/en-us/windows/win32/api/sddl/nf-sddl-convertstringsecuritydescriptortosecuritydescriptorw), and [WaitForSingleObject](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject).
- Binding signatures/constants were checked against `windows-sys 0.61.2` source under `~/.cargo/registry/src/.../windows-sys-0.61.2/src/Windows/Win32`. Cargo features enabled: `Win32_Foundation`, `Win32_Security`, `Win32_Security_Authorization`, `Win32_System_Threading`.

## Platform-neutral test-first policy

The acceptance policy is `accepts_descriptor`, and the native adapter converts inspected owner/DACL/ACE facts into that same policy. Wait result classification is `wait_decision`, which the native acquisition path consumes before deciding whether to return a guard, release abandoned ownership, or fail.

- Red: temporarily forced `accepts_descriptor` to always reject; `cargo test --manifest-path spikes/001-auth/Cargo.toml --lib windows_lock::tests::valid_descriptor_is_accepted -- --exact --test-threads=1` failed as expected (exit 101, assertion failure).
- Green: restored the policy and ran the full all-target Linux suite below. Policy tests cover valid descriptor acceptance; wrong owner; absent, null, and unprotected DACL; extra ACE; deny/non-allowed ACE; inherited flags; wrong SID; wrong mask; and acquired/abandoned/timed-out/failed wait results.

## Native implementation and typecheck

`spikes/001-auth/src/windows_lock/native.rs` obtains the effective thread token, falling back to the process token only on `ERROR_NO_TOKEN`; reads `TOKEN_USER` into aligned backing storage; builds an explicit owner and protected one-ACE DACL; creates a versioned `Global\\BinusAuthSpike.Lock.v1` mutex with minimal handle rights; validates every returned object's owner and DACL before waiting; and returns a thread-bound RAII guard. `WAIT_ABANDONED` releases acquired ownership and returns sanitized `storage_error`. Handles and `LocalAlloc` security descriptors use RAII.

An isolated non-shipping harness at `.agent/windows-lock-check` imports the real module by path and calls its public acquisition API. It does not duplicate native code or run a Windows binary. Command:

```text
cargo check --manifest-path .agent/windows-lock-check/Cargo.toml --all-targets --target x86_64-pc-windows-msvc
```

Result: exit 0, no warnings. This verifies the module and its Windows-only tests typecheck against the installed Windows target and actual bindings. It does not mean the whole CLI is Windows-compatible.

Native runtime tests are present for private test names (process-scoped) covering acquire/drop, contention timeout, a mismatching existing descriptor, and abandoned ownership. They were typechecked only and remain **PENDING** until run on Windows. No Windows binary or mutex was executed here.

## Linux verification

- `cargo test --manifest-path spikes/001-auth/Cargo.toml --all-targets -- --test-threads=1` — passed: 117 tests across library, CLI, and examples (87 library, 11 CLI, 19 example tests).
- `cargo fmt --manifest-path spikes/001-auth/Cargo.toml -- --check` — passed.
- `cargo clippy --manifest-path spikes/001-auth/Cargo.toml --all-targets -- -D warnings` — passed.
- `cargo build --manifest-path spikes/001-auth/Cargo.toml --release` — passed.
- Isolated Windows typecheck command above — passed.

## Changes made for this task

- `spikes/001-auth/src/windows_lock.rs`
- `spikes/001-auth/src/windows_lock/native.rs`
- `spikes/001-auth/src/lib.rs` (narrow module exposure under Windows or tests)
- `spikes/001-auth/Cargo.toml` (target-specific binding dependency/features)
- `spikes/001-auth/Cargo.lock` (Cargo resolution includes the target-specific dependency; this file already had unrelated working-tree edits before this task)
- `.agent/windows-lock-check/Cargo.toml`, `.agent/windows-lock-check/Cargo.lock`, `.agent/windows-lock-check/src/main.rs` (isolated verification harness)
- `.agent/windows-lock-evidence.md`, `.agent/windows-lock-summary.md`

Existing working-tree changes were present before this task and were not reverted or intentionally edited outside the files listed above. Storage/backend integration remains for parent review and a later task.
