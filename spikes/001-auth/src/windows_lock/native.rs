use super::mutex_name_for_sid;
use super::{AcePolicy, DescriptorPolicy, WaitDecision, accepts_descriptor, wait_decision};
use std::{ffi::c_void, marker::PhantomData, rc::Rc};
use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_NO_TOKEN, GetLastError, HANDLE, LocalFree},
    Security::Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT},
    Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_REVISION, ACL_SIZE_INFORMATION,
        AddAccessAllowedAce, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation,
        GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetSecurityDescriptorOwner,
        GetTokenInformation, InitializeAcl, IsValidSid, OBJECT_SECURITY_INFORMATION,
        OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SE_DACL_PRESENT, SE_DACL_PROTECTED,
        SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR, SECURITY_DESCRIPTOR_CONTROL,
        SetSecurityDescriptorControl, SetSecurityDescriptorDacl, SetSecurityDescriptorOwner,
        TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    System::Threading::{
        CreateMutexExW, GetCurrentProcess, GetCurrentThread, MUTEX_ALL_ACCESS, MUTEX_MODIFY_STATE,
        OpenThreadToken, ReleaseMutex, WaitForSingleObject,
    },
};

const SYNCHRONIZE: u32 = 0x0010_0000;
const HANDLE_ACCESS: u32 = SYNCHRONIZE | MUTEX_MODIFY_STATE | 0x0002_0000; // READ_CONTROL
const WAIT_MS: u32 = 10_000;

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}
struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

/// A mutex ownership guard is deliberately neither Send nor Sync: Windows mutex ownership
/// belongs to the thread that acquired it.
pub struct MutexGuard {
    handle: Handle,
    _thread_bound: PhantomData<Rc<()>>,
}
impl Drop for MutexGuard {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.handle.0);
        }
    }
}

struct Token(Handle);
fn effective_token() -> Result<Token, &'static str> {
    let mut h = std::ptr::null_mut();
    let ok = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut h) };
    if ok != 0 {
        return Ok(Token(Handle(h)));
    }
    if unsafe { GetLastError() } != ERROR_NO_TOKEN {
        return Err("storage_error");
    }
    let mut h = std::ptr::null_mut();
    let ok = unsafe {
        windows_sys::Win32::System::Threading::OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY,
            &mut h,
        )
    };
    if ok == 0 {
        Err("storage_error")
    } else {
        Ok(Token(Handle(h)))
    }
}

unsafe fn user_sid(token: HANDLE) -> Result<(Vec<u64>, PSID), &'static str> {
    let mut needed = 0;
    unsafe {
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
    }
    if needed < std::mem::size_of::<TOKEN_USER>() as u32 || needed > 1024 * 1024 {
        return Err("storage_error");
    }
    // Vec<u64> supplies alignment required for TOKEN_USER and its embedded SID pointer.
    let mut backing = vec![0u64; (needed as usize).div_ceil(8)];
    if unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            backing.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err("storage_error");
    }
    let tu = unsafe { &*backing.as_ptr().cast::<TOKEN_USER>() };
    if tu.User.Sid.is_null() || unsafe { IsValidSid(tu.User.Sid) } == 0 {
        return Err("storage_error");
    }
    let length = unsafe { windows_sys::Win32::Security::GetLengthSid(tu.User.Sid) } as usize;
    if length < 8 || length > 68 {
        return Err("storage_error");
    }
    Ok((backing, tu.User.Sid))
}

unsafe fn descriptor(
    sid: PSID,
    ace_mask: u32,
) -> Result<(Vec<u64>, SECURITY_DESCRIPTOR), &'static str> {
    let acl_bytes = 1024usize;
    let mut acl_storage = vec![0u64; acl_bytes.div_ceil(8)];
    let acl = acl_storage.as_mut_ptr().cast::<ACL>();
    let mut sd = SECURITY_DESCRIPTOR::default();
    if unsafe { InitializeAcl(acl, acl_bytes as u32, ACL_REVISION) } == 0
        || unsafe { AddAccessAllowedAce(acl, ACL_REVISION, ace_mask, sid) } == 0
        || unsafe {
            windows_sys::Win32::Security::InitializeSecurityDescriptor(
                (&mut sd as *mut SECURITY_DESCRIPTOR).cast(),
                1,
            )
        } == 0
        || unsafe {
            SetSecurityDescriptorOwner((&mut sd as *mut SECURITY_DESCRIPTOR).cast(), sid, 0)
        } == 0
        || unsafe {
            SetSecurityDescriptorDacl((&mut sd as *mut SECURITY_DESCRIPTOR).cast(), 1, acl, 0)
        } == 0
        || unsafe {
            SetSecurityDescriptorControl(
                (&mut sd as *mut SECURITY_DESCRIPTOR).cast(),
                SE_DACL_PROTECTED,
                SE_DACL_PROTECTED,
            )
        } == 0
    {
        return Err("storage_error");
    }
    // Keep ACL allocation alive through CreateMutexExW; descriptor copied by the kernel.
    Ok((acl_storage, sd))
}

