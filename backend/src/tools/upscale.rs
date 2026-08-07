// ============================================================
// UpscaleTool — AI image upscaling via bigjpg.com
// ============================================================

use async_trait::async_trait;
use serde_json::Value;

use super::trait_def::{RiskLevel, Tool, ToolResult};

pub struct UpscaleTool;

impl UpscaleTool {
    pub fn new() -> Self {
        Self
    }

    fn api_key() -> Option<String> {
        std::env::var("BIGJPG_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
    }
}

#[async_trait]
impl Tool for UpscaleTool {
    fn name(&self) -> &str {
        "upscale_image"
    }

    fn description(&self) -> &str {
        "使用 bigjpg.com AI 人工智能无损放大图片。支持动漫/照片风格，2x/4x 放大。需要设置 BIGJPG_API_KEY 环境变量（在 https://bigjpg.com 注册获取）。"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要放大的图片文件路径"
                },
                "scale": {
                    "type": "integer",
                    "description": "放大倍数：2 或 4",
                    "enum": [2, 4]
                },
                "style": {
                    "type": "string",
                    "description": "图片类型：art（动漫/插画）或 photo（照片）",
                    "enum": ["art", "photo"]
                },
                "noise": {
                    "type": "integer",
                    "description": "降噪级别：-1=无降噪, 0=低, 1=中, 2=高, 3=最高",
                    "enum": [-1, 0, 1, 2, 3]
                }
            },
            "required": ["path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    async fn execute(&self, args: Value) -> ToolResult {
        let api_key = match Self::api_key() {
            Some(k) => k,
            None => return ToolResult::error(
                "未配置 BIGJPG_API_KEY。请在 https://bigjpg.com/ 注册账号，在用户中心获取 API Key，然后设置环境变量 BIGJPG_API_KEY=你的密钥。"
            ),
        };

        let path = args["path"].as_str().unwrap_or("");
        let scale = args["scale"].as_i64().unwrap_or(2);
        let style = args["style"].as_str().unwrap_or("art");
        let noise = args["noise"].as_i64().unwrap_or(1);

        if path.is_empty() {
            return ToolResult::error("缺少 path 参数：请提供要放大的图片路径");
        }

        let img_path = std::path::Path::new(path);
        if !img_path.exists() {
            return ToolResult::error(format!("图片文件不存在: {}", path));
        }

        let img_data = match std::fs::read(img_path) {
            Ok(d) => d,
            Err(e) => return ToolResult::error(format!("读取图片失败: {}", e)),
        };

        let max_size = 5 * 1024 * 1024; // 5MB for free users
        let img_len = img_data.len();
        if img_len > max_size {
            return ToolResult::error(format!(
                "图片文件过大 ({}KB)，bigjpg 免费版限制 5MB。请先压缩或裁剪图片。",
                img_len / 1024
            ));
        }

        let file_ext = img_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png");
        let mime = match file_ext.to_lowercase().as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "webp" => "image/webp",
            "bmp" => "image/bmp",
            "gif" => "image/gif",
            _ => "image/png",
        };

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
        {
            Ok(c) => c,
            Err(e) => return ToolResult::error(format!("创建 HTTP 客户端失败: {}", e)),
        };

        // Encode image as base64 data URI
        use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
        let b64 = B64.encode(&img_data);
        let data_uri = format!("data:{};base64,{}", mime, b64);

        let scale_str = scale.to_string();
        let noise_str = noise.to_string();

        let task_body = serde_json::json!({
            "style": style,
            "noise": &noise_str,
            "x2": &scale_str,
            "input": &data_uri,
        });

