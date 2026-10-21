# Windows mutex focused correction 1/2 — evidence

## Scope and baseline

Scope is limited to `spikes/001-auth/src/windows_lock.rs`, `spikes/001-auth/src/windows_lock/native.rs`, the Windows isolated type-check harness, and this evidence/summary. No storage, browser, credentials, live API, or unrelated protection changes.

Before hashes:

- `windows_lock.rs`: `b0011aa32eadc02311e8dc1d9276182d3165e70845d09df95c8dd0b473143f91`
- `windows_lock/native.rs`: `2e492807e1d28faee0d2bfc1b1da568523f6a3e4fafd78a4a8abcd9c1f9ac4ff`

## Regression-first record

Completed regression: the Linux naming-policy regression was added first, failed on the missing helper, then passed after the helper was implemented. Native Windows runtime regressions cannot execute in this Linux environment and remain explicitly pending.

## Changes made

- `windows_lock.rs`: added `mutex_name_for_sid`, producing `Global\\BinusAuthSpike.Lock.v1.{canonical_sid}`; added an executable Linux unit regression for stable same-SID naming, different-SID separation, and fixed Global/version prefix.
- `windows_lock/native.rs`: production obtains the effective token and SID once, converts that exact SID with `ConvertSidToStringSidW`, builds the SID-scoped name, and passes that same SID pointer into the owner/DACL construction and descriptor inspection. The generated SID string is not logged. Existing parameterless public `acquire`, protected DACL/owner checks, single exact ACE rule, and minimal handle rights are retained. `ConvertSidToStringSidW` output is wrapped in the existing `Local` RAII type and freed with `LocalFree`.
- Native contention now has an owner thread hold the mutex while a distinct contender performs a short bounded wait; channels coordinate ownership and release. Native drop and abandoned-wait regressions keep an independent open handle to preserve object identity, then require a distinct thread to acquire the same mutex. The abandoned path still expects the sanitized error before checking availability.
- `.agent/windows-lock-check/src/main.rs`: retains the real source-module path and adds a test target so the Windows native test module is type-checked with `cfg(test)`.

### API binding and allocation lifecycle

The pinned `windows-sys 0.61.2` binding is `Win32::Security::Authorization::ConvertSidToStringSidW(PSID, *mut PWSTR) -> BOOL`; `Win32_Foundation` provides `LocalFree`. The binding is enabled by the harness and application `Win32_Security_Authorization` feature. Microsoft documents a null-terminated output string and requires freeing it with `LocalFree`: [ConvertSidToStringSidW](https://learn.microsoft.com/windows/win32/api/sddl/nf-sddl-convertsidtostringsidw). The SID string buffer remains under RAII cleanup, including UTF-16 conversion failure.

### Regression run record

- Red: after adding the Linux test before the helper, `cargo test --manifest-path spikes/001-auth/Cargo.toml --lib windows_lock::naming_tests::mutex_names_are_sid_scoped_and_versioned` failed with unresolved import `mutex_name_for_sid`.
- Green: the same focused test passed after adding the helper (1 passed).
- Native contention/drop/abandonment tests cannot execute on this Linux host. Their Windows test code was type-checked; no native runtime result is claimed.

### Checks

- `cargo test --manifest-path spikes/001-auth/Cargo.toml --all-targets -- --test-threads=1` — PASS (exit 0; includes all Linux-executable targets).
- `cargo fmt --manifest-path spikes/001-auth/Cargo.toml --check` — PASS.
- `cargo clippy --manifest-path spikes/001-auth/Cargo.toml --all-targets -- -D warnings` — PASS.
- `cargo check --manifest-path spikes/001-auth/Cargo.toml --release` — PASS.
- `cargo check --manifest-path .agent/windows-lock-check/Cargo.toml --all-targets --target x86_64-pc-windows-msvc` — PASS.
- Fresh test-target type check: `CARGO_TARGET_DIR=/tmp/binus-win-lock-fix1-check cargo check --manifest-path .agent/windows-lock-check/Cargo.toml --tests --target x86_64-pc-windows-msvc` — PASS; compiles the harness with `cfg(test)` and the actual `windows_lock/native.rs` tests.
- Harness `cargo fmt --manifest-path .agent/windows-lock-check/Cargo.toml --check` — PASS.

## Still pending

Windows native runtime tests, real cross-session behavior, and the cleanup effects on Windows have not been executed. This correction does not establish whole-CLI Windows compatibility or storage/backend completion. No reboot or cross-boot test was performed.

## Result hashes

- `windows_lock.rs`: `f6384bf317ba0ce7de4d56b901092dd0e52297778000f199e979c995bca9645f`
- `windows_lock/native.rs`: `630df06fa3f23b5c7090128b9b0875b7efc24fd3971983b1366e7662eb601e96`
