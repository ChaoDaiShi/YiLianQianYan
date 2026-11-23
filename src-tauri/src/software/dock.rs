//! Align the application's own top-level window without coupling its input queue to the host.
use super::{geometry::physical_viewport, identity::error};
use windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::ClientToScreen,
    UI::WindowsAndMessaging::*,
};
use yilian_backend::software::{SoftwareError, Viewport};

pub(super) fn align(
    host: HWND,
    hwnd: HWND,
    viewport: Viewport,
    reveal: bool,
) -> Result<RECT, SoftwareError> {
    let (x, y, width, height) = physical_viewport(host, viewport)?;
    let mut origin = POINT { x, y };
    unsafe {
        ClientToScreen(host, &mut origin)
            .ok()
            .map_err(|e| error("host_bounds_unavailable", e))?;
        let bounds = RECT {
            left: origin.x,
            top: origin.y,
            right: origin.x + width,
            bottom: origin.y + height,
        };
        let mut current = RECT::default();
        GetWindowRect(hwnd, &mut current).map_err(|e| error("window_state_unavailable", e))?;
        let mut flags = SWP_ASYNCWINDOWPOS | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
        if current.left == bounds.left && current.top == bounds.top {
            flags |= SWP_NOMOVE;
        }
        if current.right - current.left == width && current.bottom - current.top == height {
            flags |= SWP_NOSIZE;
        }
        if reveal && !IsWindowVisible(hwnd).as_bool() {
            flags |= SWP_SHOWWINDOW;
        }
        // Insert immediately above the host, without overtaking other applications or
        // promoting the target into the always-on-top band. Leave its popups above it.
        let previous = GetWindow(host, GW_HWNDPREV).unwrap_or_default();
        let insert_after = if previous == hwnd
            || GetForegroundWindow() == hwnd
            || GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize != 0
        {
            flags |= SWP_NOZORDER;
            None
        } else if previous.is_invalid()
            || GetWindowLongPtrW(previous, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize != 0
        {
            Some(HWND_TOP)
        } else {
            Some(previous)
        };
        if !flags.contains(SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER) || flags.contains(SWP_SHOWWINDOW)
        {
            SetWindowPos(hwnd, insert_after, origin.x, origin.y, width, height, flags)
                .map_err(|e| error("window_position_failed", e))?;
        }
        Ok(bounds)
    }
}

pub(super) fn at_bounds(hwnd: HWND, bounds: RECT) -> bool {
    let mut actual = RECT::default();
    unsafe {
        GetWindowRect(hwnd, &mut actual).is_ok()
            && actual == bounds
            && IsWindowVisible(hwnd).as_bool()
            && !IsIconic(hwnd).as_bool()
    }
}

pub(super) fn restore(
    hwnd: HWND,
    saved: &WINDOWPLACEMENT,
    visible: bool,
) -> Result<(), SoftwareError> {
    let mut placement = *saved;
    placement.flags |= WPF_ASYNCWINDOWPLACEMENT;
    if placement.showCmd == SW_SHOWNORMAL.0 as u32 {
        placement.showCmd = SW_SHOWNOACTIVATE.0 as u32;
    }
    unsafe {
        SetWindowPlacement(hwnd, &placement).map_err(|e| SoftwareError::unknown(e.to_string()))?;
        if !visible {
            SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_ASYNCWINDOWPOS
                    | SWP_HIDEWINDOW
                    | SWP_NOACTIVATE
                    | SWP_NOZORDER
                    | SWP_NOSIZE
                    | SWP_NOMOVE,
            )
            .map_err(|e| SoftwareError::unknown(e.to_string()))?;
        }
    }
    Ok(())
}

pub(super) fn restored(hwnd: HWND, saved: &WINDOWPLACEMENT, visible: bool) -> bool {
    let mut current = WINDOWPLACEMENT {
        length: saved.length,
        ..Default::default()
    };
    unsafe {
        GetWindowPlacement(hwnd, &mut current).is_ok()
            && current.rcNormalPosition == saved.rcNormalPosition
            && current.showCmd == saved.showCmd
            && IsWindowVisible(hwnd).as_bool() == visible
    }
}
