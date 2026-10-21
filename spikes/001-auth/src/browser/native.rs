use super::{
    LaunchState, assigned_to_job, profile_cleanup_allowed, quote_windows_argument, resumed,
    select_executable,
};
use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use tempfile::TempDir;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::Com::CoTaskMemFree,
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Threading::{
            CREATE_SUSPENDED, CreateProcessW, PROCESS_INFORMATION, ResumeThread, STARTUPINFOW,
            TerminateProcess, WaitForSingleObject,
        },
    },
    UI::Shell::{
        FOLDERID_LocalAppData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86,
        SHGetKnownFolderPath,
    },
};

struct Handle(HANDLE);
impl Handle {
    fn valid(&self) -> bool {
        !self.0.is_null()
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        if self.valid() {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

pub(crate) struct Browser {
    process: Handle,
    job: Handle,
    profile: Option<TempDir>,
    terminated: bool,
}

impl Browser {
    pub(crate) fn spawn(
        executable: PathBuf,
        args: &[String],
        profile: TempDir,
    ) -> Result<Self, &'static str> {
        let job = Handle(unsafe { CreateJobObjectW(null(), null()) });
        if !job.valid() {
            return Err("browser_launch_failed");
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err("browser_launch_failed");
        }

        let app = wide(executable.as_os_str());
        let mut command = quote_windows_argument(&executable.to_string_lossy());
        for arg in args {
            command.push(' ');
            command.push_str(&quote_windows_argument(arg));
        }
        let mut command: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
        let mut startup = STARTUPINFOW::default();
        startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut info = PROCESS_INFORMATION::default();
        let created = unsafe {
            CreateProcessW(
                app.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                0,
                CREATE_SUSPENDED,
                null(),
                null(),
                &startup,
                &mut info,
            )
        };
        if created == 0 {
            return Err("browser_launch_failed");
        }
        let process = Handle(info.hProcess);
        let thread = Handle(info.hThread);
        if unsafe { AssignProcessToJobObject(job.0, process.0) } == 0 {
            if !terminate_child(process.0) {
                // Preserve the profile if teardown is unconfirmed, while
                // closing the process/thread/job handles through RAII.
                std::mem::forget(profile);
                return Err("browser_cleanup_failed");
            }
            return Err("browser_launch_failed");
        }
        let Some(launch_state) = assigned_to_job(LaunchState::Suspended) else {
            if !terminate_child(process.0) {
                std::mem::forget(profile);
                return Err("browser_cleanup_failed");
            }
            return Err("browser_launch_failed");
        };
        if resumed(launch_state) != Some(LaunchState::Running) {
            if !terminate_child(process.0) {
                std::mem::forget(profile);
                return Err("browser_cleanup_failed");
            }
            return Err("browser_launch_failed");
        }
        if unsafe { ResumeThread(thread.0) } == u32::MAX {
            unsafe {
                TerminateJobObject(job.0, 1);
            }
            if !terminate_child(process.0) {
                // Dropping the job handle retains kill-on-close as fallback.
                std::mem::forget(profile);
                return Err("browser_cleanup_failed");
            }
            return Err("browser_launch_failed");
        }
        drop(thread);
        Ok(Self {
            process,
            job,
            profile: Some(profile),
            terminated: false,
        })
    }

    pub(crate) fn try_wait(&mut self) -> Result<Option<()>, &'static str> {
        match unsafe { WaitForSingleObject(self.process.0, 0) } {
            WAIT_OBJECT_0 => Ok(Some(())),
            WAIT_TIMEOUT => Ok(None),
            _ => Err("browser_launch_failed"),
        }
    }

    pub(crate) fn cleanup(&mut self) -> Result<(), &'static str> {
        if self.terminated {
            return Ok(());
        }
        let active = active_processes(self.job.0).ok_or("browser_cleanup_failed")?;
        if active != 0 && unsafe { TerminateJobObject(self.job.0, 1) } == 0 {
            return Err("browser_cleanup_failed");
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        let root_done = unsafe { WaitForSingleObject(self.process.0, 2_000) } == WAIT_OBJECT_0;
        loop {
            let termination_confirmed = root_done && active_processes(self.job.0) == Some(0);
            if profile_cleanup_allowed(termination_confirmed) {
                self.terminated = true;
                self.profile.take();
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("browser_cleanup_failed");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn active_processes(job: HANDLE) -> Option<u32> {
    let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    (unsafe {
        QueryInformationJobObject(
            job,
            JobObjectBasicAccountingInformation,
            &mut accounting as *mut _ as *mut _,
            std::mem::size_of_val(&accounting) as u32,
            null_mut(),
        )
    } != 0)
        .then_some(accounting.ActiveProcesses)
}

impl Drop for Browser {
    fn drop(&mut self) {
        if !self.terminated && self.cleanup().is_err() {
            // Keep the isolated profile if tree termination could not be confirmed.
            if let Some(profile) = self.profile.take() {
                std::mem::forget(profile);
            }
        }
    }
}

fn terminate_child(process: HANDLE) -> bool {
    unsafe {
        TerminateProcess(process, 1);
    }
    unsafe { WaitForSingleObject(process, 2_000) == WAIT_OBJECT_0 }
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

fn known_folder(id: &windows_sys::core::GUID) -> Option<PathBuf> {
    let mut ptr = null_mut();
    if unsafe { SHGetKnownFolderPath(id, 0, null_mut(), &mut ptr) } < 0 || ptr.is_null() {
        return None;
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
    }
    let value = unsafe { String::from_utf16(std::slice::from_raw_parts(ptr, len)).ok() };
    unsafe {
        CoTaskMemFree(ptr.cast());
    }
    value.map(PathBuf::from)
}

pub(crate) fn resolve_executable() -> Result<PathBuf, &'static str> {
    let explicit = std::env::var_os("BINUS_CHROMIUM");
    if let Some(value) = explicit {
        let path = PathBuf::from(value);
        return select_executable(Some(&path), &[]).ok_or("browser_launch_failed");
    }
    let roots = [
        known_folder(&FOLDERID_ProgramFilesX86),
        known_folder(&FOLDERID_ProgramFiles),
        known_folder(&FOLDERID_LocalAppData),
    ];
    let mut candidates = Vec::new();
    for root in roots.iter().flatten() {
        candidates.push(root.join("Microsoft\\Edge\\Application\\msedge.exe"));
    }
    for root in roots.iter().flatten() {
        candidates.push(root.join("Google\\Chrome\\Application\\chrome.exe"));
    }
    select_executable(None, &candidates).ok_or("browser_launch_failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        process::{Child, Command},
        sync::Mutex,
    };
    use windows_sys::Win32::System::{
        JobObjects::IsProcessInJob,
        Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE},
    };

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }

    fn fixture_path() -> PathBuf {
        PathBuf::from(std::env::var_os("BINUS_SUPERVISOR_TEST_FIXTURE").unwrap())
    }

    // The subprocess reports both process IDs only after the descendant exists.
    #[test]
    fn helper_process_waits() {
        let Some(parent) = std::env::var("BINUS_SUPERVISOR_TEST_PARENT_PID")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
        else {
            return;
        };
        if parent == std::process::id() {
            return;
        }

        let fixture = fixture_path();
        let level = std::env::var("BINUS_SUPERVISOR_TEST_LEVEL").unwrap_or_default();
        if level.is_empty() {
            let child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "browser::native::tests::helper_process_waits",
                    "--nocapture",
                ])
                .env("BINUS_SUPERVISOR_TEST_LEVEL", "descendant")
                .spawn()
                .unwrap();
            std::fs::write(fixture.join("root.pid"), std::process::id().to_string()).unwrap();
            drop(child);
        } else if level == "descendant" {
            std::fs::write(
                fixture.join("descendant.pid"),
                std::process::id().to_string(),
            )
            .unwrap();
        }
        loop {
            std::thread::park_timeout(Duration::from_secs(1));
        }
    }

    fn wait_for_tree(fixture: &std::path::Path) -> (u32, u32) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let (Ok(root), Ok(descendant)) = (
                std::fs::read_to_string(fixture.join("root.pid")),
                std::fs::read_to_string(fixture.join("descendant.pid")),
            ) {
                return (root.parse().unwrap(), descendant.parse().unwrap());
            }
            assert!(
                Instant::now() < deadline,
                "helper tree readiness deadline elapsed"
            );
            std::thread::park_timeout(Duration::from_millis(10));
        }
    }

    fn open_member(job: HANDLE, pid: u32) -> Handle {
        let process = Handle(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        });
        assert!(
            process.valid(),
            "reported helper process could not be opened"
        );
        let mut member = 0;
        assert_ne!(unsafe { IsProcessInJob(process.0, job, &mut member) }, 0);
        assert_ne!(member, 0, "reported descendant was outside the owned job");
        process
    }

    fn assert_exited(process: &Handle) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if unsafe { WaitForSingleObject(process.0, 0) } == WAIT_OBJECT_0 {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "owned helper process remained alive"
            );
            std::thread::park_timeout(Duration::from_millis(10));
        }
    }

    fn spawn_tree(fixture: &std::path::Path) -> Browser {
        let executable = std::env::current_exe().unwrap();
        let args = [
            "--exact",
            "browser::native::tests::helper_process_waits",
            "--nocapture",
        ]
        .map(str::to_owned);
        let profile = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var(
                "BINUS_SUPERVISOR_TEST_PARENT_PID",
                std::process::id().to_string(),
            );
            std::env::set_var("BINUS_SUPERVISOR_TEST_FIXTURE", fixture);
        }
        let browser = Browser::spawn(executable, &args, profile).unwrap();
        let (root, descendant) = wait_for_tree(fixture);
        let _root_process = open_member(browser.job.0, root);
        let _descendant_process = open_member(browser.job.0, descendant);
        unsafe {
            std::env::remove_var("BINUS_SUPERVISOR_TEST_PARENT_PID");
            std::env::remove_var("BINUS_SUPERVISOR_TEST_FIXTURE");
        }
        browser
    }

    #[test]
    fn job_shutdown_reaps_confirmed_tree_and_drop_cleans_up() {
        let _lock = ENV_LOCK.lock().unwrap();
        let fixture = tempfile::tempdir().unwrap();
        let mut browser = spawn_tree(fixture.path());
        let root: u32 = std::fs::read_to_string(fixture.path().join("root.pid"))
            .unwrap()
            .parse()
            .unwrap();
        let descendant: u32 = std::fs::read_to_string(fixture.path().join("descendant.pid"))
            .unwrap()
            .parse()
            .unwrap();
        let root_process = open_member(browser.job.0, root);
        let descendant_process = open_member(browser.job.0, descendant);
        let profile_path = browser.profile.as_ref().unwrap().path().to_path_buf();

        let mut unrelated = ChildGuard(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "browser::native::tests::helper_process_waits",
                    "--nocapture",
                ])
                .env(
                    "BINUS_SUPERVISOR_TEST_PARENT_PID",
                    std::process::id().to_string(),
                )
                .env("BINUS_SUPERVISOR_TEST_FIXTURE", fixture.path())
                .env("BINUS_SUPERVISOR_TEST_LEVEL", "unrelated")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        assert!(browser.cleanup().is_ok());
        assert_exited(&root_process);
        assert_exited(&descendant_process);
        assert!(
            !profile_path.exists(),
            "confirmed teardown must remove the profile"
        );
        assert!(unrelated.0.try_wait().unwrap().is_none());
        unrelated.0.kill().unwrap();
        unrelated.0.wait().unwrap();

        let fixture2 = tempfile::tempdir().unwrap();
        let browser = spawn_tree(fixture2.path());
        let root: u32 = std::fs::read_to_string(fixture2.path().join("root.pid"))
            .unwrap()
            .parse()
            .unwrap();
        let descendant: u32 = std::fs::read_to_string(fixture2.path().join("descendant.pid"))
            .unwrap()
            .parse()
            .unwrap();
        let root_process = open_member(browser.job.0, root);
        let descendant_process = open_member(browser.job.0, descendant);
        let profile_path = browser.profile.as_ref().unwrap().path().to_path_buf();
        drop(browser);
        assert_exited(&root_process);
        assert_exited(&descendant_process);
        assert!(
            !profile_path.exists(),
            "Drop teardown must remove the confirmed tree profile"
        );
    }
}
