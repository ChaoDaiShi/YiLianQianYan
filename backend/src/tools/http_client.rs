// ============================================================
// HTTP Request tool — make HTTP requests from the agent
// ============================================================

use async_trait::async_trait;
use std::time::Duration;

use super::trait_def::{RiskLevel, Tool, ToolResult};
use crate::utils::text::truncate_chars;

pub struct HttpRequestTool;

/// Reject SSRF-prone targets: resolve the host and fail closed if ANY resolved
/// address is loopback / private / link-local / unspecified / multicast.
async fn reject_private_target(url: &str) -> Result<(), String> {
    let host = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .and_then(|rest| rest.split('/').next())
        .and_then(|authority| authority.split('@').last())
        .and_then(|authority| {
            if authority.starts_with('[') {
                authority
                    .strip_prefix('[')
                    .and_then(|h| h.split(']').next())
            } else {
                authority.split(':').next()
            }
        })
        .unwrap_or("");

    // Quick literal checks before DNS.
    if host.is_empty()
        || host.eq_ignore_ascii_case("localhost")
        || host.starts_with("127.")
        || host == "::1"
        || host == "0.0.0.0"
        || host.starts_with("169.254.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("172.16.")
        || host.starts_with("172.17.")
        || host.starts_with("172.18.")
        || host.starts_with("172.19.")
        || host.starts_with("172.20.")
        || host.starts_with("172.21.")
        || host.starts_with("172.22.")
        || host.starts_with("172.23.")
        || host.starts_with("172.24.")
        || host.starts_with("172.25.")
        || host.starts_with("172.26.")
        || host.starts_with("172.27.")
        || host.starts_with("172.28.")
        || host.starts_with("172.29.")
        || host.starts_with("172.30.")
        || host.starts_with("172.31.")
    {
        return Err("已阻止请求内网/回环地址（SSRF 防护）".to_string());
    }

    // DNS resolution: reject if ANY candidate is a non-public address.
    let addrs = match tokio::net::lookup_host((host, 80)).await {
        Ok(addrs) => addrs,
        Err(_) => return Ok(()), // DNS failure is handled by the request itself
    };
    for addr in addrs {
        let ip = addr.ip();
        let bad = match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.is_multicast()
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || v6.is_multicast()
                    || v6.is_unique_local()
                    || v6.is_unicast_link_local()
            }
        };
        if bad {
            return Err("已阻止请求内网/回环地址（SSRF 防护）".to_string());
        }
    }
    Ok(())
}

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

        // SSRF hardening: no automatic redirects (a redirect target would be a
        // second, unvalidated network target), and reject loopback/private/
        // link-local/multicast/unspecified targets before any request is sent.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| format!("创建HTTP客户端失败: {}", e))
            .unwrap();

        if let Err(e) = reject_private_target(url).await {
            return ToolResult::error(e);
        }

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
