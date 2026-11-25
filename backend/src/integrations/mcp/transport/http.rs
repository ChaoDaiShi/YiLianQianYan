// ============================================================
// Modern Streamable HTTP transport (2026-07-28).
//
// Every request is an independent HTTP POST (no Mcp-Session-Id, no persistent
// GET stream). Supports JSON and request-scoped SSE responses. Redirects are
// rejected. User-managed servers use environment names, while managed cloud
// services resolve their own origin-bound credential immediately before sending.
// ============================================================

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::super::config::validate_mcp_url;
use super::super::protocol::jsonrpc::{parse_message, JsonRpcMessage, JsonRpcRequest};
use super::super::protocol::model::{
    McpRuntimeError, MAX_MCP_REQUEST_BYTES, MAX_MCP_RESPONSE_BYTES, MAX_MCP_SSE_EVENTS_PER_REQUEST,
    MAX_MCP_SSE_EVENT_BYTES,
};
use super::super::protocol::version::MODERN_MCP_VERSION;
use super::super::security::header::{encode_header_value, reject_crlf};
use super::McpTransport;
use crate::plugin::cloud_auth::CloudBearer;

pub const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
pub const CALL_TIMEOUT: Duration = Duration::from_secs(120);

fn mcp_name_for(method: &str, params: &serde_json::Value) -> Option<String> {
    match method {
        "tools/call" => params
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        "resources/read" => params
            .get("uri")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        "prompts/get" => params
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        _ => None,
    }
}

pub struct HttpTransport {
    url: String,
    headers_from_env: BTreeMap<String, String>,
    cloud_bearer: Option<CloudBearer>,
    client: reqwest::Client,
}

impl HttpTransport {
    pub fn new(
        url: String,
        headers_from_env: BTreeMap<String, String>,
    ) -> Result<Self, McpRuntimeError> {
        Self::with_client(url, headers_from_env, reqwest::Client::builder())
    }

    pub(crate) fn managed_local(url: String) -> Result<Self, McpRuntimeError> {
        Self::with_client(url, BTreeMap::new(), reqwest::Client::builder().no_proxy())
    }

    pub(crate) fn managed_cloud(url: String, bearer: CloudBearer) -> Result<Self, McpRuntimeError> {
        let mut transport = Self::new(url, BTreeMap::new())?;
        transport.cloud_bearer = Some(bearer);
        Ok(transport)
    }

