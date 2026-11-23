//! Exercise the OS input path on the edit control in our own child process.
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::{HWND, LPARAM, POINT, RECT, WPARAM},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

unsafe fn center_of(hwnd: HWND) -> Result<POINT, String> {
    let mut rect = RECT::default();
    GetWindowRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
    Ok(POINT {
        x: (rect.left + rect.right) / 2,
        y: (rect.top + rect.bottom) / 2,
    })
}

unsafe fn click_owned_control(hwnd: HWND, ready: impl Fn() -> bool) -> Result<(), String> {
    let center = center_of(hwnd)?;
    let hit = WindowFromPoint(center);
    if hit != hwnd {
        let mut name = [0; 128];
        let len = GetClassNameW(hit, &mut name);
        return Err(format!(
            "Owned fixture window {hwnd:?} is covered at its center by {}",
            String::from_utf16_lossy(&name[..len as usize])
        ));
    }
    let mut previous = POINT::default();
    GetCursorPos(&mut previous).map_err(|e| e.to_string())?;
    SetCursorPos(center.x, center.y).map_err(|e| e.to_string())?;
    let clicks = [MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP].map(|flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    });
    let size = std::mem::size_of::<INPUT>() as i32;
    let sent = SendInput(&clicks, size);
    let deadline = Instant::now() + Duration::from_secs(1);
    while !ready() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut current = POINT::default();
    if GetCursorPos(&mut current).is_ok() && current == center {
        let _ = SetCursorPos(previous.x, previous.y);
    }
    if sent != 2 || !ready() {
        return Err("The click did not activate the owned fixture control".into());
    }
    Ok(())
}

pub(super) unsafe fn activate_fixture_host(host: HWND) -> Result<(), String> {
    if GetForegroundWindow() == host {
        return Ok(());
    }
    // Simulate the user first selecting the workbench. Restore its ordinary z-order
    // before testing any production docking or input behavior on the target.
    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
    SetWindowPos(host, Some(HWND_TOPMOST), 0, 0, 0, 0, flags).map_err(|e| e.to_string())?;
    let activated = click_owned_control(host, || GetForegroundWindow() == host);
    SetWindowPos(host, Some(HWND_NOTOPMOST), 0, 0, 0, 0, flags).map_err(|e| e.to_string())?;
    activated
}

pub(super) unsafe fn verify_edit(
    target: HWND,
    child_pid: u32,
    ch: u16,
    expected: &[u16],
) -> Result<(), String> {
    let edit = GetDlgItem(Some(target), 302).map_err(|e| e.to_string())?;
    let mut pid = 0;
    GetWindowThreadProcessId(edit, Some(&mut pid));
    if pid != child_pid {
        return Err("Edit no longer belongs to the fixture process".into());
    }
    click_owned_control(edit, || {
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(0, &mut gui).is_ok()
            && gui.hwndFocus == edit
            && GetForegroundWindow() == target
    })?;
    let size = std::mem::size_of::<INPUT>() as i32;
    let keys = [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP].map(|flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wScan: ch,
                dwFlags: flags,
                ..Default::default()
            },
        },
    });
    if SendInput(&keys, size) != 2 {
        return Err("Windows did not accept the fixture keyboard events".into());
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let mut text = [0u16; 4];
        let mut length = 0;
        let read = SendMessageTimeoutW(
            edit,
            WM_GETTEXT,
            WPARAM(text.len()),
            LPARAM(text.as_mut_ptr() as isize),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            500,
            Some(&mut length),
        );
        if read.0 != 0 && text.get(..length) == Some(expected) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("The mounted edit did not receive the typed character".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
