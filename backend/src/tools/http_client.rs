// ============================================================
// HTTP Request tool — canonical URL parsing and pinned DNS resolution.
// ============================================================

use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use super::trait_def::{RiskLevel, Tool, ToolResult};
use crate::safety::grant::{classify_ip, parse_network_target, NetworkZone};
use crate::utils::text::truncate_chars;

const SAFE_RESPONSE_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "etag",
    "last-modified",
    "location",
    "cache-control",
];

#[async_trait]
pub trait NetworkResolver: Send + Sync {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, String>;
}

struct SystemNetworkResolver;

#[async_trait]
impl NetworkResolver for SystemNetworkResolver {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
        let addresses = tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| "DNS 解析失败（SSRF 防护：无法验证目标地址）".to_string())?
            .collect::<Vec<_>>();
        if addresses.is_empty() {
            Err("DNS 解析失败（SSRF 防护：没有可验证地址）".to_string())
        } else {
            Ok(addresses)
        }
    }
}

pub struct HttpRequestTool {
    resolver: Arc<dyn NetworkResolver>,
}

impl HttpRequestTool {
    pub fn new() -> Self {
        Self {
            resolver: Arc::new(SystemNetworkResolver),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_resolver(resolver: Arc<dyn NetworkResolver>) -> Self {
        Self { resolver }
    }
}

fn safe_response_headers(headers: &reqwest::header::HeaderMap) -> String {
    let mut out = String::new();
    for name in SAFE_RESPONSE_HEADERS {
        if let Some(value) = headers.get(*name) {
            if let Ok(value) = value.to_str() {
                out.push_str(&format!("{}: {}\n", name, truncate_chars(value, 200)));
            }
        }
    }
    out
}

fn safe_url_display(url: &url::Url) -> String {
    let mut display = url.clone();
    display.set_query(None);
    display.set_fragment(None);
    display.to_string()
}

fn is_blocked_request_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "authorization" | "proxy-authorization" | "cookie" | "set-cookie" | "www-authenticate"
    ) || name.starts_with("proxy-")
}

fn address_matches_requested_zone(requested: NetworkZone, address: SocketAddr) -> bool {
    let actual = classify_ip(address.ip());
    match requested {
        NetworkZone::Public => actual == NetworkZone::Public,
        NetworkZone::Loopback => actual == NetworkZone::Loopback,
        NetworkZone::Private => actual != NetworkZone::Public,
    }
}

fn validate_resolved_addresses(
    requested: NetworkZone,
    addresses: &[SocketAddr],
) -> Result<(), String> {
    if addresses.is_empty() {
        return Err("DNS 解析失败（SSRF 防护：没有可验证地址）".to_string());
    }
    if addresses
        .iter()
        .any(|address| !address_matches_requested_zone(requested, *address))
    {
        return Err("已阻止请求地址类别变化（SSRF 防护）".to_string());
    }
    Ok(())
}

#[async_trait]
impl Tool for HttpRequestTool {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "发送HTTP请求。使用规范化URL、一次性DNS解析和固定地址连接。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "请求URL（仅限http/https）"},
                "method": {"type": "string", "enum": ["GET", "POST", "PUT", "DELETE", "PATCH"]},
                "headers": {"type": "object"},
                "body": {"type": "string"},
                "timeout_ms": {"type": "number", "description": "超时毫秒，默认30000"}
            },
            "required": ["url"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    async fn execute(&self, _args: serde_json::Value) -> ToolResult {
        ToolResult::error("http_request requires SecurityExecutionGateway trusted context")
    }

    async fn execute_with_context(
        &self,
        args: serde_json::Value,
        context: &crate::tools::trait_def::ToolExecutionContext,
    ) -> ToolResult {
        self.execute_request(args, context).await
    }
}

