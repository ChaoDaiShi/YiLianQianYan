use super::identity::error;
use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*},
};
use yilian_backend::software::{SoftwareError, Viewport};

pub(super) fn physical_viewport(
    parent: HWND,
    viewport: Viewport,
) -> Result<(i32, i32, i32, i32), SoftwareError> {
    let scale = unsafe { GetDpiForWindow(parent) }.max(96) as f64 / 96.0;
    let (x, y, w, h) = (
        (viewport.x as f64 * scale).round() as i64,
        (viewport.y as f64 * scale).round() as i64,
        (viewport.width as f64 * scale).round() as i64,
        (viewport.height as f64 * scale).round() as i64,
    );
    let mut client = RECT::default();
    unsafe {
        GetClientRect(parent, &mut client).map_err(|e| error("host_bounds_unavailable", e))?;
    }
    if x < 0
        || y < 0
        || w < 64
        || h < 48
        || x + w > client.right as i64
        || y + h > client.bottom as i64
    {
        return Err(error(
            "invalid_viewport",
            "Software viewport must fit the visible native client area",
        ));
    }
    Ok((x as i32, y as i32, w as i32, h as i32))
}
