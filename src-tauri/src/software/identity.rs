use std::path::{Path, PathBuf};
use windows::{
    core::{HRESULT, PWSTR},
    Win32::{
        Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, FILETIME, HANDLE, HWND, LPARAM, WAIT_OBJECT_0,
            WAIT_TIMEOUT,
        },
        System::Threading::{
            GetCurrentProcessId, GetProcessTimes, OpenProcess, QueryFullProcessImageNameW,
            WaitForSingleObject, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_SYNCHRONIZE,
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindow, GetWindowLongPtrW, GetWindowThreadProcessId,
            IsIconic, IsWindow, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_TOOLWINDOW,
        },
    },
};
use yilian_backend::software::{SoftwareError, WindowCandidate, WindowIdentity};

pub(super) fn error(code: &str, value: impl ToString) -> SoftwareError {
    SoftwareError::new(code, value.to_string())
}
pub(super) fn handle(target: &WindowIdentity) -> Result<HWND, SoftwareError> {
    let raw = target
        .hwnd
        .parse::<usize>()
        .map_err(|_| error("invalid_window", "Invalid window identity"))?;
    if raw == 0 {
        return Err(error("invalid_window", "Window handle must not be zero"));
    }
    Ok(HWND(raw as *mut _))
}
struct ProcessHandle(HANDLE);
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(super) fn process_alive(target: &WindowIdentity) -> Result<bool, SoftwareError> {
    unsafe {
        let process = match OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            target.pid,
        ) {
            Ok(handle) => ProcessHandle(handle),
            Err(error) if error.code() == HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) => {
                return Ok(false)
            }
            Err(cause) => return Err(error("process_access_denied", cause)),
        };
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user)
            .map_err(|cause| error("identity_unavailable", cause))?;
        let started = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        if started != target.process_started_at {
            return Ok(false);
        }
        match WaitForSingleObject(process.0, 0) {
            WAIT_OBJECT_0 => Ok(false),
            WAIT_TIMEOUT => Ok(true),
            _ => Err(error(
                "process_status_unavailable",
                windows::core::Error::from_win32(),
            )),
        }
    }
}
pub(super) fn canonical_executable(path: &str) -> Result<PathBuf, SoftwareError> {
    let path = Path::new(path);
    if !path.is_absolute() || !path.is_file() {
        return Err(error(
            "invalid_executable",
            "Executable must be an existing absolute file",
        ));
    }
    path.canonicalize()
        .map_err(|e| error("executable_unavailable", e))
}
pub(super) fn read_identity(hwnd: HWND) -> Result<WindowIdentity, SoftwareError> {
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return Err(error(
                "window_closed",
                "The selected window no longer exists",
            ));
        }
        let mut pid = 0;
        if GetWindowThreadProcessId(hwnd, Some(&mut pid)) == 0 || pid == 0 {
            return Err(error(
                "identity_unavailable",
                "Cannot resolve target process",
            ));
        }
        if pid == GetCurrentProcessId() {
            return Err(error(
                "software_host_target_forbidden",
                "The assistant's own process cannot be a software automation target",
            ));
        }
        let process = ProcessHandle(
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                .map_err(|e| error("process_access_denied", e))?,
        );
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user)
            .map_err(|e| error("identity_unavailable", e))?;
        let mut path = vec![0u16; 32768];
        let mut len = path.len() as u32;
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut len,
        )
        .map_err(|e| error("identity_unavailable", e))?;
        let path = String::from_utf16_lossy(&path[..len as usize]);
        let executable = canonical_executable(&path)?.to_string_lossy().to_string();
        let mut class = [0u16; 512];
        let len = GetClassNameW(hwnd, &mut class);
        if len == 0 {
            return Err(error("identity_unavailable", "Cannot resolve window class"));
        }
        // Re-read PID after queries: never combine identities from two recycled windows.
        let mut current_pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut current_pid));
        if current_pid != pid {
            return Err(error(
                "target_changed",
                "Window process changed during observation",
            ));
        }
        Ok(WindowIdentity {
            hwnd: (hwnd.0 as usize).to_string(),
            pid,
            process_started_at: ((created.dwHighDateTime as u64) << 32)
                | created.dwLowDateTime as u64,
            executable,
            class_name: String::from_utf16_lossy(&class[..len as usize]),
        })
    }
}
pub(super) fn verify(target: &WindowIdentity) -> Result<HWND, SoftwareError> {
    let hwnd = handle(target)?;
    if &read_identity(hwnd)? != target {
        return Err(error(
            "target_changed",
            "Window/process identity changed; select the target again",
        ));
    }
    Ok(hwnd)
}
pub(super) fn candidate(hwnd: HWND) -> Result<WindowCandidate, SoftwareError> {
    Ok(WindowCandidate {
        identity: read_identity(hwnd)?,
        visible: unsafe { IsWindowVisible(hwnd).as_bool() },
        minimized: unsafe { IsIconic(hwnd).as_bool() },
    })
}
struct Enumeration {
    executable: PathBuf,
    wechat: bool,
    windows: Vec<(WindowCandidate, u64)>,
}
unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
    let data = &mut *(data.0 as *mut Enumeration);
    if let Ok(window) = candidate(hwnd) {
        if matches!(
            window.identity.class_name.as_str(),
            "IME" | "MSCTFIME UI" | "Default IME"
        ) {
            return true.into();
        }
        let mut rectangle = windows::Win32::Foundation::RECT::default();
        if windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rectangle).is_err()
            || rectangle.right <= rectangle.left
            || rectangle.bottom <= rectangle.top
        {
            return true.into();
        }
        if Path::new(&window.identity.executable) == data.executable && data.windows.len() < 128 {
            if data.wechat
                && (GetWindow(hwnd, GW_OWNER).is_ok()
                    || GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW.0 as isize != 0
                    || !is_wechat_window_class(&window.identity.class_name))
            {
                return true.into();
            }
            let width = i64::from(rectangle.right) - i64::from(rectangle.left);
            let height = i64::from(rectangle.bottom) - i64::from(rectangle.top);
            data.windows.push((window, width as u64 * height as u64));
        }
    }
    true.into()
}
fn is_wechat_window_class(class: &str) -> bool {
    matches!(class, "WeChatMainWndForPC" | "WeChatLoginWndForPC")
        || (class.starts_with("Qt") && class.ends_with("QWindowIcon"))
}