impl HttpRequestTool {
    async fn execute_request(
        &self,
        args: serde_json::Value,
        context: &crate::tools::trait_def::ToolExecutionContext,
    ) -> ToolResult {
        let raw_url = args["url"].as_str().unwrap_or("");
        if raw_url.is_empty() {
            return ToolResult::error("url不能为空");
        }
        let url = match url::Url::parse(raw_url) {
            Ok(url) => url,
            Err(_) => return ToolResult::error("URL格式无效"),
        };
        let target = match parse_network_target(raw_url) {
            Ok(target) => target,
            Err(error) => return ToolResult::error(error),
        };
        let method = args["method"].as_str().unwrap_or("GET").to_uppercase();
        let timeout_ms = args["timeout_ms"].as_u64().unwrap_or(30000).min(300000);
        let port = target
            .port
            .unwrap_or(if target.scheme.eq_ignore_ascii_case("http") {
                80
            } else {
                443
            });
        let Some(crate::safety::grant::AuthorizedResource::Network { zone, .. }) =
            context.authorized_network(&target.scheme, &target.host, port, &method)
        else {
            return ToolResult::error("HTTP请求缺少安全网关创建的网络授权证据");
        };

        let addresses = match self.resolver.resolve(&target.host, port).await {
            Ok(addresses) => addresses,
            Err(error) => return ToolResult::error(error),
        };
        if let Err(error) = validate_resolved_addresses(*zone, &addresses) {
            return ToolResult::error(error);
        }

        let client = match reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(&target.host, &addresses)
            .build()
        {
            Ok(client) => client,
            Err(error) => return ToolResult::error(format!("创建HTTP客户端失败: {error}")),
        };

        let mut request = match method.as_str() {
            "GET" => client.get(url.clone()),
            "POST" => client.post(url.clone()),
            "PUT" => client.put(url.clone()),
            "DELETE" => client.delete(url.clone()),
            "PATCH" => client.patch(url.clone()),
            _ => return ToolResult::error(format!("不支持的HTTP方法: {method}")),
        };
        if let Some(headers) = args["headers"].as_object() {
            for (key, value) in headers {
                if is_blocked_request_header(key) {
                    continue;
                }
                if let Some(value) = value.as_str() {
                    request = request.header(key.as_str(), value);
                }
            }
        }
        if let Some(body) = args["body"].as_str() {
            request = request.body(body.to_string());
        }

        match request.send().await {
            Ok(response) => {
                let status = response.status();
                let headers = safe_response_headers(response.headers());
                let body = response.text().await.unwrap_or_default();
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
                    method,
                    safe_url_display(&url),
                    status,
                    headers,
                    body_display
                ))
            }
            Err(error) => ToolResult::error(format!("HTTP请求失败: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::trait_def::ToolExecutionContext;
    use async_trait::async_trait;
    use axum::{http::HeaderMap, response::IntoResponse, routing::get, Router};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct StaticResolver {
        addresses: Vec<SocketAddr>,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl NetworkResolver for StaticResolver {
        async fn resolve(&self, _host: &str, _port: u16) -> Result<Vec<SocketAddr>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.addresses.clone())
        }
    }

    #[test]
    fn request_url_display_removes_query_and_fragment() {
        let url = url::Url::parse("https://example.com/path?token=secret#frag").unwrap();
        assert_eq!(safe_url_display(&url), "https://example.com/path");
    }

    #[test]
    fn request_header_filter_blocks_credentials_and_cookies() {
        assert!(is_blocked_request_header("authorization"));
        assert!(is_blocked_request_header("proxy-authorization"));
        assert!(is_blocked_request_header("cookie"));
        assert!(!is_blocked_request_header("accept"));
    }

    #[test]
    fn resolver_result_is_validated_once_before_request() {
        let result = validate_resolved_addresses(
            NetworkZone::Public,
            &["93.184.216.34:443".parse().unwrap()],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn resolver_rejects_any_mixed_zone_candidate() {
        let result = validate_resolved_addresses(
            NetworkZone::Public,
            &[
                "93.184.216.34:443".parse().unwrap(),
                "127.0.0.1:443".parse().unwrap(),
            ],
        );
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn approved_loopback_request_uses_pinned_addresses_and_redacts_sensitive_values() {
        let request_count = Arc::new(AtomicUsize::new(0));
        let authorization_seen = Arc::new(AtomicBool::new(false));
        let count_for_handler = Arc::clone(&request_count);
        let auth_for_handler = Arc::clone(&authorization_seen);
        let app = Router::new().route(
            "/",
            get(move |headers: HeaderMap| {
                let count = Arc::clone(&count_for_handler);
                let auth = Arc::clone(&auth_for_handler);
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    auth.store(headers.contains_key("authorization"), Ordering::SeqCst);
                    let mut response = "ok".into_response();
                    response
                        .headers_mut()
                        .insert("set-cookie", "session=secret".parse().unwrap());
                    response
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let calls = Arc::new(AtomicUsize::new(0));
        let tool = HttpRequestTool::with_resolver(Arc::new(StaticResolver {
            addresses: vec![address],
            calls: Arc::clone(&calls),
        }));
        let context = ToolExecutionContext::new_with_resources(
            Arc::new(crate::isolation::ManagedProcessRegistry::new()),
            "http-test",
            vec![crate::safety::grant::AuthorizedResource::Network {
                scheme: "http".into(),
                host: "127.0.0.1".into(),
                port: address.port(),
                method: "GET".into(),
                zone: NetworkZone::Loopback,
                grant_id: None,
                one_shot_approval: true,
            }],
        );
        let result = tool
            .execute_with_context(
                serde_json::json!({
                    "url": format!("http://127.0.0.1:{}/?token=secret", address.port()),
                    "headers": {"Authorization": "Bearer secret", "Accept": "text/plain"}
                }),
                &context,
            )
            .await;
        server.abort();

        assert!(result.ok, "request failed: {}", result.content);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(request_count.load(Ordering::SeqCst), 1);
        assert!(!authorization_seen.load(Ordering::SeqCst));
        assert!(!result.content.contains("token=secret"));
        assert!(!result.content.contains("set-cookie"));
    }
}