unsafe fn inspect(handle: HANDLE, expected_sid: PSID) -> Result<(), &'static str> {
    let mut owner: PSID = std::ptr::null_mut();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let info: OBJECT_SECURITY_INFORMATION = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    if unsafe {
        GetSecurityInfo(
            handle,
            SE_KERNEL_OBJECT,
            info,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut sd,
        )
    } != 0
        || sd.is_null()
    {
        return Err("storage_error");
    }
    let _sd = Local(sd.cast());
    let mut control: SECURITY_DESCRIPTOR_CONTROL = 0;
    let mut revision = 0;
    let mut owner_out = std::ptr::null_mut();
    let mut owner_defaulted = 0;
    let mut present = 0;
    let mut dacl_out = std::ptr::null_mut();
    let mut dacl_defaulted = 0;
    if unsafe { GetSecurityDescriptorControl(sd, &mut control, &mut revision) } == 0
        || unsafe { GetSecurityDescriptorOwner(sd, &mut owner_out, &mut owner_defaulted) } == 0
        || unsafe {
            GetSecurityDescriptorDacl(sd, &mut present, &mut dacl_out, &mut dacl_defaulted)
        } == 0
        || owner_out.is_null()
        || unsafe { IsValidSid(owner_out) } == 0
        || unsafe { EqualSid(owner_out, expected_sid) } == 0
        || control & SE_DACL_PRESENT == 0
        || control & SE_DACL_PROTECTED == 0
        || present == 0
        || dacl_out.is_null()
    {
        return Err("storage_error");
    }
    let mut sizes = ACL_SIZE_INFORMATION::default();
    if unsafe {
        GetAclInformation(
            dacl_out,
            (&mut sizes as *mut ACL_SIZE_INFORMATION).cast(),
            std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            2,
        )
    } == 0
        || sizes.AceCount != 1
    {
        return Err("storage_error");
    }
    let mut ace_ptr: *mut c_void = std::ptr::null_mut();
    if unsafe { GetAce(dacl_out, 0, &mut ace_ptr) } == 0 || ace_ptr.is_null() {
        return Err("storage_error");
    }
    let hdr = unsafe { &*ace_ptr.cast::<ACE_HEADER>() };
    if (hdr.AceSize as usize) < std::mem::size_of::<ACCESS_ALLOWED_ACE>() + 8
        || (hdr.AceSize as usize) > sizes.AclBytesInUse as usize
    {
        return Err("storage_error");
    }
    let ace = unsafe { &*ace_ptr.cast::<ACCESS_ALLOWED_ACE>() };
    let sid_ptr: PSID = unsafe {
        ace_ptr
            .cast::<u8>()
            .add(std::mem::offset_of!(ACCESS_ALLOWED_ACE, SidStart))
            .cast()
    };
    let sid_prefix_len = 8usize;
    let sid_subauthorities = unsafe { *sid_ptr.cast::<u8>().add(1) } as usize;
    let sid_len_from_header = sid_prefix_len + sid_subauthorities * 4;
    let sid_offset = std::mem::offset_of!(ACCESS_ALLOWED_ACE, SidStart);
    if sid_len_from_header > hdr.AceSize as usize - sid_offset {
        return Err("storage_error");
    }
    if unsafe { IsValidSid(sid_ptr) } == 0 || unsafe { EqualSid(sid_ptr, expected_sid) } == 0 {
        return Err("storage_error");
    }
    let sid_len = unsafe { windows_sys::Win32::Security::GetLengthSid(sid_ptr) } as usize;
    if hdr.AceSize as usize != sid_offset + sid_len || sid_len != sid_len_from_header {
        return Err("storage_error");
    }
    let accepted = accepts_descriptor(
        &DescriptorPolicy {
            owner_matches: true,
            dacl_present: true,
            dacl_protected: true,
            null_dacl: false,
            aces: vec![AcePolicy {
                allowed: hdr.AceType == 0,
                explicit: hdr.AceFlags == 0,
                mask: ace.Mask,
                sid_matches: true,
            }],
        },
        MUTEX_ALL_ACCESS,
    );
    if accepted {
        Ok(())
    } else {
        Err("storage_error")
    }
}

pub fn acquire() -> Result<MutexGuard, &'static str> {
    let token = effective_token()?;
    let (_sid_backing, sid) = unsafe { user_sid(token.0.0)? };
    let canonical_sid = unsafe { sid_string(sid)? };
    acquire_with_sid(mutex_name_for_sid(&canonical_sid), sid, WAIT_MS)
}

unsafe fn sid_string(sid: PSID) -> Result<String, &'static str> {
    let mut wide = std::ptr::null_mut();
    if unsafe {
        windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW(sid, &mut wide)
    } == 0
        || wide.is_null()
    {
        return Err("storage_error");
    }
    let _wide = Local(wide.cast());
    let mut len = 0;
    while unsafe { *wide.add(len) } != 0 {
        len += 1;
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(wide, len) })
        .map_err(|_| "storage_error")
}

