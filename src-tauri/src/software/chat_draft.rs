//! Write a single draft to the active conversation in an explicitly delegated WeChat window.
use super::{
    automation, chat_capture,
    identity::{self, error},
};
use image::{DynamicImage, RgbImage};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HWND, POINT},
    Graphics::Gdi::ClientToScreen,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
use yilian_backend::{
    software::{SoftwareError, WindowIdentity},
    tools::gui_window::{SystemWindowController, WindowController},
};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Layout {
    divider: u32,
    editor: u32,
    width: u32,
    height: u32,
}

fn strongest(start: u32, end: u32, strength: impl Fn(u32) -> usize) -> Option<u32> {
    let mut best = None;
    let mut score = 23;
    for p in start..=end {
        let s = strength(p);
        if s > score {
            best = Some(p);
            score = s;
        }
    }
    best
}
fn layout(im: &RgbImage) -> Option<Layout> {
    let (w, h) = im.dimensions();
    if w < 300 || h < 240 {
        return None;
    }
    let edge = |x, y, xx, yy, threshold| {
        im.get_pixel(x, y)
            .0
            .iter()
            .zip(im.get_pixel(xx, yy).0)
            .any(|(a, b)| a.abs_diff(b) >= threshold)
    };
    let divider = strongest(w * 15 / 100, w * 62 / 100, |x| {
        (0..32)
            .filter(|i| {
                let y = h * (16 * 31 + 48 * i) / 3100;
                edge(x - 1, y, x, y, 7)
            })
            .count()
    })?;
    let editor = strongest(h * 48 / 100, h * 91 / 100, |y| {
        (0..32)
            .filter(|i| {
                let x = divider + (w - divider) * (9 * 31 + 79 * i) / 3100;
                edge(x, y - 1, x, y, 10)
            })
            .count()
    })?;
    Some(Layout {
        divider,
        editor,
        width: w,
        height: h,
    })
}
fn snapshot(target: &WindowIdentity) -> Result<(DynamicImage, Layout), SoftwareError> {
    let im = image::load_from_memory(&chat_capture::capture(target)?)
        .map_err(|e| error("wechat_capture_failed", e))?;
    let area = layout(&im.to_rgb8()).ok_or_else(|| {
        error(
            "wechat_editor_missing",
            "没有识别到聊天列表分隔线和消息输入框",
        )
    })?;
    Ok((im, area))
}
fn group_key(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .replace('（', "(")
        .replace('）', ")")
        .replace("—群", "一群")
        .replace("–群", "一群")
}
fn check_group(im: &DynamicImage, a: Layout, group: Option<&str>) -> Result<(), SoftwareError> {
    let Some(group) = group else { return Ok(()) };
    let left = a.divider + (a.width - a.divider) / 50;
    let title = im.crop_imm(
        left,
        a.height / 17,
        (a.width - left) * 72 / 100,
        a.height / 11,
    );
    let recognized = group_key(
        &chat_capture::recognize_image(title)
            .map_err(|_| error("wechat_group_unreadable", "未能读清当前聊天标题"))?,
    );
    let mut title = recognized.as_str();
    if let Some((base, count)) = title.rsplit_once('(') {
        if count
            .strip_suffix(')')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|d| d.is_ascii_digit()))
        {
            title = base;
        }
    }
    let expected = group_key(group);
    if title == expected
        || title
            .split_once('(')
            .is_some_and(|(base, _)| base == expected)
    {
        Ok(())
    } else {
        Err(error(
            "wechat_group_mismatch",
            "当前打开的聊天标题与指定会话不匹配",
        ))
    }
}
// The blinking green insertion caret otherwise reads as a trailing “1” in short drafts.
fn remove_editor_caret(crop: &mut RgbImage) {
    let mut bounds = None::<(u32, u32, u32, u32)>;
    let mut count = 0;
    for (x, y, pixel) in crop.enumerate_pixels() {
        let [r, g, b] = pixel.0;
        if g.saturating_sub(r) > 100 && g.saturating_sub(b) > 40 {
            let (left, top, right, bottom) = bounds.get_or_insert((x, y, x, y));
            *left = (*left).min(x);
            *top = (*top).min(y);
            *right = (*right).max(x);
            *bottom = (*bottom).max(y);
            count += 1;
        }
    }
    if let Some((left, top, right, bottom)) = bounds {
        let width = right - left + 1;
        let height = bottom - top + 1;
        if width <= 4 && height >= 8 && count == width * height {
            let background = *crop.get_pixel(crop.width() - 2, 2);
            for y in top..=bottom {
                for x in left..=right {
                    crop.put_pixel(x, y, background);
                }
            }
        }
    }
}
fn read_editor(im: &DynamicImage, a: Layout, ascii: bool) -> Result<String, SoftwareError> {
    let cw = a.width - a.divider;
    let depth = a.height - a.editor;
    let margin = (cw / 35).max(4);
    let x = a.divider + margin;
    let y = a.editor + (depth / 22).max(3);
    let mut pixels = im
        .crop_imm(x, y, cw - margin * 2, depth * 58 / 100)
        .into_rgb8();
    remove_editor_caret(&mut pixels);
    let crop = DynamicImage::ImageRgb8(pixels);
    let gray = crop.to_luma8();
    let bg = gray.get_pixel(gray.width() - 2, 2)[0];
    let mut ink = None::<(u32, u32, u32, u32)>;
    for (x, y, color) in gray.enumerate_pixels() {
        if color[0].abs_diff(bg) > 85 {
            let (left, top, right, bottom) = ink.get_or_insert((x, y, x, y));
            *left = (*left).min(x);
            *top = (*top).min(y);
            *right = (*right).max(x);
            *bottom = (*bottom).max(y);
        }
    }
    let Some((left, top, right, bottom)) = ink else {
        return Ok(String::new());
    };
    let height = bottom - top + 1;
    if ascii && right - left < 4 * height && height <= 64 {
        // Windows OCR can drop a whole line containing just one or two letters. Give it
        // three spaced copies of the actual ink and accept only three identical readings.
        let x = left.saturating_sub(1);
        let y = top.saturating_sub(1);
        let word = crop
            .crop_imm(
                x,
                y,
                (right + 2).min(crop.width()) - x,
                (bottom + 2).min(crop.height()) - y,
            )
            .into_luma8();
        let padding = word.height();
        let mut line = image::GrayImage::from_pixel(
            3 * word.width() + 6 * padding,
            3 * padding,
            image::Luma([bg]),
        );
        for i in 0..3 {
            image::imageops::replace(
                &mut line,
                &word,
                i64::from(padding + i * (word.width() + 2 * padding)),
                i64::from(padding),
            );
        }
        if let Ok(reading) = chat_capture::recognize_editor(DynamicImage::ImageLuma8(line), true) {
            let mut words = reading.split_whitespace();
            if let Some(first) = words.next() {
                if words.next() == Some(first)
                    && words.next() == Some(first)
                    && words.next().is_none()
                {
                    return Ok(first.to_owned());
                }
            }
        }
    }
    chat_capture::recognize_editor(crop, ascii)
        .map_err(|_| error("wechat_draft_unreadable", "未能读清当前草稿"))
}
fn keyboard(vk: VIRTUAL_KEY, unit: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: unit,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}
fn send(events: &[INPUT]) -> Result<(), SoftwareError> {
    if unsafe { SendInput(events, std::mem::size_of::<INPUT>() as i32) } as usize == events.len() {
        Ok(())
    } else {
        Err(SoftwareError::unknown(
            "系统未确认输入事件完整送达，请检查原草稿",
        ))
    }
}
fn ready(target: &WindowIdentity, hwnd: HWND, cancel: &AtomicBool) -> Result<(), SoftwareError> {
    if cancel.load(Ordering::Acquire)
        || identity::verify(target)? != hwnd
        || unsafe { GetForegroundWindow() } != hwnd
    {
        return Err(error("wechat_focus_changed", "微信已失去焦点或操作已取消"));
    }
    Ok(())
}
fn focus_editor(
    target: &WindowIdentity,
    hwnd: HWND,
    a: Layout,
    cancel: &AtomicBool,
    closing: &AtomicBool,
) -> Result<(), SoftwareError> {
    if let Some(control) = automation::observe(target, "wechat-draft", false)?
        .into_iter()
        .find(|c| {
            c.enabled
                && !c.is_password
                && matches!(c.name.as_str(), "输入消息" | "消息输入框" | "聊天输入框")
                && c.role.eq_ignore_ascii_case("edit")
        })
    {
        return automation::control(target, &control, "focus", None, cancel, closing, &|| {
            ready(target, hwnd, cancel)
        });
    }
    let mut point = POINT {
        x: (a.divider + (a.width - a.divider) / 5) as i32,
        y: (a.editor + (a.height - a.editor) / 5) as i32,
    };
    unsafe {
        ClientToScreen(hwnd, &mut point)
            .ok()
            .map_err(|e| error("wechat_editor_missing", e))?;
        ready(target, hwnd, cancel)?;
        if GetAncestor(WindowFromPoint(point), GA_ROOT) != hwnd {
            return Err(error(
                "wechat_editor_obscured",
                "微信消息输入框被其他窗口遮挡",
            ));
        }
        let mut old = POINT::default();
        GetCursorPos(&mut old).map_err(|e| error("wechat_cursor_unavailable", e))?;
        SetCursorPos(point.x, point.y).map_err(|e| error("wechat_cursor_unavailable", e))?;
        let clicked = send(&[MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP].map(|f| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: f,
                    ..Default::default()
                },
            },
        }));
        let mut current = POINT::default();
        if GetCursorPos(&mut current).is_ok() && current == point {
            let _ = SetCursorPos(old.x, old.y);
        }
        clicked?;
    }
    Ok(())
}
pub(super) fn write(
    target: &WindowIdentity,
    group: Option<&str>,
    text: &str,
    replace: bool,
    cancel: &AtomicBool,
    closing: &AtomicBool,
) -> Result<(), SoftwareError> {
    let hwnd = identity::verify(target)?;
    if closing.load(Ordering::Acquire) || cancel.load(Ordering::Acquire) {
        return Err(error("cancelled", "草稿操作已取消"));
    }
    let restored = unsafe { IsIconic(hwnd).as_bool() };
    if restored {
        unsafe {
            let _ = ShowWindowAsync(hwnd, SW_RESTORE);
        }
        if !super::mount::wait_for_window_state(target, |window| unsafe {
            IsWindowVisible(window).as_bool() && !IsIconic(window).as_bool()
        })? {
            return Err(error(
                "wechat_restore_failed",
                "微信仍处于最小化状态，请显示窗口后重试",
            ));
        }
    }
    // Qt can acknowledge SW_RESTORE before it paints the first usable chat frame.
    let repaint_deadline = Instant::now() + Duration::from_millis(1500);
    let (before, a) = loop {
        if cancel.load(Ordering::Acquire) || closing.load(Ordering::Acquire) {
            return Err(error("cancelled", "草稿操作已取消"));
        }
        match snapshot(target) {
            Err(e)
                if restored
                    && e.code == "wechat_editor_missing"
                    && Instant::now() < repaint_deadline =>
            {
                thread::sleep(Duration::from_millis(60));
            }
            frame => break frame?,
        }
    };
    check_group(&before, a, group)?;
    let ascii = text.is_ascii();
    let existing = read_editor(&before, a, ascii);
    if replace && existing.as_deref().ok() == Some(text) {
        return Ok(());
    }
    let expected = if replace {
        text.to_owned()
    } else {
        format!("{}{text}", existing?)
    };
    if unsafe { GetForegroundWindow() } != hwnd {
        SystemWindowController
            .restore_and_focus(hwnd.0 as usize as u32)
            .map_err(|e| error("wechat_focus_failed", e))?;
        for _ in 0..15 {
            if unsafe { GetForegroundWindow() } == hwnd {
                break;
            }
            thread::sleep(Duration::from_millis(24));
        }
    }
    ready(target, hwnd, cancel)?;
    let (current, b) = snapshot(target)?;
    check_group(&current, b, group)?;
    focus_editor(target, hwnd, b, cancel, closing)?;
    ready(target, hwnd, cancel)?;
    let (focused, c) = snapshot(target)?;
    check_group(&focused, c, group)?;
    if c != b {
        return Err(error("wechat_layout_changed", "微信窗口布局已变更"));
    }
    let key = if replace { VK_A } else { VK_END };
    let mut events = vec![
        keyboard(VK_CONTROL, 0, KEYBD_EVENT_FLAGS(0)),
        keyboard(key, 0, KEYBD_EVENT_FLAGS(0)),
        keyboard(key, 0, KEYEVENTF_KEYUP),
        keyboard(VK_CONTROL, 0, KEYEVENTF_KEYUP),
    ];
    for u in text.encode_utf16() {
        events.push(keyboard(VIRTUAL_KEY(0), u, KEYEVENTF_UNICODE));
        events.push(keyboard(
            VIRTUAL_KEY(0),
            u,
            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
        ));
    }
    ready(target, hwnd, cancel)?;
    if closing.load(Ordering::Acquire) {
        return Err(error("cancelled", "桌面宿主正在退出"));
    }
    send(&events)?;
    for _ in 0..4 {
        thread::sleep(Duration::from_millis(100));
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let Ok((im, a)) = snapshot(target) else { break };
        if check_group(&im, a, group).is_err() {
            break;
        }
        if read_editor(&im, a, ascii).as_deref().ok() == Some(expected.as_str()) {
            return Ok(());
        }
    }
    Err(SoftwareError::unknown(
        "已尝试输入，尚未从微信输入框核对到目标草稿，请检查原窗口",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn blinking_caret_is_removed_without_erasing_the_adjacent_letter() {
        let background = Rgb([250, 250, 250]);
        let mut image = RgbImage::from_pixel(50, 32, background);
        for y in 5..28 {
            image.put_pixel(29, y, Rgb([15, 15, 15]));
            image.put_pixel(30, y, Rgb([0, 195, 117]));
            image.put_pixel(31, y, Rgb([0, 195, 117]));
        }
        remove_editor_caret(&mut image);
        assert_eq!(*image.get_pixel(29, 10), Rgb([15, 15, 15]));
        assert_eq!(*image.get_pixel(30, 10), background);
        assert_eq!(*image.get_pixel(31, 10), background);
    }
}
