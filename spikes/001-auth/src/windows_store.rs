use crate::CredentialRecord;
#[cfg(windows)]
use crate::windows_lock;
use std::path::Path;
#[cfg(windows)]
use std::ptr;

const MAX_BLOB: usize = 2560;
#[cfg(windows)]
const PRIMARY_TARGET: &str = "BinusAuthSpike.v1.credentials";
#[cfg(windows)]
const CANDIDATE_TARGET: &str = "BinusAuthSpike.v1.credentials.captured";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Key {
    Primary,
    Candidate,
}
fn key(path: &Path) -> Result<Key, &'static str> {
    match path.file_name().and_then(|s| s.to_str()) {
        Some("credentials.json") => Ok(Key::Primary),
        Some("credentials.captured.json") => Ok(Key::Candidate),
        _ => Err("storage_error"),
    }
}
#[cfg(windows)]
impl Key {
    fn target(self) -> &'static str {
        match self {
            Key::Primary => PRIMARY_TARGET,
            Key::Candidate => CANDIDATE_TARGET,
        }
    }
}

trait Api {
    fn read(&mut self, key: Key) -> Result<Option<Vec<u8>>, &'static str>;
    fn write(&mut self, key: Key, bytes: &[u8]) -> Result<(), &'static str>;
    fn delete(&mut self, key: Key) -> Result<bool, &'static str>;
}
fn with_lock<G, T>(
    acquire: impl FnOnce() -> Result<G, &'static str>,
    operation: impl FnOnce() -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    let _guard = acquire()?;
    operation()
}
fn decode(bytes: &[u8]) -> Result<CredentialRecord, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_BLOB {
        return Err("storage_error");
    }
    let value: CredentialRecord = serde_json::from_slice(bytes).map_err(|_| "storage_error")?;
    if !crate::valid_credential_record(&value) {
        return Err("storage_error");
    }
    Ok(value)
}
fn load<A: Api>(api: &mut A, key: Key) -> Result<CredentialRecord, &'static str> {
    api.read(key)?
        .ok_or("not_logged_in")
        .and_then(|bytes| decode(&bytes))
}
fn store<A: Api>(api: &mut A, key: Key, record: &CredentialRecord) -> Result<(), &'static str> {
    if !crate::valid_credential_record(record) {
        return Err("storage_error");
    }
    let bytes = serde_json::to_vec(record).map_err(|_| "storage_error")?;
    if bytes.len() > MAX_BLOB {
        return Err("credential_too_large");
    }
    api.write(key, &bytes)
}
fn preferred<A: Api>(api: &mut A) -> Result<(CredentialRecord, bool), &'static str> {
    match load(api, Key::Candidate) {
        Ok(r) => Ok((r, true)),
        Err("not_logged_in") => load(api, Key::Primary).map(|r| (r, false)),
        Err(e) => Err(e),
    }
}
fn promote<A: Api>(api: &mut A, verified: &CredentialRecord) -> Result<bool, &'static str> {
    let snapshot = serde_json::to_vec(verified).map_err(|_| "storage_error")?;
    let candidate = load(api, Key::Candidate)?;
    if serde_json::to_vec(&candidate).map_err(|_| "storage_error")? != snapshot {
        return Err("candidate_changed");
    }
    store(api, Key::Primary, verified)?;
    api.delete(Key::Candidate)?;
    Ok(true)
}
fn logout<A: Api>(api: &mut A) -> Result<bool, &'static str> {
    let a = api.delete(Key::Primary);
    let b = api.delete(Key::Candidate);
    match (a, b) {
        (Ok(x), Ok(y)) => Ok(x || y),
        _ => Err("storage_error"),
    }
}

#[cfg(test)]
fn register_fixture_target(namespace: Option<&str>, created: &mut Vec<String>, target: String) {
    if namespace.is_some() {
        created.push(target);
    }
}