#[cfg(test)]
fn private_test_name(label: &str) -> String {
    format!("Global\\BinusAuthSpike.Test.{label}.{}", unsafe {
        windows_sys::Win32::System::Threading::GetCurrentProcessId()
    })
}

#[cfg(test)]
fn acquire_named(name: &str) -> Result<MutexGuard, &'static str> {
    acquire_named_wait(name, WAIT_MS)
}

#[cfg(test)]
fn acquire_named_wait(name: &str, wait_ms: u32) -> Result<MutexGuard, &'static str> {
    let token = effective_token()?;
    let (_sid_backing, sid) = unsafe { user_sid(token.0.0)? };
    acquire_with_sid(name.to_owned(), sid, wait_ms)
}

fn acquire_with_sid(name: String, sid: PSID, wait_ms: u32) -> Result<MutexGuard, &'static str> {
    let (_acl_storage, sd) = unsafe { descriptor(sid, MUTEX_ALL_ACCESS)? };
    let mut attrs = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: (&sd as *const SECURITY_DESCRIPTOR).cast_mut().cast(),
        bInheritHandle: 0,
    };
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let h = unsafe { CreateMutexExW(&mut attrs, wide.as_ptr(), 0, HANDLE_ACCESS) };
    if h.is_null() {
        return Err("storage_error");
    }
    let handle = Handle(h);
    unsafe {
        inspect(handle.0, sid)?;
    }
    let result = unsafe { WaitForSingleObject(handle.0, wait_ms) };
    match wait_decision(result) {
        WaitDecision::Acquired => Ok(MutexGuard {
            handle,
            _thread_bound: PhantomData,
        }),
        WaitDecision::Abandoned => {
            unsafe {
                ReleaseMutex(handle.0);
            }
            Err("storage_error")
        }
        WaitDecision::TimedOut | WaitDecision::Failed => Err("storage_error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin_object(name: &str) -> Handle {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let h = unsafe { CreateMutexExW(std::ptr::null_mut(), wide.as_ptr(), 0, HANDLE_ACCESS) };
        assert!(!h.is_null(), "independent mutex handle");
        Handle(h)
    }

    fn assert_other_thread_acquires(name: String) {
        std::thread::spawn(move || acquire_named_wait(&name, 500).map(drop))
            .join()
            .expect("acquirer thread")
            .expect("mutex should be available");
    }

    #[test]
    fn native_private_mutex_acquires_and_releases() {
        let name = private_test_name("Acquire");
        {
            let guard = acquire_named(&name).expect("isolated mutex");
            let _pin = pin_object(&name);
            drop(guard);
            assert_other_thread_acquires(name.clone());
        }
    }

    #[test]
    fn native_private_mutex_contention_times_out() {
        let name = private_test_name("Contention");
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let owner_name = name.clone();
        let owner = std::thread::spawn(move || {
            let _guard = acquire_named(&owner_name).expect("first acquisition");
            held_tx.send(()).expect("signal held");
            release_rx.recv().expect("release signal");
        });
        held_rx.recv().expect("owner acquired mutex");
        let _pin = pin_object(&name);
        let attempt_name = name.clone();
        let contender = std::thread::spawn(move || acquire_named_wait(&attempt_name, 200).err());
        assert_eq!(
            contender.join().expect("contender thread"),
            Some("storage_error")
        );
        release_tx.send(()).expect("allow owner release");
        owner.join().expect("owner thread");
        assert_other_thread_acquires(name);
    }

    #[test]
    fn native_abandoned_mutex_is_released_and_rejected() {
        let name = private_test_name("Abandoned");
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (abandon_tx, abandon_rx) = std::sync::mpsc::channel();
        let worker_name = name.clone();
        let worker = std::thread::spawn(move || {
            let guard = acquire_named(&worker_name).expect("worker acquisition");
            held_tx.send(()).expect("signal ownership");
            abandon_rx.recv().expect("abandon signal");
            std::mem::forget(guard);
        });
        held_rx.recv().expect("worker owns mutex");
        let _pin = pin_object(&name);
        abandon_tx.send(()).expect("allow abandonment");
        worker.join().expect("worker exit");
        assert_eq!(acquire_named_wait(&name, 500).err(), Some("storage_error"));
        assert_other_thread_acquires(name);
    }

    #[test]
    fn native_mismatching_existing_descriptor_is_rejected() {
        let name = private_test_name("BadDescriptor");
        let token = effective_token().expect("effective token");
        let (_sid_backing, sid) = unsafe { user_sid(token.0.0).expect("user SID") };
        let (_acl_storage, sd) =
            unsafe { descriptor(sid, MUTEX_ALL_ACCESS | 0x100).expect("test security descriptor") };
        let mut attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: (&sd as *const SECURITY_DESCRIPTOR).cast_mut().cast(),
            bInheritHandle: 0,
        };
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let existing = unsafe { CreateMutexExW(&mut attrs, wide.as_ptr(), 0, HANDLE_ACCESS) };
        assert!(!existing.is_null());
        let _existing = Handle(existing);
        assert_eq!(acquire_named(&name).err(), Some("storage_error"));
    }
}