    fn with_client(
        url: String,
        headers_from_env: BTreeMap<String, String>,
        client: reqwest::ClientBuilder,
    ) -> Result<Self, McpRuntimeError> {
        validate_mcp_url(&url).map_err(|_| McpRuntimeError::Misconfigured)?;
        let client = client
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| McpRuntimeError::Transport(e.to_string()))?;
        Ok(Self {
            url,
            headers_from_env,
            cloud_bearer: None,
            client,
        })
    }

    /// Resolve configured env-referenced headers. Missing env → Misconfigured.
    fn resolve_headers(&self) -> Result<BTreeMap<String, String>, McpRuntimeError> {
        let mut headers = BTreeMap::new();
        for (name, env_var) in &self.headers_from_env {
            let value = std::env::var(env_var).map_err(|_| McpRuntimeError::Misconfigured)?;
            reject_crlf(&value).map_err(|_| McpRuntimeError::Misconfigured)?;
            headers.insert(name.clone(), value);
        }
        Ok(headers)
    }

    async fn authorize_cloud(
        &self,
        mut request: reqwest::Request,
    ) -> Result<reqwest::Request, McpRuntimeError> {
        if let Some(bearer) = &self.cloud_bearer {
            let authorization = bearer
                .authorization(request.url().as_str())
                .await
                .map_err(McpRuntimeError::Transport)?;
            request
                .headers_mut()
                .insert(reqwest::header::AUTHORIZATION, authorization);
        }
        Ok(request)
    }

    async fn send_inner(
        &self,
        request: &JsonRpcRequest,
        options: &super::McpRequestOptions,
        cancel: &CancellationToken,
    ) -> Result<JsonRpcMessage, McpRuntimeError> {
        let body = request.to_bounded_string(MAX_MCP_REQUEST_BYTES)?;

        let mut http_req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", MODERN_MCP_VERSION)
            .header("Mcp-Method", &request.method);

        if let Some(name) = mcp_name_for(
            &request.method,
            request.params.as_ref().unwrap_or(&serde_json::json!({})),
        ) {
            reject_crlf(&name)
                .map_err(|_| McpRuntimeError::Protocol("invalid name".to_string()))?;
            http_req = http_req.header("Mcp-Name", encode_header_value(&name));
        }
        for (name, value) in self.resolve_headers()? {
            http_req = http_req.header(name, value);
        }
        // Extra per-request headers (e.g. Mcp-Param-* from x-mcp-header).
        for (name, value) in &options.extra_headers {
            reject_crlf(name)
                .map_err(|_| McpRuntimeError::Protocol("invalid header name".to_string()))?;
            reject_crlf(value)
                .map_err(|_| McpRuntimeError::Protocol("invalid header value".to_string()))?;
            http_req = http_req.header(name, encode_header_value(value));
        }

        let http_req = http_req
            .body(body)
            .build()
            .map_err(|error| McpRuntimeError::Transport(error.to_string()))?;
        let http_req = tokio::select! {
            _ = cancel.cancelled() => return Err(McpRuntimeError::Cancelled),
            request = self.authorize_cloud(http_req) => request?,
        };
        let response = tokio::select! {
            _ = cancel.cancelled() => return Err(McpRuntimeError::Cancelled),
            r = self.client.execute(http_req) => r.map_err(|e| McpRuntimeError::Transport(e.to_string()))?,
        };

        let status = response.status();
        if status.is_redirection() {
            return Err(McpRuntimeError::Transport("redirect rejected".to_string()));
        }
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(McpRuntimeError::Unauthorized);
        }
        if status == reqwest::StatusCode::REQUEST_TIMEOUT {
            return Err(McpRuntimeError::Timeout);
        }
        if status == reqwest::StatusCode::PAYLOAD_TOO_LARGE {
            return Err(McpRuntimeError::RequestTooLarge);
        }
        if !status.is_success() {
            return Err(McpRuntimeError::ServerError(format!("HTTP {status}")));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();

        if content_type.contains("text/event-stream") {
            self.read_sse(response, request.id, cancel).await
        } else {
            self.read_json(response, request.id, cancel).await
        }
    }

    async fn read_json(
        &self,
        response: reqwest::Response,
        expected_id: i64,
        cancel: &CancellationToken,
    ) -> Result<JsonRpcMessage, McpRuntimeError> {
        use futures::StreamExt;
        let mut stream = response.bytes_stream();
        let mut total = 0usize;
        let mut bytes: Vec<u8> = Vec::new();
        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => return Err(McpRuntimeError::Cancelled),
                c = stream.next() => match c {
                    Some(Ok(c)) => c,
                    Some(Err(e)) => return Err(McpRuntimeError::Transport(e.to_string())),
                    None => break,
                },
            };
            total += chunk.len();
            if total > MAX_MCP_RESPONSE_BYTES {
                return Err(McpRuntimeError::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| McpRuntimeError::InvalidResponse)?;
        let message = parse_message(&value).map_err(|_| McpRuntimeError::InvalidResponse)?;
        match &message {
            JsonRpcMessage::Success(s) if s.id == expected_id => Ok(message),
            JsonRpcMessage::Error(e) if e.id == expected_id => Ok(message),
            _ => Err(McpRuntimeError::InvalidResponse),
        }
    }

    async fn read_sse(
        &self,
        response: reqwest::Response,
        expected_id: i64,
        cancel: &CancellationToken,
    ) -> Result<JsonRpcMessage, McpRuntimeError> {
        use futures::StreamExt;
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut data_lines: Vec<String> = Vec::new();
        let mut event_count = 0usize;
        let mut total_received = 0usize;

        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => return Err(McpRuntimeError::Cancelled),
                c = stream.next() => match c {
                    Some(Ok(c)) => c,
                    Some(Err(e)) => return Err(McpRuntimeError::Transport(e.to_string())),
                    None => return Err(McpRuntimeError::InvalidResponse),
                },
            };
            total_received += chunk.len();
            if total_received > MAX_MCP_RESPONSE_BYTES {
                return Err(McpRuntimeError::ResponseTooLarge);
            }
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            // A server that never emits a newline must not grow the buffer
            // without bound.
            if buffer.len() > MAX_MCP_SSE_EVENT_BYTES {
                return Err(McpRuntimeError::ResponseTooLarge);
            }

            while let Some(nl) = buffer.find('\n') {
                let mut line = buffer[..nl].to_string();
                buffer = buffer[nl + 1..].to_string();
                if line.ends_with('\r') {
                    line.pop();
                }
                if line.is_empty() {
                    // End of an event: flush collected data lines.
                    if !data_lines.is_empty() {
                        event_count += 1;
                        if event_count > MAX_MCP_SSE_EVENTS_PER_REQUEST {
                            return Err(McpRuntimeError::ResponseTooLarge);
                        }
                        let joined = data_lines.join("\n");
                        if joined.len() > MAX_MCP_SSE_EVENT_BYTES {
                            return Err(McpRuntimeError::ResponseTooLarge);
                        }
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&joined) {
                            if let Ok(message) = parse_message(&value) {
                                match &message {
                                    JsonRpcMessage::Success(s) if s.id == expected_id => {
                                        return Ok(message);
                                    }
                                    JsonRpcMessage::Error(e) if e.id == expected_id => {
                                        return Ok(message);
                                    }
                                    // notifications + wrong-id responses are ignored
                                    _ => {}
                                }
                            }
                        }
                        data_lines.clear();
                    }
                    continue;
                }
                if let Some(data) = line.strip_prefix("data:") {
                    data_lines.push(data.trim_start().to_string());
                }
                // event:/id:/retry:/comment lines are ignored for result collection.
            }
        }
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn connect(
        &self,
        cancel: &CancellationToken,
    ) -> Result<super::super::protocol::model::McpNegotiationResult, McpRuntimeError> {
        // Modern Streamable HTTP is always 2026-07-28. Discover capabilities.
        // Fail closed: a discover failure must NOT assume tools=true.
        let mut params = serde_json::json!({});
        super::super::protocol::version::attach_request_metadata(
            &mut params,
            env!("CARGO_PKG_VERSION"),
        );
        let discover = JsonRpcRequest::new(0, "server/discover", Some(params));
        match self
            .send_inner(&discover, &super::McpRequestOptions::default(), cancel)
            .await
        {
            Ok(JsonRpcMessage::Success(s)) => {
                Ok(super::super::protocol::model::McpNegotiationResult {
                    protocol_version:
                        super::super::protocol::model::McpProtocolVersion::V2026_07_28,
                    capabilities: super::super::protocol::model::parse_capabilities(&s.result),
                })
            }
            Ok(JsonRpcMessage::Error(e)) => Err(McpRuntimeError::ServerError(e.error.message)),
            Ok(JsonRpcMessage::Notification(_)) => Err(McpRuntimeError::InvalidResponse),
            Err(e) => Err(e),
        }
    }

    async fn send_with_options(
        &self,
        request: &JsonRpcRequest,
        options: &super::McpRequestOptions,
        cancel: &CancellationToken,
    ) -> Result<JsonRpcMessage, McpRuntimeError> {
        self.send_inner(request, options, cancel).await
    }

    async fn shutdown(&self) {
        // No persistent connection to close for request-scoped HTTP.
    }

    fn protocol_version(&self) -> super::super::protocol::model::McpProtocolVersion {
        super::super::protocol::model::McpProtocolVersion::V2026_07_28
    }
}