fn select_wechat_windows(mut windows: Vec<(WindowCandidate, u64)>) -> Vec<(WindowCandidate, u64)> {
    if windows
        .iter()
        .any(|(window, _)| window.visible || window.minimized)
    {
        windows.retain(|(window, _)| window.visible || window.minimized);
    }
    // A Qt process can own both the main surface and a small auxiliary surface.
    // Keep equally plausible windows, including windows from distinct instances.
    if let Some((first, _)) = windows.first() {
        if windows
            .iter()
            .all(|(window, _)| window.identity.pid == first.identity.pid)
        {
            let largest = windows.iter().map(|(_, area)| *area).max().unwrap_or(0);
            windows.retain(|(_, area)| *area == largest);
        }
    }
    windows
}

pub(super) fn discover(
    executable: &str,
    wechat: bool,
) -> Result<Vec<WindowCandidate>, SoftwareError> {
    let mut data = Enumeration {
        executable: canonical_executable(executable)?,
        wechat,
        windows: vec![],
    };
    unsafe {
        EnumWindows(Some(collect), LPARAM(&mut data as *mut _ as isize))
            .map_err(|e| error("window_discovery_failed", e))?;
    }
    let mut windows = if wechat {
        select_wechat_windows(data.windows)
    } else {
        data.windows
    };
    windows.sort_by(|(a, _), (b, _)| a.identity.hwnd.cmp(&b.identity.hwnd));
    Ok(windows.into_iter().map(|(window, _)| window).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wechat_prefers_its_main_surface_but_keeps_distinct_visible_instances() {
        let window = |hwnd: &str, pid, visible, area| {
            (
                WindowCandidate {
                    identity: WindowIdentity {
                        hwnd: hwnd.into(),
                        pid,
                        process_started_at: 1,
                        executable: "C:/Weixin/Weixin.exe".into(),
                        class_name: "Qt51514QWindowIcon".into(),
                    },
                    visible,
                    minimized: false,
                },
                area,
            )
        };
        assert!(is_wechat_window_class("Qt51514QWindowIcon"));
        assert!(!is_wechat_window_class(
            "Qt51514WxTrayIconMessageWindowClass"
        ));
        assert!(!is_wechat_window_class("Chrome_SystemMessageWindow"));
        let main = window("1", 10, false, 800_000);
        let helper = window("2", 10, false, 50_000);
        assert_eq!(
            select_wechat_windows(vec![helper, main.clone()]),
            vec![main]
        );
        assert_eq!(
            select_wechat_windows(vec![
                window("1", 10, true, 800_000),
                window("3", 11, true, 600_000),
            ])
            .len(),
            2
        );
    }
}
