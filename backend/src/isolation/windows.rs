// ============================================================
// Windows OS-assisted process isolation — privilege-reduced token + Job Object.
//
// Security model (honest boundaries):
//   - PrivilegeOnly Restricted Token (CreateRestrictedToken + DISABLE_MAX_PRIVILEGE,
//     RestrictedSidCount = 0) reduces the child's privilege surface. This is NOT
//     a restricting-SID filesystem ACL sandbox, and it does NOT provide a
//     filesystem or network namespace.
//   - A Job Object (KILL_ON_JOB_CLOSE + ACTIVE_PROCESS limit) provides
//     process-tree lifecycle containment; TerminateJobObject kills the whole tree
//     on timeout/cancel. This is not full container isolation.
//
// HARD GATE: child privilege surface < parent (verified via TokenPrivileges),
// NOT IsTokenRestricted == TRUE (which only reflects a restricting-SID list and
// is intentionally FALSE here). There is NO unrestricted fallback: any backend
// failure returns an isolation error.
// ============================================================

#![cfg(windows)]

use std::collections::BTreeMap;
use std::os::windows::ffi::OsStrExt;
use std::sync::Arc;

use windows_sys::Win32::Foundation::{
    CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
    WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::{
    CreateRestrictedToken, GetTokenInformation, TokenPrivileges, SECURITY_ATTRIBUTES,
    TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_QUERY,
};
use windows_sys::Win32::Storage::FileSystem::ReadFile;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_BASIC_LIMIT_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CreateProcessAsUserW, GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, ResumeThread,
    WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
    PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW,
};

const MAX_ACTIVE_PROCESSES: u32 = 32;
const MAX_CAPTURE_CHARS: usize = 20_000;

/// Non-Copy RAII wrapper over a raw HANDLE. Closes on drop.
struct WinHandle(HANDLE);

impl WinHandle {
    unsafe fn new(handle: HANDLE) -> Option<Self> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            None
        } else {
            Some(WinHandle(handle))
        }
    }
    fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for WinHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

// A raw HANDLE is an opaque kernel-object identifier owned by the process, not
// thread-affine; moving it across threads and closing it from another thread is
// safe.
unsafe impl Send for WinHandle {}
unsafe impl Sync for WinHandle {}

pub struct WindowsChild {
    process: Arc<WinHandle>,
    // Held for RAII cleanup (CloseHandle on drop).
    #[allow(dead_code)]
    thread: WinHandle,
    job: Arc<WinHandle>,
    pid: u32,
    stdout_reader: Option<WinHandle>,
    stderr_reader: Option<WinHandle>,
}

struct WindowsProcessControl {
    process: Arc<WinHandle>,
    job: Arc<WinHandle>,
}

impl crate::isolation::ManagedProcessControl for WindowsProcessControl {
    fn is_alive(&self) -> bool {
        unsafe { WaitForSingleObject(self.process.raw(), 0) != WAIT_OBJECT_0 }
    }

    fn terminate(&self) -> Result<(), String> {
        unsafe {
            if TerminateJobObject(self.job.raw(), 1) == 0 {
                Err(format!(
                    "TerminateJobObject failed (GetLastError={})",
                    std::io::Error::last_os_error()
                ))
            } else {
                Ok(())
            }
        }
    }
}

impl WindowsChild {
    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn has_exited(&self) -> bool {
        unsafe { WaitForSingleObject(self.process.raw(), 0) == WAIT_OBJECT_0 }
    }
    pub fn wait_blocking(&self) {
        unsafe {
            WaitForSingleObject(
                self.process.raw(),
                windows_sys::Win32::System::Threading::INFINITE,
            );
        }
    }
    pub fn terminate_tree(&self) {
        unsafe {
            TerminateJobObject(self.job.raw(), 1);
        }
    }
    pub fn exit_code(&self) -> i32 {
        let mut code = 0u32;
        unsafe {
            GetExitCodeProcess(self.process.raw(), &mut code);
        }
        code as i32
    }
    fn take_stdout_reader(&mut self) -> Option<WinHandle> {
        self.stdout_reader.take()
    }
    fn take_stderr_reader(&mut self) -> Option<WinHandle> {
        self.stderr_reader.take()
    }
}