#[cfg(test)]
fn fixture_cleanup_targets(namespace: Option<&str>, created: &mut Vec<String>) -> Vec<String> {
    if namespace.is_some() {
        std::mem::take(created)
    } else {
        Vec::new()
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::ffi::c_void;
    use windows_sys::Win32::{
        Foundation::{ERROR_NOT_FOUND, GetLastError},
        Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
            CredReadW, CredWriteW,
        },
    };
    struct Cred(*mut CREDENTIALW);
    impl Drop for Cred {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    CredFree(self.0.cast::<c_void>());
                }
            }
        }
    }
    pub(super) struct WinApi {
        #[cfg(test)]
        namespace: Option<String>,
        #[cfg(test)]
        created: Vec<String>,
    }
    impl WinApi {
        fn target(&self, key: Key) -> Vec<u16> {
            #[cfg(test)]
            if let Some(namespace) = &self.namespace {
                let suffix = match key {
                    Key::Primary => ".primary",
                    Key::Candidate => ".candidate",
                };
                return format!("{namespace}{suffix}")
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
            }
            key.target().encode_utf16().chain(Some(0)).collect()
        }
        #[cfg(test)]
        pub(super) fn isolated(namespace: String) -> Result<Self, &'static str> {
            for target in [
                format!("{namespace}.primary"),
                format!("{namespace}.candidate"),
                format!("{namespace}.unrelated"),
            ] {
                if read_target(&target)?.is_some() {
                    return Err("storage_error");
                }
            }
            Ok(Self {
                namespace: Some(namespace),
                created: Vec::new(),
            })
        }
        #[cfg(test)]
        pub(super) fn unrelated_target(&self) -> String {
            format!("{}.unrelated", self.namespace.as_deref().unwrap())
        }
        #[cfg(test)]
        pub(super) fn write_unrelated(&mut self, bytes: &[u8]) -> Result<(), &'static str> {
            let target = self.unrelated_target();
            write_target(&target, bytes)?;
            self.created.push(target);
            Ok(())
        }
        #[cfg(test)]
        pub(super) fn read_unrelated(&self) -> Result<Option<Vec<u8>>, &'static str> {
            read_target(&self.unrelated_target())
        }
    }
    #[cfg(test)]
    impl Drop for WinApi {
        fn drop(&mut self) {
            for name in fixture_cleanup_targets(self.namespace.as_deref(), &mut self.created) {
                let target: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
                unsafe {
                    CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0);
                }
            }
        }
    }
    #[cfg(test)]
    fn write_target(name: &str, bytes: &[u8]) -> Result<(), &'static str> {
        if bytes.is_empty() || bytes.len() > MAX_BLOB {
            return Err("credential_too_large");
        }
        let target: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let mut cred = CREDENTIALW::default();
        cred.Type = CRED_TYPE_GENERIC;
        cred.TargetName = target.as_ptr() as *mut u16;
        cred.CredentialBlobSize = bytes.len() as u32;
        cred.CredentialBlob = bytes.as_ptr() as *mut u8;
        cred.Persist = CRED_PERSIST_LOCAL_MACHINE;
        if unsafe { CredWriteW(&cred, 0) } == 0 {
            Err("storage_error")
        } else {
            Ok(())
        }
    }
    #[cfg(test)]
    pub(super) fn read_target(name: &str) -> Result<Option<Vec<u8>>, &'static str> {
        let target: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let mut raw = ptr::null_mut();
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) } == 0 {
            return if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err("storage_error")
            };
        }
        let cred = Cred(raw);
        if cred.0.is_null() {
            return Err("storage_error");
        }
        let c = unsafe { &*cred.0 };
        if c.CredentialBlob.is_null() || c.CredentialBlobSize == 0 {
            return Err("storage_error");
        }
        Ok(Some(
            unsafe { std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize) }
                .to_vec(),
        ))
    }
    impl Api for WinApi {
        fn read(&mut self, key: Key) -> Result<Option<Vec<u8>>, &'static str> {
            let target = self.target(key);
            let mut raw = ptr::null_mut();
            let success = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) };
            let cred = Cred(raw);
            if success == 0 {
                return if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                    Ok(None)
                } else {
                    Err("storage_error")
                };
            }
            if cred.0.is_null() {
                return Err("storage_error");
            }
            let c = unsafe { &*cred.0 };
            if c.Type != CRED_TYPE_GENERIC || c.TargetName.is_null() {
                return Err("storage_error");
            }
            let expected: Vec<u16> = target[..target.len() - 1].to_vec();
            let mut actual = Vec::with_capacity(expected.len());
            for i in 0..=expected.len() {
                let unit = unsafe { *c.TargetName.add(i) };
                if unit == 0 {
                    break;
                }
                if i >= expected.len() {
                    return Err("storage_error");
                }
                actual.push(unit);
            }
            if actual != expected {
                return Err("storage_error");
            }
            let len = c.CredentialBlobSize as usize;
            if len == 0 || len > MAX_BLOB || c.CredentialBlob.is_null() {
                return Err("storage_error");
            }
            Ok(Some(
                unsafe { std::slice::from_raw_parts(c.CredentialBlob, len) }.to_vec(),
            ))
        }
        fn write(&mut self, key: Key, bytes: &[u8]) -> Result<(), &'static str> {
            if bytes.is_empty() || bytes.len() > MAX_BLOB {
                return Err("credential_too_large");
            }
            let target = self.target(key);
            let mut cred = CREDENTIALW::default();
            cred.Type = CRED_TYPE_GENERIC;
            cred.TargetName = target.as_ptr() as *mut u16;
            cred.CredentialBlobSize = bytes.len() as u32;
            cred.CredentialBlob = bytes.as_ptr() as *mut u8;
            cred.Persist = CRED_PERSIST_LOCAL_MACHINE;
            if unsafe { CredWriteW(&cred, 0) } == 0 {
                Err("storage_error")
            } else {
                #[cfg(test)]
                register_fixture_target(
                    self.namespace.as_deref(),
                    &mut self.created,
                    String::from_utf16_lossy(&target[..target.len() - 1]),
                );
                Ok(())
            }
        }
        fn delete(&mut self, key: Key) -> Result<bool, &'static str> {
            let target = self.target(key);
            if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0 {
                return Ok(true);
            }
            if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                Ok(false)
            } else {
                Err("storage_error")
            }
        }
    }
    fn locked<T>(
        f: impl FnOnce(&mut WinApi) -> Result<T, &'static str>,
    ) -> Result<T, &'static str> {
        with_lock(windows_lock::acquire, || {
            f(&mut WinApi {
                #[cfg(test)]
                namespace: None,
                #[cfg(test)]
                created: Vec::new(),
            })
        })
    }
    #[cfg(test)]
    pub(super) fn locked_isolated<T>(
        api: &mut WinApi,
        f: impl FnOnce(&mut WinApi) -> Result<T, &'static str>,
    ) -> Result<T, &'static str> {
        with_lock(windows_lock::acquire, || f(api))
    }
    pub fn store_record(path: &Path, r: &CredentialRecord) -> Result<(), &'static str> {
        let k = key(path)?;
        locked(|a| store(a, k, r))
    }
    pub fn load_record(path: &Path) -> Result<CredentialRecord, &'static str> {
        let k = key(path)?;
        locked(|a| load(a, k))
    }
    pub fn remove_record_file(path: &Path) -> Result<bool, &'static str> {
        let k = key(path)?;
        locked(|a| a.delete(k))
    }
    pub fn remove_spike_artifacts(path: &Path) -> Result<bool, &'static str> {
        if key(path)? != Key::Primary {
            return Err("storage_error");
        }
        locked(logout)
    }
    pub fn promote_verified_candidate(
        primary: &Path,
        candidate: &Path,
        r: &CredentialRecord,
    ) -> Result<bool, &'static str> {
        if key(primary)? != Key::Primary || key(candidate)? != Key::Candidate {
            return Err("storage_error");
        }
        locked(|a| promote(a, r))
    }
    pub fn load_preferred_record(primary: &Path) -> Result<(CredentialRecord, bool), &'static str> {
        if key(primary)? != Key::Primary {
            return Err("storage_error");
        }
        locked(preferred)
    }
}
#[cfg(windows)]
pub use native::{
    load_preferred_record, load_record, promote_verified_candidate, remove_record_file,
    remove_spike_artifacts, store_record,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    #[derive(Default)]
    struct Mem {
        map: HashMap<Key, Vec<u8>>,
        fail_write: bool,
        fail_delete_primary: bool,
        fail_delete_candidate: bool,
        fail_read: bool,
        reads: usize,
        deletes: usize,
        unrelated_sentinel: Vec<u8>,
    }
    impl Api for Mem {
        fn read(&mut self, k: Key) -> Result<Option<Vec<u8>>, &'static str> {
            self.reads += 1;
            if self.fail_read {
                return Err("storage_error");
            }
            Ok(self.map.get(&k).cloned())
        }
        fn write(&mut self, k: Key, b: &[u8]) -> Result<(), &'static str> {
            if self.fail_write {
                return Err("storage_error");
            }
            self.map.insert(k, b.to_vec());
            Ok(())
        }
        fn delete(&mut self, k: Key) -> Result<bool, &'static str> {
            self.deletes += 1;
            if (self.fail_delete_primary && k == Key::Primary)
                || (self.fail_delete_candidate && k == Key::Candidate)
            {
                return Err("storage_error");
            }
            Ok(self.map.remove(&k).is_some())
        }
    }
    fn rec(account: &str) -> CredentialRecord {
        CredentialRecord {
            account: account.into(),
            access_token: "a".repeat(16),
            refresh_token: "b".repeat(16),
            role: crate::RoleContext {
                academic_career: "x".into(),
                institution: "x".into(),
                name: "x".into(),
                role_id: "x".into(),
                organization_id: "x".into(),
            },
        }
    }

    #[test]
    fn engine_roundtrip_and_exact_size_preflight() {
        let mut m = Mem::default();
        let old = rec("old");
        store(&mut m, Key::Primary, &old).unwrap();
        let mut exact = rec("");
        let mut n = 0;
        loop {
            exact.account.push('x');
            n += 1;
            if serde_json::to_vec(&exact).unwrap().len() >= MAX_BLOB {
                break;
            }
        }
        exact.account.clear();
        let base = serde_json::to_vec(&exact).unwrap().len();
        exact.account.push_str(&"x".repeat(MAX_BLOB - base));
        assert_eq!(serde_json::to_vec(&exact).unwrap().len(), MAX_BLOB);
        store(&mut m, Key::Primary, &exact).unwrap();
        exact.account.push('x');
        assert_eq!(serde_json::to_vec(&exact).unwrap().len(), MAX_BLOB + 1);
        assert_eq!(
            store(&mut m, Key::Primary, &exact),
            Err("credential_too_large")
        );
        assert_eq!(
            load(&mut m, Key::Primary).unwrap().account.len(),
            MAX_BLOB - base
        );
        assert!(n > 0);
    }
    #[test]
    fn malformed_records_and_native_errors_fail_closed() {
        let mut m = Mem::default();
        m.map.insert(Key::Candidate, b"{".to_vec());
        assert!(matches!(preferred(&mut m), Err("storage_error")));
        m.map.insert(Key::Candidate, vec![]);
        assert!(matches!(load(&mut m, Key::Candidate), Err("storage_error")));
        m.map.insert(Key::Candidate, vec![0; MAX_BLOB + 1]);
        assert!(matches!(load(&mut m, Key::Candidate), Err("storage_error")));
        let mut invalid = rec("bad-token");
        invalid.access_token = "short".into();
        m.map
            .insert(Key::Candidate, serde_json::to_vec(&invalid).unwrap());
        assert!(matches!(load(&mut m, Key::Candidate), Err("storage_error")));
        let mut invalid = rec("bad-role");
        invalid.role.name.clear();
        m.map
            .insert(Key::Candidate, serde_json::to_vec(&invalid).unwrap());
        assert!(matches!(load(&mut m, Key::Candidate), Err("storage_error")));
        m.fail_read = true;
        m.map
            .insert(Key::Primary, serde_json::to_vec(&rec("primary")).unwrap());
        assert!(matches!(preferred(&mut m), Err("storage_error")));
    }
    #[test]
    fn promotion_and_preference() {
        let mut m = Mem::default();
        let old = rec("old");
        let r = rec("candidate");
        store(&mut m, Key::Primary, &old).unwrap();
        store(&mut m, Key::Candidate, &r).unwrap();
        assert!(preferred(&mut m).unwrap().1);
        assert!(promote(&mut m, &r).unwrap());
        assert!(!preferred(&mut m).unwrap().1);
        store(&mut m, Key::Candidate, &old).unwrap();
        assert_eq!(promote(&mut m, &r), Err("candidate_changed"));
        m.fail_write = true;
        assert_eq!(promote(&mut m, &old), Err("storage_error"));
        assert!(load(&mut m, Key::Candidate).is_ok());
        assert_eq!(load(&mut m, Key::Primary).unwrap().account, "candidate");
        m.fail_write = false;
        m.fail_delete_candidate = true;
        assert_eq!(promote(&mut m, &old), Err("storage_error"));
        assert_eq!(load(&mut m, Key::Primary).unwrap().account, "old");
        assert!(load(&mut m, Key::Candidate).is_ok());
        m.map.remove(&Key::Candidate);
        assert!(!preferred(&mut m).unwrap().1);
    }
    #[test]
    fn logout_attempts_both() {
        let mut m = Mem::default();
        m.map.insert(Key::Primary, vec![1]);
        m.map.insert(Key::Candidate, vec![1]);
        m.fail_delete_primary = true;
        m.unrelated_sentinel = vec![9, 8, 7];
        assert_eq!(logout(&mut m), Err("storage_error"));
        assert_eq!(m.map.len(), 1);
        assert!(!m.map.contains_key(&Key::Candidate));
        assert_eq!(m.unrelated_sentinel, vec![9, 8, 7]);
        assert_eq!(m.deletes, 2);
        assert_eq!(m.reads, 0);
    }
    #[test]
    fn only_closed_keys_are_supported() {
        assert!(key(Path::new("/anything/credentials.json")).is_ok());
        assert!(key(Path::new("/anything/credentials.captured.json")).is_ok());
        assert_eq!(key(Path::new("/anything/other.json")), Err("storage_error"));
    }

    #[test]
    fn fixture_cleanup_registration_requires_isolated_namespace() {
        let mut production_created = Vec::new();
        register_fixture_target(
            None,
            &mut production_created,
            "synthetic.production-route".into(),
        );
        assert!(production_created.is_empty());

        // Even stale or accidentally seeded metadata cannot authorize cleanup
        // for the non-isolated adapter.
        production_created.push("synthetic.must-survive".into());
        assert!(fixture_cleanup_targets(None, &mut production_created).is_empty());
        assert_eq!(production_created, ["synthetic.must-survive"]);

        let mut isolated_created = Vec::new();
        register_fixture_target(
            Some("synthetic.fixture"),
            &mut isolated_created,
            "synthetic.fixture.primary".into(),
        );
        assert_eq!(
            fixture_cleanup_targets(Some("synthetic.fixture"), &mut isolated_created),
            ["synthetic.fixture.primary"]
        );
        assert!(isolated_created.is_empty());
    }

    #[test]
    fn lock_covers_candidate_read_compare_primary_write_and_delete() {
        use std::cell::Cell;
        struct Guard<'a>(&'a Cell<bool>);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        struct LockedApi<'a> {
            inner: Mem,
            held: &'a Cell<bool>,
            calls: usize,
        }
        impl Api for LockedApi<'_> {
            fn read(&mut self, k: Key) -> Result<Option<Vec<u8>>, &'static str> {
                assert!(self.held.get());
                self.calls += 1;
                self.inner.read(k)
            }
            fn write(&mut self, k: Key, b: &[u8]) -> Result<(), &'static str> {
                assert!(self.held.get());
                self.calls += 1;
                self.inner.write(k, b)
            }
            fn delete(&mut self, k: Key) -> Result<bool, &'static str> {
                assert!(self.held.get());
                self.calls += 1;
                self.inner.delete(k)
            }
        }
        let held = Cell::new(false);
        let mut inner = Mem::default();
        let r = rec("verified");
        store(&mut inner, Key::Candidate, &r).unwrap();
        let mut api = LockedApi {
            inner,
            held: &held,
            calls: 0,
        };
        let result = with_lock(
            || {
                held.set(true);
                Ok(Guard(&held))
            },
            || promote(&mut api, &r),
        );
        assert_eq!(result, Ok(true));
        assert_eq!(api.calls, 3);
        assert!(!held.get());
    }
}