#[cfg(test)]
mod cloud_tests {
    use super::*;
    use crate::integrations::secret::{InMemorySecretStore, SecretResolver, SecretStore};
    use crate::plugin::cloud_auth::{new_reference, replace_on, revoke_on};
    use std::sync::Arc;

    #[tokio::test]
    async fn managed_cloud_http_prepares_bearer_only_for_its_https_origin() {
        let store = Arc::new(InMemorySecretStore::new());
        let resolver = Arc::new(SecretResolver::new(store.clone()));
        let db = crate::db::Database::new(std::path::Path::new(":memory:")).unwrap();
        let url = url::Url::parse("https://transport.example.test/mcp").unwrap();
        let service = "owned-cloud-service";
        let reference = new_reference(service);
        store
            .put(
                &reference,
                secrecy::SecretString::from("fixture-bearer".to_owned()),
            )
            .await
            .unwrap();
        replace_on(&db.conn(), service, &url.origin(), reference).unwrap();
        let bearer = CloudBearer::new(&db, resolver, service.into(), url.origin());
        let transport = HttpTransport::managed_cloud(url.to_string(), bearer).unwrap();

        let message = transport.client.post(url).body("{}").build().unwrap();
        let request = transport.authorize_cloud(message).await.unwrap();
        let header = &request.headers()[reqwest::header::AUTHORIZATION];
        assert_eq!(header.to_str().unwrap(), "Bearer fixture-bearer");
        assert!(header.is_sensitive());
        let foreign = transport
            .client
            .post("https://transport.example.test:9443/mcp")
            .build()
            .unwrap();
        assert!(transport.authorize_cloud(foreign).await.is_err());

        revoke_on(&db.conn(), service).unwrap();
        let same = transport
            .client
            .post(request.url().clone())
            .build()
            .unwrap();
        assert!(transport.authorize_cloud(same).await.is_err());
    }
}