/// Drain a pipe read handle to EOF, bounded capture (still drains to avoid
/// child pipe-buffer deadlock).
fn read_pipe(handle: WinHandle) -> String {
    let mut out: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let mut read = 0u32;
        let ok = unsafe {
            ReadFile(
                handle.raw(),
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || read == 0 {
            break;
        }
        if out.len() < MAX_CAPTURE_CHARS {
            let remaining = MAX_CAPTURE_CHARS - out.len();
            let take = (read as usize).min(remaining);
            out.extend_from_slice(&buf[..take]);
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Privilege summary: (total, enabled) counts from TokenPrivileges.
pub fn privilege_summary(token: HANDLE) -> (u32, u32) {
    unsafe {
        let mut size = 0u32;
        GetTokenInformation(token, TokenPrivileges, std::ptr::null_mut(), 0, &mut size);
        if size == 0 {
            return (0, 0);
        }
        let mut buf = vec![0u8; size as usize];
        if GetTokenInformation(
            token,
            TokenPrivileges,
            buf.as_mut_ptr() as *mut core::ffi::c_void,
            size,
            &mut size,
        ) == 0
        {
            return (0, 0);
        }
        let tp = buf.as_ptr() as *const windows_sys::Win32::Security::TOKEN_PRIVILEGES;
        let total = (*tp).PrivilegeCount;
        let first = (*tp).Privileges.as_ptr();
        let mut enabled = 0u32;
        for i in 0..total as usize {
            if (*first.add(i)).Attributes & 0x2 != 0 {
                enabled += 1;
            }
        }
        (total, enabled)
    }
}

fn is_strict_privilege_reduction(parent_total: u32, child_total: u32) -> bool {
    child_total < parent_total
}

fn check_non_inheritable_handle_result(result: i32) -> Result<(), String> {
    if result == 0 {
        Err("SetHandleInformation failed".to_string())
    } else {
        Ok(())
    }
}

/// Create a privilege-reduced token (DISABLE_MAX_PRIVILEGE, no restricting SIDs)
/// and verify the privilege surface is strictly reduced. Returns (token, parent_total, child_total).
fn create_privilege_reduced_token() -> Result<(WinHandle, u32, u32), String> {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY,
            &mut token,
        ) == 0
        {
            return Err("OpenProcessToken failed".to_string());
        }
        let original = WinHandle::new(token).ok_or("invalid process token")?;
        let (parent_total, _) = privilege_summary(original.raw());

        let mut restricted = HANDLE::default();
        let ok = CreateRestrictedToken(
            original.raw(),
            windows_sys::Win32::Security::DISABLE_MAX_PRIVILEGE,
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            &mut restricted,
        );
        if ok == 0 {
            return Err("CreateRestrictedToken failed".to_string());
        }
        let restricted_handle = WinHandle::new(restricted).ok_or("invalid restricted token")?;
        let (child_total, _) = privilege_summary(restricted_handle.raw());

        // HARD GATE: the child privilege surface must be strictly smaller.
        if !is_strict_privilege_reduction(parent_total, child_total) {
            return Err(format!(
                "privilege reduction not verified: parent={parent_total} child={child_total}"
            ));
        }
        Ok((restricted_handle, parent_total, child_total))
    }
}

fn create_job() -> Result<WinHandle, String> {
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        let job_handle = WinHandle::new(job).ok_or("CreateJobObjectW failed")?;
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation = JOBOBJECT_BASIC_LIMIT_INFORMATION {
            PerProcessUserTimeLimit: 0,
            PerJobUserTimeLimit: 0,
            LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
            MinimumWorkingSetSize: 0,
            MaximumWorkingSetSize: 0,
            ActiveProcessLimit: MAX_ACTIVE_PROCESSES,
            Affinity: 0,
            PriorityClass: 0,
            SchedulingClass: 0,
        };
        if SetInformationJobObject(
            job_handle.raw(),
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            return Err("SetInformationJobObject failed".to_string());
        }
        Ok(job_handle)
    }
}

fn create_pipe_pair() -> Result<(WinHandle, WinHandle), String> {
    unsafe {
        let mut read = HANDLE::default();
        let mut write = HANDLE::default();
        let attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };
        if CreatePipe(&mut read, &mut write, &attrs, 0) == 0 {
            return Err("CreatePipe failed".to_string());
        }
        check_non_inheritable_handle_result(SetHandleInformation(read, HANDLE_FLAG_INHERIT, 0))?;
        let read_handle = WinHandle::new(read).ok_or("invalid read pipe")?;
        let write_handle = WinHandle::new(write).ok_or("invalid write pipe")?;
        Ok((read_handle, write_handle))
    }
}