#[cfg(all(test, windows))]
mod native_tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_NAMESPACE: AtomicU64 = AtomicU64::new(0);

    fn isolated_api() -> native::WinApi {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let namespace = format!(
            "BinusAuthSpike.test.{}.{}.{}",
            std::process::id(),
            nanos,
            NEXT_NAMESPACE.fetch_add(1, Ordering::Relaxed)
        );
        native::WinApi::isolated(namespace).unwrap()
    }

    fn rec(account: &str) -> CredentialRecord {
        CredentialRecord {
            account: account.into(),
            access_token: "a".repeat(16),
            refresh_token: "b".repeat(16),
            role: crate::RoleContext {
                academic_career: "x".into(),
                institution: "x".into(),
                name: "x".into(),
                role_id: "x".into(),
                organization_id: "x".into(),
            },
        }
    }

    fn assert_record_eq(actual: &CredentialRecord, expected: &CredentialRecord) {
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }

    #[test]
    fn native_save_load_delete_roundtrip_uses_isolated_credentials() {
        let mut api = isolated_api();
        let expected = rec("isolated-roundtrip");
        native::locked_isolated(&mut api, |api| {
            store(api, Key::Primary, &expected)?;
            assert_record_eq(&load(api, Key::Primary)?, &expected);
            assert!(api.delete(Key::Primary)?);
            assert!(matches!(load(api, Key::Primary), Err("not_logged_in")));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn native_candidate_preference_verified_promotion_and_cleanup() {
        let mut api = isolated_api();
        let verified = rec("isolated-candidate");
        native::locked_isolated(&mut api, |api| {
            store(api, Key::Candidate, &verified)?;
            let (preferred_record, is_candidate) = preferred(api)?;
            assert_record_eq(&preferred_record, &verified);
            assert!(is_candidate);
            assert!(promote(api, &verified)?);
            assert_record_eq(&load(api, Key::Primary)?, &verified);
            assert!(matches!(load(api, Key::Candidate), Err("not_logged_in")));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn native_logout_preserves_unrelated_test_credential_in_same_keyspace() {
        let mut api = isolated_api();
        let unrelated = b"synthetic-unrelated-test-credential";
        native::locked_isolated(&mut api, |api| {
            store(api, Key::Primary, &rec("isolated-primary"))?;
            store(api, Key::Candidate, &rec("isolated-candidate"))?;
            api.write_unrelated(unrelated)?;
            assert_eq!(api.read_unrelated()?.as_deref(), Some(unrelated.as_slice()));
            assert!(logout(api)?);
            assert!(matches!(load(api, Key::Primary), Err("not_logged_in")));
            assert!(matches!(load(api, Key::Candidate), Err("not_logged_in")));
            assert_eq!(api.read_unrelated()?.as_deref(), Some(unrelated.as_slice()));
            Ok(())
        })
        .unwrap();
    }
}