        // Step 1: Create enlargement task
        let task_resp = match client
            .post("https://bigjpg.com/api/task/")
            .header("X-API-KEY", &api_key)
            .header("Content-Type", "application/json")
            .json(&task_body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return ToolResult::error(format!("连接 bigjpg.com 失败: {}。请检查网络连接。", e))
            }
        };

        let status_code = task_resp.status();
        let task_json: Value = match task_resp.text().await {
            Ok(t) => {
                match serde_json::from_str(&t) {
                    Ok(j) => j,
                    Err(_) => {
                        // If JSON parse fails, check for HTML/error page
                        let preview = if t.len() > 200 { &t[..200] } else { &t };
                        return ToolResult::error(format!(
                            "bigjpg API 返回非 JSON 响应 (HTTP {}): {}…",
                            status_code, preview
                        ));
                    }
                }
            }
            Err(e) => return ToolResult::error(format!("读取 bigjpg 响应失败: {}", e)),
        };

        if !status_code.is_success() {
            let err_msg = task_json["error"]
                .as_str()
                .or_else(|| task_json["message"].as_str())
                .unwrap_or("未知错误");
            return ToolResult::error(format!(
                "bigjpg API 错误 (HTTP {}): {}\n请确认 API Key 有效且图片格式受支持。",
                status_code, err_msg
            ));
        }

        // Step 2: Extract task ID
        let task_id = task_json["tid"]
            .as_str()
            .or_else(|| task_json["task_id"].as_str())
            .or_else(|| task_json["data"]["tid"].as_str())
            .or_else(|| task_json["data"]["task_id"].as_str());

        let task_id = match task_id {
            Some(id) => id.to_string(),
            None => {
                // Maybe already returned a download URL?
                if let Some(url) = task_json["url"].as_str() {
                    return download_result(&client, url, path).await;
                }
                // Print response for debugging
                let resp_str = serde_json::to_string_pretty(&task_json).unwrap_or_default();
                let preview = if resp_str.len() > 500 {
                    &resp_str[..500]
                } else {
                    &resp_str
                };
                return ToolResult::error(format!(
                    "无法从 bigjpg 响应中解析任务 ID。\n响应内容: {}",
                    preview
                ));
            }
        };

        // Step 3: Poll for completion (up to 5 minutes)
        let max_polls = 60;
        for _i in 0..max_polls {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;

            let status_resp = match client
                .get(&format!("https://bigjpg.com/api/task/{}", task_id))
                .header("X-API-KEY", &api_key)
                .send()
                .await
            {
                Ok(r) => r,
                Err(_) => continue, // retry on network error
            };

            let status_json: Value = match status_resp.json().await {
                Ok(j) => j,
                Err(_) => continue,
            };

            let task_status = status_json["status"]
                .as_str()
                .or_else(|| status_json["data"]["status"].as_str())
                .unwrap_or("");

            match task_status {
                "success" | "done" | "completed" => {
                    let result_url = status_json["url"]
                        .as_str()
                        .or_else(|| status_json["data"]["url"].as_str())
                        .or_else(|| status_json["download_url"].as_str());

                    let url = match result_url {
                        Some(u) => u.to_string(),
                        None => return ToolResult::error("任务已完成但响应中没有下载链接"),
                    };
                    return download_result(&client, &url, path).await;
                }
                "failed" | "error" => {
                    let err = status_json["error"]
                        .as_str()
                        .or_else(|| status_json["message"].as_str())
                        .unwrap_or("未知");
                    return ToolResult::error(format!("放大任务失败: {}", err));
                }
                _ => { /* still processing, continue polling */ }
            }
        }

        ToolResult::error(format!(
            "放大任务超时（已等待 {} 分钟）。任务 ID: {}。请稍后在 bigjpg.com 用户中心查看结果。",
            max_polls * 5 / 60,
            task_id
        ))
    }
}

/// Download result image and save beside original
async fn download_result(client: &reqwest::Client, url: &str, original_path: &str) -> ToolResult {
    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => return ToolResult::error(format!("下载放大图片失败: {}", e)),
    };

    if !resp.status().is_success() {
        return ToolResult::error(format!("下载放大图片失败: HTTP {}", resp.status()));
    }

    let img_bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => return ToolResult::error(format!("读取图片数据失败: {}", e)),
    };

    let orig = std::path::Path::new(original_path);
    let stem = orig.file_stem().unwrap_or_default().to_string_lossy();
    let ext = orig.extension().unwrap_or_default().to_string_lossy();
    let dir = orig.parent().unwrap_or_else(|| std::path::Path::new("."));

    let out_path = if ext.is_empty() {
        dir.join(format!("{}_upscaled", stem))
    } else {
        dir.join(format!("{}_upscaled.{}", stem, ext))
    };

    if let Err(e) = std::fs::write(&out_path, &img_bytes) {
        return ToolResult::error(format!("保存放大图片失败: {}", e));
    }

    let size_kb = img_bytes.len() as f64 / 1024.0;
    let size_str = if size_kb > 1024.0 {
        format!("{:.1}MB", size_kb / 1024.0)
    } else {
        format!("{:.0}KB", size_kb)
    };

    ToolResult::success(format!(
        "✅ 图片放大完成！\n原始: {}\n输出: {}\n大小: {}",
        original_path,
        out_path.display(),
        size_str
    ))
}