fn build_env_block(env: &BTreeMap<String, String>) -> Vec<u16> {
    let mut block: Vec<u16> = Vec::new();
    for (k, v) in env {
        for unit in format!("{k}={v}").encode_utf16() {
            block.push(unit);
        }
        block.push(0);
    }
    block.push(0);
    block
}

fn build_command_line(program: &str, args: &[&str]) -> Vec<u16> {
    let mut line = format!("\"{}\"", program);
    for arg in args {
        line.push(' ');
        line.push('"');
        line.push_str(&arg.replace('"', "\\\""));
        line.push('"');
    }
    std::ffi::OsStr::new(&line)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Windows production managed-process runner: spawn under a privilege-reduced
/// token + Job Object, drain pipes concurrently (no deadlock), wait with a real
/// timeout, and TerminateJobObject on timeout. No unrestricted fallback.
pub async fn run_windows_managed_process(
    program: &str,
    args: &[&str],
    current_dir: &str,
    explicit_env: &BTreeMap<String, String>,
    timeout_ms: u64,
) -> crate::isolation::ProcessRunResult {
    let registry = Arc::new(crate::isolation::ManagedProcessRegistry::new());
    run_windows_managed_process_with_registry(
        program,
        args,
        current_dir,
        explicit_env,
        timeout_ms,
        &registry,
        None,
        program,
    )
    .await
}

pub async fn run_windows_managed_process_with_registry(
    program: &str,
    args: &[&str],
    current_dir: &str,
    explicit_env: &BTreeMap<String, String>,
    timeout_ms: u64,
    registry: &crate::isolation::SharedManagedProcessRegistry,
    tool_call_id: Option<String>,
    name: &str,
) -> crate::isolation::ProcessRunResult {
    use crate::isolation::ProcessRunResult;
    let env = crate::isolation::build_child_env(explicit_env);

    // Spawn synchronously: the Win32 setup calls are fast (non-blocking I/O).
    let mut child = match spawn_isolated(program, args, current_dir, &env) {
        Ok(c) => c,
        Err(e) => {
            return ProcessRunResult {
                exit_code: None,
                stdout: String::new(),
                stderr: format!("isolation error: {e}"),
                timed_out: false,
            }
        }
    };

    let pid = child.pid();
    registry.record_with_control(
        pid,
        tool_call_id,
        name.to_string(),
        Arc::new(WindowsProcessControl {
            process: Arc::clone(&child.process),
            job: Arc::clone(&child.job),
        }),
    );

    let stdout_reader = child.take_stdout_reader().unwrap();
    let stderr_reader = child.take_stderr_reader().unwrap();
    let stdout_task = tokio::task::spawn_blocking(move || read_pipe(stdout_reader));
    let stderr_task = tokio::task::spawn_blocking(move || read_pipe(stderr_reader));

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let mut timed_out = false;
    while !child.has_exited() {
        if std::time::Instant::now() >= deadline {
            child.terminate_tree();
            timed_out = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    let exit_code = tokio::task::spawn_blocking(move || {
        child.wait_blocking();
        child.exit_code()
    })
    .await
    .unwrap_or(-1);

    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();

    let result = ProcessRunResult {
        exit_code: if timed_out { None } else { Some(exit_code) },
        stdout,
        stderr,
        timed_out,
    };
    registry.remove_pid(pid);
    result
}

pub fn spawn_isolated(
    program: &str,
    args: &[&str],
    current_dir: &str,
    env: &BTreeMap<String, String>,
) -> Result<WindowsChild, String> {
    let (token, _parent, _child) = create_privilege_reduced_token()?;
    let job = create_job()?;
    let (stdout_reader, stdout_writer) = create_pipe_pair()?;
    let (stderr_reader, stderr_writer) = create_pipe_pair()?;

    unsafe {
        let mut command_line = build_command_line(program, args);
        let env_block = build_env_block(env);
        let cwd: Vec<u16> = std::ffi::OsStr::new(current_dir)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut startup: STARTUPINFOW = std::mem::zeroed();
        startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        startup.dwFlags = STARTF_USESTDHANDLES;
        startup.hStdInput = INVALID_HANDLE_VALUE;
        startup.hStdOutput = stdout_writer.raw();
        startup.hStdError = stderr_writer.raw();

        let mut process_info: PROCESS_INFORMATION = std::mem::zeroed();
        let ok = CreateProcessAsUserW(
            token.raw(),
            std::ptr::null(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
            env_block.as_ptr() as *const core::ffi::c_void,
            cwd.as_ptr(),
            &startup,
            &mut process_info,
        );
        if ok == 0 {
            return Err(format!(
                "CreateProcessAsUserW failed (GetLastError={})",
                std::io::Error::last_os_error()
            ));
        }
        let process =
            Arc::new(WinHandle::new(process_info.hProcess).ok_or("invalid process handle")?);
        let thread = WinHandle::new(process_info.hThread).ok_or("invalid thread handle")?;

        // Drop the parent's child-write copies so reads can see EOF on exit.
        drop(stdout_writer);
        drop(stderr_writer);

        // HARD GATE: assign to Job BEFORE resume.
        let job = Arc::new(job);
        if AssignProcessToJobObject(job.raw(), process.raw()) == 0 {
            TerminateJobObject(job.raw(), 1);
            return Err("AssignProcessToJobObject failed".to_string());
        }
        if ResumeThread(thread.raw()) == u32::MAX {
            TerminateJobObject(job.raw(), 1);
            return Err("ResumeThread failed".to_string());
        }

        Ok(WindowsChild {
            process,
            thread,
            job,
            pid: process_info.dwProcessId,
            stdout_reader: Some(stdout_reader),
            stderr_reader: Some(stderr_reader),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };

    fn current_exe() -> String {
        std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    fn helper_env() -> BTreeMap<String, String> {
        crate::isolation::build_child_env(&BTreeMap::new())
    }

    #[test]
    fn isolation_helper_child() {
        if std::env::var("YILIAN_ISOLATION_HELPER").as_deref() != Ok("1") {
            return;
        }
        // Report privilege summary + whether the helper ran.
        let (total, enabled) = unsafe {
            let mut token = HANDLE::default();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                (0, 0)
            } else {
                let s = privilege_summary(token);
                CloseHandle(token);
                s
            }
        };
        if let Ok(path) = std::env::var("YILIAN_RESULT_FILE") {
            let _ = std::fs::write(&path, format!("total={total} enabled={enabled}"));
        }
        if std::env::var("YILIAN_SPAWN_GRANDCHILD").as_deref() == Ok("1") {
            let grandchild = std::process::Command::new(current_exe())
                .args([
                    "--exact",
                    "isolation::windows::tests::grandchild_sleep",
                    "--nocapture",
                ])
                .env("YILIAN_GRANDCHILD", "1")
                .spawn()
                .expect("grandchild should start");
            if let Ok(path) = std::env::var("YILIAN_RESULT_FILE") {
                let _ = std::fs::write(
                    &path,
                    format!(
                        "parent_pid={}\ngrandchild_pid={}\n",
                        std::process::id(),
                        grandchild.id()
                    ),
                );
            }
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    }

    #[test]
    fn grandchild_sleep() {
        if std::env::var("YILIAN_GRANDCHILD").as_deref() != Ok("1") {
            return;
        }
        std::thread::sleep(std::time::Duration::from_secs(60));
    }

    #[test]
    fn privilege_reduction_verified() {
        let (token, parent, child) = create_privilege_reduced_token().unwrap();
        assert!(parent > 1, "expected a non-empty parent privilege set");
        assert!(
            is_strict_privilege_reduction(parent, child),
            "privilege surface not reduced: parent={parent} child={child}"
        );
        let _ = token;
    }

    #[test]
    fn privilege_reduction_rejects_equal_or_increased_surface() {
        assert!(!is_strict_privilege_reduction(1, 1));
        assert!(!is_strict_privilege_reduction(5, 5));
        assert!(!is_strict_privilege_reduction(5, 6));
        assert!(is_strict_privilege_reduction(5, 4));
    }

    #[test]
    fn non_inheritable_handle_failure_is_fail_closed() {
        assert!(check_non_inheritable_handle_result(1).is_ok());
        assert!(check_non_inheritable_handle_result(0).is_err());
    }

    #[test]
    fn powershell_launches_under_privilege_reduced_token() {
        let args: Vec<&str> = vec!["-NoProfile", "-NonInteractive", "-Command", "exit 0"];
        let child = spawn_isolated("powershell.exe", &args, ".", &helper_env()).unwrap();
        child.wait_blocking();
        assert_eq!(child.exit_code(), 0, "PowerShell did not exit 0");
    }

    #[test]
    fn job_object_terminates_descendant_tree() {
        let exe = current_exe();
        let result_file = std::env::temp_dir().join(format!(
            "yilian-isolation-grandchild-{}.txt",
            std::process::id()
        ));
        let args: Vec<&str> = vec![
            "--exact",
            "isolation::windows::tests::isolation_helper_child",
            "--nocapture",
        ];
        let mut env = helper_env();
        env.insert("YILIAN_ISOLATION_HELPER".to_string(), "1".to_string());
        env.insert("YILIAN_SPAWN_GRANDCHILD".to_string(), "1".to_string());
        env.insert(
            "YILIAN_RESULT_FILE".to_string(),
            result_file.to_string_lossy().to_string(),
        );
        let child = spawn_isolated(&exe, &args, ".", &env).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let grandchild_pid = loop {
            if let Ok(contents) = std::fs::read_to_string(&result_file) {
                if let Some(pid) = contents
                    .lines()
                    .find_map(|line| line.strip_prefix("grandchild_pid=")?.parse::<u32>().ok())
                {
                    break pid;
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "grandchild did not report a PID"
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        };
        assert!(process_is_alive(grandchild_pid), "grandchild was not live");
        child.terminate_tree();
        child.wait_blocking();
        assert!(
            child.has_exited(),
            "parent did not exit after job termination"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while process_is_alive(grandchild_pid) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(
            !process_is_alive(grandchild_pid),
            "grandchild survived Job termination"
        );
        let _ = std::fs::remove_file(result_file);
    }

    fn process_is_alive(pid: u32) -> bool {
        unsafe {
            let handle = OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            );
            let Some(handle) = WinHandle::new(handle) else {
                return false;
            };
            WaitForSingleObject(handle.raw(), 0) != WAIT_OBJECT_0
        }
    }

    #[tokio::test]
    async fn bash_timeout_terminates_job_and_returns_timed_out() {
        let args: Vec<&str> = vec![
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 30",
        ];
        let result =
            run_windows_managed_process("powershell.exe", &args, ".", &BTreeMap::new(), 1000).await;
        assert!(
            result.timed_out,
            "expected timeout; got exit_code={:?}",
            result.exit_code
        );
    }

    #[test]
    fn secret_env_not_inherited_by_child() {
        std::env::set_var("OPENAI_API_KEY", "PHASE6_SECRET_SENTINEL");
        let env = crate::isolation::build_child_env(&BTreeMap::new());
        std::env::remove_var("OPENAI_API_KEY");
        assert!(
            !env.contains_key("OPENAI_API_KEY"),
            "secret environment leaked into child env"
        );
    }
}
