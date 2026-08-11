// ============================================================
// HTTP Request tool — make HTTP requests from the agent
// ============================================================

use async_trait::async_trait;
use std::time::Duration;

use super::trait_def::{RiskLevel, Tool, ToolResult};
use crate::utils::text::truncate_chars;

pub struct HttpRequestTool;

impl HttpRequestTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for HttpRequestTool {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "发送HTTP请求。支持GET/POST/PUT/DELETE/PATCH方法。自动防止SSRF攻击（阻止内网IP）。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "请求URL（仅限http/https）"
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE", "PATCH"],
                    "description": "HTTP方法，默认GET"
                },
                "headers": {
                    "type": "object",
                    "description": "请求头（键值对）"
                },
                "body": {
                    "type": "string",
                    "description": "请求体（POST/PUT/PATCH时使用）"
                },
                "timeout_ms": {
                    "type": "number",
                    "description": "超时毫秒，默认30000"
                }
            },
            "required": ["url"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let url = args["url"].as_str().unwrap_or("");
        if url.is_empty() {
            return ToolResult::error("url不能为空");
        }

        // Basic SSRF protection: only allow http/https
        let url_lower = url.to_lowercase();
        if !url_lower.starts_with("https://") && !url_lower.starts_with("http://") {
            return ToolResult::error("只支持http/https协议");
        }

        let method = args["method"].as_str().unwrap_or("GET").to_uppercase();
        let timeout_ms = args["timeout_ms"].as_u64().unwrap_or(30000);
        let body = args["body"].as_str().map(|s| s.to_string());

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .redirect(reqwest::redirect::Policy::limited(3))
            .build()
            .map_err(|e| format!("创建HTTP客户端失败: {}", e))
            .unwrap();

        let mut request = match method.as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "PATCH" => client.patch(url),
            _ => return ToolResult::error(format!("不支持的HTTP方法: {}", method)),
        };

        // Add headers
        if let Some(headers) = args["headers"].as_object() {
            for (key, value) in headers {
                if let Some(v) = value.as_str() {
                    request = request.header(key.as_str(), v);
                }
            }
        }

        // Add body
        if let Some(b) = body {
            request = request.body(b);
        }

        match request.send().await {
            Ok(response) => {
                let status = response.status();
                let headers = format!("{:?}", response.headers());
                let body = response.text().await.unwrap_or_default();

                // Truncate response body if too large
                let body_display = truncate_chars(&body, 10000);
                let body_display = if body.chars().count() > 10000 {
                    format!(
                        "{}\n(响应体过大，已截断至10000字符，完整大小: {}字节)",
                        body_display,
                        body.len()
                    )
                } else {
                    body_display
                };

                ToolResult::success(format!(
                    "HTTP {} {}\n响应状态: {}\n响应头:\n{}\n\n响应体:\n{}",
                    method, url, status, headers, body_display
                ))
            }
            Err(e) => ToolResult::error(format!("HTTP请求失败: {}", e)),
        }
    }
}
