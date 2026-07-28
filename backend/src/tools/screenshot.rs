// ============================================================
// Screenshot tool — capture the screen via xcap, return base64
// ============================================================

use async_trait::async_trait;
use base64::Engine;
use serde::Serialize;

use super::trait_def::{Tool, ToolResult};

#[derive(Debug, Serialize)]
struct ScreenshotMetadata {
    width: u32,
    height: u32,
    format: String,
    monitor: usize,
}

fn capture_screenshot(
    monitor_idx: usize,
    x: Option<u32>,
    y: Option<u32>,
    width: Option<u32>,
    height: Option<u32>,
    format: &str,
) -> Result<(String, ScreenshotMetadata), String> {
    let monitors = xcap::Monitor::all().map_err(|e| format!("截图失败：{}", e))?;

    let monitor = monitors
        .get(monitor_idx)
        .ok_or_else(|| "截图失败：无可用显示器".to_string())?;

    let buf = monitor
        .capture_image()
        .map_err(|e| format!("截图失败：{}", e))?;

    // Convert to DynamicImage for crop + encode support
    let mut image = image::DynamicImage::ImageRgba8(buf);

    // Optional region crop
    let has_region = x.is_some() && y.is_some() && width.is_some() && height.is_some();
    if has_region {
        let rx = x.unwrap();
        let ry = y.unwrap();
        let rw = width.unwrap();
        let rh = height.unwrap();

        let img_w = image.width();
        let img_h = image.height();
        if rx + rw > img_w || ry + rh > img_h {
            return Err(format!("区域超出屏幕范围 ({}x{})", img_w, img_h));
        }
        image = image.crop(rx, ry, rw, rh);
    }

    let img_w = image.width();
    let img_h = image.height();

    // Encode to PNG or JPEG bytes
    let mime = match format {
        "jpg" | "jpeg" => "image/jpeg",
        _ => "image/png",
    };
    let img_fmt = match format {
        "jpg" | "jpeg" => image::ImageFormat::Jpeg,
        _ => image::ImageFormat::Png,
    };

    let mut out = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut out, img_fmt)
        .map_err(|e| format!("{}编码失败：{}", format, e))?;
    let bytes = out.into_inner();
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    // Return a data URI that browsers can render directly in <img src="...">
    let data_uri = format!("data:{};base64,{}", mime, b64);

    Ok((
        data_uri,
        ScreenshotMetadata {
            width: img_w,
            height: img_h,
            format: format.to_string(),
            monitor: monitor_idx,
        },
    ))
}

pub struct ScreenshotTool;

#[async_trait]
impl Tool for ScreenshotTool {
    fn name(&self) -> &str {
        "screenshot"
    }

    fn description(&self) -> &str {
        "截取屏幕截图，返回base64编码的图片数据。可选指定显示器索引和截取区域。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "monitor": {
                    "type": "integer",
                    "description": "显示器索引，默认0（主显示器）"
                },
                "x": {
                    "type": "integer",
                    "description": "区域左上角X坐标，不传则全屏截图"
                },
                "y": {
                    "type": "integer",
                    "description": "区域左上角Y坐标，不传则全屏截图"
                },
                "width": {
                    "type": "integer",
                    "description": "区域宽度"
                },
                "height": {
                    "type": "integer",
                    "description": "区域高度"
                },
                "format": {
                    "type": "string",
                    "enum": ["png", "jpg"],
                    "description": "图片格式，默认png"
                }
            }
        })
    }

    fn requires_approval(&self) -> bool {
        true
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let monitor_idx = args["monitor"].as_u64().unwrap_or(0) as usize;
        let x = args["x"].as_u64().map(|v| v as u32);
        let y = args["y"].as_u64().map(|v| v as u32);
        let width = args["width"].as_u64().map(|v| v as u32);
        let height = args["height"].as_u64().map(|v| v as u32);
        let format = args["format"].as_str().unwrap_or("png");

        match capture_screenshot(monitor_idx, x, y, width, height, format) {
            Ok((data_uri, meta)) => {
                // Return data URI on first line (rendered as <img> by frontend),
                // followed by metadata as a human-readable caption
                let output = format!(
                    "{}\n截图尺寸: {}x{} | 格式: {} | 显示器: {}",
                    data_uri, meta.width, meta.height, meta.format, meta.monitor
                );
                ToolResult::success(output)
            }
            Err(e) => ToolResult::error(e),
        }
    }
}
