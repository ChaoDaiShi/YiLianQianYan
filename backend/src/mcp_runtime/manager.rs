// ============================================================
// McpRuntimeManager — manages server configs, transports, and catalogs.
//
// This is a runtime-level implementation. `call_tool` is `pub(crate)`-visible
// only: real execution must flow through the Security Execution Gateway and the
// Managed MCP Tool Adapter, never directly from an agent or frontend.
// ============================================================

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;
use tokio_util::sync::CancellationToken;

use super::http::HttpTransport;
use super::jsonrpc::{JsonRpcMessage, JsonRpcRequest};
use super::model::{
    McpProtocolVersion, McpRuntimeError, McpRuntimeStatus, McpServerCapabilities, McpTool,
    MAX_MCP_LIST_PAGES, MAX_MCP_TOOLS_PER_SERVER,
};
use super::protocol::attach_request_metadata;
use super::stdio::StdioTransport;
use super::tools::{call_result_text, parse_call_result, parse_tool_list};
use super::transport::{McpTransport, McpTransportConfig};

pub struct McpServerRuntime {
    pub server_id: String,
    pub name: String,
    pub config: McpTransportConfig,
    pub protocol_version: McpProtocolVersion,
    pub status: McpRuntimeStatus,
    pub capabilities: McpServerCapabilities,
    pub tools: Vec<McpTool>,
    pub last_error: Option<String>,
    pub last_refresh: Option<i64>,
    transport: Option<Arc<dyn McpTransport>>,
}

impl McpServerRuntime {
    fn new(server_id: String, name: String, config: McpTransportConfig) -> Self {
        Self {
            server_id,
            name,
            config,
            protocol_version: McpProtocolVersion::V2025_11_25,
            status: McpRuntimeStatus::Disconnected,
            capabilities: McpServerCapabilities::default(),
            tools: Vec::new(),
            last_error: None,
            last_refresh: None,
            transport: None,
        }
    }
}

pub struct McpRuntimeManager {
    servers: RwLock<HashMap<String, Arc<McpServerRuntime>>>,
    next_id: AtomicI64,
}

impl Default for McpRuntimeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl McpRuntimeManager {
    pub fn new() -> Self {
        Self {
            servers: RwLock::new(HashMap::new()),
            next_id: AtomicI64::new(1),
        }
    }

    pub fn register_server(&self, server_id: String, name: String, config: McpTransportConfig) {
        self.servers.write().insert(
            server_id.clone(),
            Arc::new(McpServerRuntime::new(server_id, name, config)),
        );
    }

    pub fn list_servers(&self) -> Vec<Arc<McpServerRuntime>> {
        let mut v: Vec<_> = self.servers.read().values().cloned().collect();
        v.sort_by(|a, b| a.server_id.cmp(&b.server_id));
        v
    }

    pub fn get_server(&self, server_id: &str) -> Option<Arc<McpServerRuntime>> {
        self.servers.read().get(server_id).cloned()
    }

