use crate::plugin::service::ServiceLaunch;
use std::{collections::BTreeMap, ffi::OsStr, os::windows::ffi::OsStrExt, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
            JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
            TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Threading::{
            CreateProcessW, GetExitCodeProcess, ResumeThread, TerminateProcess,
            WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
            PROCESS_INFORMATION, STARTUPINFOW,
        },
    },
};

struct Handle(HANDLE);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub struct ServiceProcess {
    job: Handle,
    process: Handle,
    pid: u32,
}
fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain(Some(0)).collect()
}

// Windows command-line quoting, including quotes and trailing backslashes.
fn quote(arg: &str) -> String {
    let mut result = String::from("\"");
    let mut slashes = 0;
    for c in arg.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '"' {
            result.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
        } else {
            result.extend(std::iter::repeat_n('\\', slashes));
        }
        slashes = 0;
        result.push(c);
    }
    result.extend(std::iter::repeat_n('\\', slashes * 2));
    result.push('"');
    result
}
fn failure(operation: &str) -> String {
    format!("{operation}：{}", std::io::Error::last_os_error())
}

// Child services share the user's Windows account. Pass OS execution settings
// explicitly so the backend's provider and application secrets are not inherited.
fn service_environment(withhold: &[String]) -> Vec<u16> {
    let settings = std::env::vars_os()
        .filter_map(|(name, value)| {
            let key = name.to_string_lossy().to_ascii_uppercase();
            (!withhold
                .iter()
                .any(|secret| secret.eq_ignore_ascii_case(&key)))
            .then_some((key, (name, value)))
        })
        .collect::<BTreeMap<_, _>>();
    let mut block = Vec::new();
    for (_, (name, value)) in settings {
        block.extend(name.encode_wide());
        block.push(b'=' as u16);
        block.extend(value.encode_wide());
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

impl ServiceProcess {
    pub fn spawn(launch: &ServiceLaunch, withhold: &[String]) -> Result<Self, String> {
        unsafe {
            let raw_job = CreateJobObjectW(ptr::null(), ptr::null());
            if raw_job.is_null() {
                return Err(failure("创建服务进程组失败"));
            }
            let job = Handle(raw_job);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
            {
                return Err(failure("设置服务进程组失败"));
            }
            let mut startup: STARTUPINFOW = std::mem::zeroed();
            startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            let mut info: PROCESS_INFORMATION = std::mem::zeroed();
            let command = std::iter::once(launch.executable.as_str())
                .chain(launch.args.iter().map(String::as_str))
                .map(quote)
                .collect::<Vec<_>>()
                .join(" ");
            let environment = service_environment(withhold);
            if CreateProcessW(
                wide(&launch.executable).as_ptr(),
                wide(&command).as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                0,
                CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
                environment.as_ptr().cast(),
                wide(&launch.working_directory).as_ptr(),
                &startup,
                &mut info,
            ) == 0
            {
                return Err(failure("启动插件服务失败"));
            }
            let process = Handle(info.hProcess);
            let thread = Handle(info.hThread);
            if AssignProcessToJobObject(job.0, process.0) == 0 {
                let error = failure("关联服务进程组失败");
                TerminateProcess(process.0, 1);
                WaitForSingleObject(process.0, 2000);
                return Err(error);
            }
            if ResumeThread(thread.0) == u32::MAX {
                return Err(failure("恢复服务进程失败"));
            }
            Ok(Self {
                job,
                process,
                pid: info.dwProcessId,
            })
        }
    }
    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn exit_code(&self) -> Result<Option<u32>, String> {
        unsafe {
            match WaitForSingleObject(self.process.0, 0) {
                WAIT_TIMEOUT => Ok(None),
                WAIT_OBJECT_0 => {
                    let mut code = 0;
                    if GetExitCodeProcess(self.process.0, &mut code) == 0 {
                        return Err(failure("读取服务退出码失败"));
                    }
                    Ok(Some(code))
                }
                _ => Err(failure("读取服务进程状态失败")),
            }
        }
    }
    pub fn stop(&self) -> Result<(), String> {
        unsafe {
            if TerminateJobObject(self.job.0, 0) == 0 {
                return Err(failure("停止服务进程组失败"));
            }
            if WaitForSingleObject(self.process.0, 2000) != WAIT_OBJECT_0 {
                return Err("尚未确认服务进程停止".into());
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
                if QueryInformationJobObject(
                    self.job.0,
                    JobObjectBasicAccountingInformation,
                    &mut accounting as *mut _ as *mut _,
                    std::mem::size_of_val(&accounting) as u32,
                    ptr::null_mut(),
                ) == 0
                {
                    return Err(failure("读取服务进程组状态失败"));
                }
                if accounting.ActiveProcesses == 0 {
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    return Err("尚未确认服务子进程全部停止".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_argument_quoting_preserves_paths_and_quotes() {
        assert_eq!(quote("F:\\QQ files\\"), "\"F:\\QQ files\\\\\"");
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote(""), "\"\"");
    }
}
