//! One client-window frame, followed by local recognition of the user's selected pixels.
use super::identity::{self, error};
use base64::Engine;
use image::{DynamicImage, ImageFormat, RgbImage};
use std::{io::Cursor, ptr};
use windows::{
    core::HSTRING,
    Globalization::Language,
    Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Storage::Streams::DataWriter,
    Win32::{
        Foundation::RECT,
        Graphics::Gdi::*,
        Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS},
        System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
        UI::WindowsAndMessaging::{GetClientRect, IsHungAppWindow, IsIconic, IsWindowVisible},
    },
};
use yilian_backend::software::{SoftwareError, WindowIdentity};

pub(super) fn capture(target: &WindowIdentity) -> Result<Vec<u8>, SoftwareError> {
    let hwnd = identity::verify(target)?;
    let mut rect = RECT::default();
    let unavailable = || {
        error(
            "wechat_capture_failed",
            "无法读取微信窗口，请打开目标群聊后重试",
        )
    };
    unsafe {
        if !IsWindowVisible(hwnd).as_bool()
            || IsIconic(hwnd).as_bool()
            || IsHungAppWindow(hwnd).as_bool()
        {
            return Err(unavailable());
        }
        GetClientRect(hwnd, &mut rect).map_err(|_| unavailable())?;
    }
    let (width, height) = (rect.right, rect.bottom);
    if width <= 0 || height <= 0 || i64::from(width) * i64::from(height) > 16_000_000 {
        return Err(unavailable());
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    // A 32-bit DIB needs no screen DC. Print only the identified client's own pixels.
    let rgb = unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return Err(unavailable());
        }
        let mut bits = ptr::null_mut();
        let result = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|_| unavailable())
            .and_then(|bitmap| {
                let old = SelectObject(dc, bitmap.into());
                let mut out = Vec::new();
                if !old.is_invalid()
                    && !bits.is_null()
                    && PrintWindow(hwnd, dc, PRINT_WINDOW_FLAGS(3)).as_bool()
                {
                    let _ = GdiFlush();
                    let len = width as usize * height as usize;
                    out.reserve_exact(len * 3);
                    for pixel in
                        std::slice::from_raw_parts(bits.cast::<u8>(), len * 4).chunks_exact(4)
                    {
                        out.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
                    }
                }
                if !old.is_invalid() {
                    SelectObject(dc, old);
                }
                let _ = DeleteObject(bitmap.into());
                if out.is_empty() {
                    Err(unavailable())
                } else {
                    Ok(out)
                }
            });
        let _ = DeleteDC(dc);
        result?
    };
    identity::verify(target)?;
    let frame = RgbImage::from_raw(width as u32, height as u32, rgb).ok_or_else(unavailable)?;
    let mut output = Cursor::new(Vec::new());
    frame
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|_| unavailable())?;
    Ok(output.into_inner())
}

pub(crate) fn recognize_png(encoded: &str) -> Result<String, String> {
    if encoded.len() > 24_000_000 {
        return Err("选区过大，请缩小范围".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "选区图像无效")?;
    let reader = || image::ImageReader::with_format(Cursor::new(&bytes), ImageFormat::Png);
    let (w, h) = reader().into_dimensions().map_err(|_| "选区图像无效")?;
    if w < 8 || h < 8 || u64::from(w) * u64::from(h) > 16_000_000 {
        return Err("请重新选择聊天区域".into());
    }
    let image = reader().decode().map_err(|_| "无法打开选区图像")?;
    recognize_image(image)
}

pub(super) fn recognize_image(image: DynamicImage) -> Result<String, String> {
    recognize_editor(image, false)
}

pub(super) fn recognize_editor(image: DynamicImage, ascii: bool) -> Result<String, String> {
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|e| e.to_string())?;
    let result = recognize(image, ascii);
    unsafe { RoUninitialize() };
    result
}

fn recognize(image: DynamicImage, ascii: bool) -> Result<String, String> {
    let native_error = |e: windows::core::Error| format!("本机文字识别失败：{e}");
    let tag = if ascii { "en-US" } else { "zh-Hans" };
    let language = Language::CreateLanguage(&HSTRING::from(tag)).map_err(native_error)?;
    let engine = OcrEngine::TryCreateFromLanguage(&language)
        .map_err(|_| format!("请先在 Windows 安装 {tag} OCR 语言组件"))?;
    let limit = OcrEngine::MaxImageDimension().map_err(native_error)?;
    let scale = (f64::from(limit) / f64::from(image.width().max(image.height()))).min(2.0);
    let (w, h) = (
        (f64::from(image.width()) * scale).round() as u32,
        (f64::from(image.height()) * scale).round() as u32,
    );
    let mut pixels = image
        .resize_exact(w, h, image::imageops::FilterType::Lanczos3)
        .into_luma8();
    // WeChat uses pale gray for sender names and timestamps. Preserve them alongside the bubbles.
    let samples = pixels.as_raw().iter().step_by(127);
    let dark_background = samples.clone().filter(|&&v| v < 96).count() > samples.count() / 2;
    for value in pixels.as_mut() {
        let light = if dark_background {
            255 - *value
        } else {
            *value
        };
        *value = ((u16::from(light.saturating_sub(110)) * 255 / 128).min(255)) as u8;
    }
    let writer = DataWriter::new().map_err(native_error)?;
    writer.WriteBytes(pixels.as_raw()).map_err(native_error)?;
    let buffer = writer.DetachBuffer().map_err(native_error)?;
    let bitmap =
        SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Gray8, w as i32, h as i32)
            .map_err(native_error)?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .and_then(|op| op.get())
        .map_err(native_error)?;
    let mut lines = Vec::new();
    for line in result.Lines().map_err(native_error)? {
        let mut text = String::new();
        let mut previous_right = 0.0;
        let mut top = u32::MAX;
        let mut left = u32::MAX;
        for word in line.Words().map_err(native_error)? {
            let rect = word.BoundingRect().map_err(native_error)?;
            top = top.min(rect.Y as u32);
            left = left.min(rect.X as u32);
            let token = word.Text().map_err(native_error)?.to_string_lossy();
            if !text.is_empty() && rect.X - previous_right > rect.Height * 0.32 {
                text.push(' ');
            }
            text.push_str(&token);
            previous_right = rect.X + rect.Width;
        }
        if !text.trim().is_empty() {
            lines.push((top, left, text));
        }
    }
    if lines.is_empty() {
        return Err("没有识别到可见文字".into());
    }
    lines.sort_by_key(|(top, left, _)| (*top, *left));
    Ok(lines
        .into_iter()
        .map(|(_, _, text)| text)
        .collect::<Vec<_>>()
        .join("\n"))
}