    fn next_id(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    fn build_transport(
        config: &McpTransportConfig,
    ) -> Result<Arc<dyn McpTransport>, McpRuntimeError> {
        match config {
            McpTransportConfig::Stdio { command, args, env } => Ok(Arc::new(StdioTransport::new(
                command.clone(),
                args.clone(),
                env.clone(),
            ))),
            McpTransportConfig::StreamableHttp {
                url,
                headers_from_env,
            } => Ok(Arc::new(HttpTransport::new(
                url.clone(),
                headers_from_env.clone(),
            )?)),
        }
    }

    async fn send(
        &self,
        transport: &dyn McpTransport,
        method: &str,
        params: serde_json::Value,
        cancel: &CancellationToken,
    ) -> Result<serde_json::Value, McpRuntimeError> {
        let mut params = params;
        attach_request_metadata(&mut params, "0.8.0");
        let request = JsonRpcRequest::new(self.next_id(), method, Some(params));
        match transport.send(&request, cancel).await? {
            JsonRpcMessage::Success(s) => Ok(s.result),
            JsonRpcMessage::Error(e) => Err(McpRuntimeError::ServerError(e.error.message)),
            JsonRpcMessage::Notification(_) => Err(McpRuntimeError::InvalidResponse),
        }
    }

    /// Connect (or reconnect) a server and populate its tool catalog.
    pub async fn refresh_server(&self, server_id: &str) -> Result<(), McpRuntimeError> {
        let Some(runtime) = self.get_server(server_id) else {
            return Err(McpRuntimeError::ServerNotFound);
        };
        let transport = Self::build_transport(&runtime.config)?;
        let tools = self.tools_list_wire(&*transport).await?;
        // Rebuild the runtime entry with the connected transport + catalog.
        let refreshed = Arc::new(McpServerRuntime {
            server_id: runtime.server_id.clone(),
            name: runtime.name.clone(),
            config: runtime.config.clone(),
            protocol_version: McpProtocolVersion::V2026_07_28,
            status: McpRuntimeStatus::Ready,
            capabilities: McpServerCapabilities {
                tools: true,
                ..Default::default()
            },
            tools,
            last_error: None,
            last_refresh: Some(chrono::Utc::now().timestamp_millis()),
            transport: Some(transport),
        });
        self.servers
            .write()
            .insert(server_id.to_string(), refreshed);
        Ok(())
    }

    async fn tools_list_wire(
        &self,
        transport: &dyn McpTransport,
    ) -> Result<Vec<McpTool>, McpRuntimeError> {
        let cancel = CancellationToken::new();
        let mut all: Vec<McpTool> = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_MCP_LIST_PAGES {
            let mut params = serde_json::json!({});
            if let Some(c) = &cursor {
                params["cursor"] = serde_json::json!(c);
            }
            let result = self.send(transport, "tools/list", params, &cancel).await?;
            let (tools, next_cursor) = parse_tool_list(&result)?;
            all.extend(tools);
            if all.len() > MAX_MCP_TOOLS_PER_SERVER {
                return Err(McpRuntimeError::ResponseTooLarge);
            }
            match next_cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        all.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(all)
    }

    pub async fn list_tools(&self, server_id: &str) -> Result<Vec<McpTool>, McpRuntimeError> {
        let runtime = self
            .get_server(server_id)
            .ok_or(McpRuntimeError::ServerNotFound)?;
        // Reuse cached catalog if already connected.
        if runtime.transport.is_some() {
            return Ok(runtime.tools.clone());
        }
        self.refresh_server(server_id).await?;
        Ok(self
            .get_server(server_id)
            .map(|r| r.tools.clone())
            .unwrap_or_default())
    }

    /// Execute a remote tool. `pub(crate)`: only the Managed MCP Tool Adapter
    /// (behind the Security Execution Gateway) should call this.
    pub(crate) async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
        cancel: &CancellationToken,
    ) -> Result<McpToolCallResult, McpRuntimeError> {
        let runtime = self
            .get_server(server_id)
            .ok_or(McpRuntimeError::ServerNotFound)?;
        let transport = runtime
            .transport
            .as_ref()
            .ok_or(McpRuntimeError::Transport("not connected".to_string()))?;
        let params = serde_json::json!({ "name": tool_name, "arguments": arguments });
        let result = self
            .send(transport.as_ref(), "tools/call", params, cancel)
            .await?;
        match parse_call_result(&result) {
            super::model::McpOperationOutcome::Complete(value) => {
                let is_error = value
                    .get("isError")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                Ok(McpToolCallResult {
                    is_error,
                    text: call_result_text(&value),
                })
            }
            super::model::McpOperationOutcome::InputRequired(_) => {
                Err(McpRuntimeError::InputRequired)
            }
        }
    }

    pub async fn shutdown_all(&self) {
        let runtimes: Vec<_> = self.servers.read().values().cloned().collect();
        for runtime in runtimes {
            if let Some(transport) = &runtime.transport {
                transport.shutdown().await;
            }
        }
    }
}

/// A bounded, safe tool call result (no raw base64 / structured content).
pub struct McpToolCallResult {
    pub is_error: bool,
    pub text: String,
}
