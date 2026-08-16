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
    McpInputRequired, McpOperationOutcome, McpPromptDescriptor, McpPromptResult,
    McpProtocolVersion, McpResourceContent, McpResourceDescriptor, McpResourceTemplate,
    McpRuntimeError, McpRuntimeStatus, McpServerCapabilities, McpTool, MAX_MCP_LIST_PAGES,
    MAX_MCP_RESOURCES_PER_SERVER, MAX_MCP_TOOLS_PER_SERVER,
};
use super::prompts::{parse_prompt_get, parse_prompt_list};
use super::protocol::attach_request_metadata;
use super::resources::{
    parse_resource_contents, parse_resource_list, parse_resource_template_list,
};
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

const DEFAULT_CACHE_TTL_MS: u64 = 60_000;

pub struct McpRuntimeManager {
    servers: RwLock<HashMap<String, Arc<McpServerRuntime>>>,
    next_id: AtomicI64,
    cache: super::cache::McpCache,
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
            cache: super::cache::McpCache::new(),
        }
    }

    pub fn invalidate_server_cache(&self, server_id: &str) {
        self.cache.invalidate_server(server_id);
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
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
        protocol_version: McpProtocolVersion,
        cancel: &CancellationToken,
    ) -> Result<serde_json::Value, McpRuntimeError> {
        let mut params = params;
        // Modern-only `_meta` must never leak into legacy requests.
        if protocol_version == McpProtocolVersion::V2026_07_28 {
            attach_request_metadata(&mut params, "0.8.0");
        }
        let request = JsonRpcRequest::new(self.next_id(), method, Some(params));
        match transport.send(&request, cancel).await? {
            JsonRpcMessage::Success(s) => Ok(s.result),
            JsonRpcMessage::Error(e) => Err(McpRuntimeError::ServerError(e.error.message)),
            JsonRpcMessage::Notification(_) => Err(McpRuntimeError::InvalidResponse),
        }
    }

    /// Connect (or reconnect) a server and populate its tool catalog.
    pub async fn refresh_server(&self, server_id: &str) -> Result<(), McpRuntimeError> {
        // Manual refresh always bypasses + invalidates the cache.
        self.invalidate_server_cache(server_id);
        let Some(runtime) = self.get_server(server_id) else {
            return Err(McpRuntimeError::ServerNotFound);
        };
        let transport = Self::build_transport(&runtime.config)?;
        let cancel = CancellationToken::new();
        let negotiation = transport.connect(&cancel).await?;
        let tools = self
            .tools_list_wire(&*transport, negotiation.protocol_version)
            .await?;
        // Rebuild the runtime entry with the real negotiated version + catalog.
        let refreshed = Arc::new(McpServerRuntime {
            server_id: runtime.server_id.clone(),
            name: runtime.name.clone(),
            config: runtime.config.clone(),
            protocol_version: negotiation.protocol_version,
            status: McpRuntimeStatus::Ready,
            capabilities: negotiation.capabilities,
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
        protocol_version: McpProtocolVersion,
    ) -> Result<Vec<McpTool>, McpRuntimeError> {
        let cancel = CancellationToken::new();
        let mut all: Vec<McpTool> = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_MCP_LIST_PAGES {
            let mut params = serde_json::json!({});
            if let Some(c) = &cursor {
                params["cursor"] = serde_json::json!(c);
            }
            let result = self
                .send(transport, "tools/list", params, protocol_version, &cancel)
                .await?;
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
            .send(
                transport.as_ref(),
                "tools/call",
                params,
                runtime.protocol_version,
                cancel,
            )
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

    pub async fn list_resources(
        &self,
        server_id: &str,
    ) -> Result<Vec<McpResourceDescriptor>, McpRuntimeError> {
        let runtime = self
            .get_server(server_id)
            .ok_or(McpRuntimeError::ServerNotFound)?;
        let transport = runtime
            .transport
            .as_ref()
            .ok_or(McpRuntimeError::Transport("not connected".to_string()))?;
        let cancel = CancellationToken::new();
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_MCP_LIST_PAGES {
            let mut params = serde_json::json!({});
            if let Some(c) = &cursor {
                params["cursor"] = serde_json::json!(c);
            }
            let result = self
                .send(
                    transport.as_ref(),
                    "resources/list",
                    params,
                    runtime.protocol_version,
                    &cancel,
                )
                .await?;
            let (resources, next) = parse_resource_list(&result)?;
            all.extend(resources);
            if all.len() > MAX_MCP_RESOURCES_PER_SERVER {
                return Err(McpRuntimeError::ResponseTooLarge);
            }
            match next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        all.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(all)
    }

    pub async fn list_resource_templates(
        &self,
        server_id: &str,
    ) -> Result<Vec<McpResourceTemplate>, McpRuntimeError> {
        let runtime = self
            .get_server(server_id)
            .ok_or(McpRuntimeError::ServerNotFound)?;
        let transport = runtime
            .transport
            .as_ref()
            .ok_or(McpRuntimeError::Transport("not connected".to_string()))?;
        let cancel = CancellationToken::new();
        let params = serde_json::json!({});
        let result = self
            .send(
                transport.as_ref(),
                "resources/templates/list",
                params,
                runtime.protocol_version,
                &cancel,
            )
            .await?;
        let (templates, _) = parse_resource_template_list(&result)?;
        Ok(templates)
    }

    pub async fn read_resource(
        &self,
        server_id: &str,
        uri: &str,
        cancel: &CancellationToken,
    ) -> Result<McpOperationOutcome<Vec<McpResourceContent>>, McpRuntimeError> {
        let runtime = self
            .get_server(server_id)
            .ok_or(McpRuntimeError::ServerNotFound)?;
        let transport = runtime
            .transport
            .as_ref()
            .ok_or(McpRuntimeError::Transport("not connected".to_string()))?;
        let params = serde_json::json!({ "uri": uri });
        let cache_key = super::cache::McpCache::key(server_id, "resources/read", &params);
        if let Some(super::cache::McpCacheValue::ResourceRead(contents)) =
            self.cache.get(&cache_key, Self::now_ms())
        {
            return Ok(McpOperationOutcome::Complete(contents));
        }
        let result = self
            .send(
                transport.as_ref(),
                "resources/read",
                params,
                runtime.protocol_version,
                cancel,
            )
            .await?;
        match result.get("resultType").and_then(|t| t.as_str()) {
            Some("input_required") => Ok(McpOperationOutcome::InputRequired(McpInputRequired {
                prompt: result
                    .get("prompt")
                    .and_then(|p| p.as_str())
                    .unwrap_or_default()
                    .to_string(),
                input_schema: result.get("inputSchema").cloned(),
            })),
            _ => {
                let contents = parse_resource_contents(&result)?;
                self.cache.put(
                    &cache_key,
                    super::cache::McpCacheValue::ResourceRead(contents.clone()),
                    DEFAULT_CACHE_TTL_MS,
                    super::cache::CacheScope::Private,
                    Self::now_ms(),
                );
                Ok(McpOperationOutcome::Complete(contents))
            }
        }
    }

    pub async fn list_prompts(
        &self,
        server_id: &str,
    ) -> Result<Vec<McpPromptDescriptor>, McpRuntimeError> {
        let runtime = self
            .get_server(server_id)
            .ok_or(McpRuntimeError::ServerNotFound)?;
        let transport = runtime
            .transport
            .as_ref()
            .ok_or(McpRuntimeError::Transport("not connected".to_string()))?;
        let cancel = CancellationToken::new();
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_MCP_LIST_PAGES {
            let mut params = serde_json::json!({});
            if let Some(c) = &cursor {
                params["cursor"] = serde_json::json!(c);
            }
            let result = self
                .send(
                    transport.as_ref(),
                    "prompts/list",
                    params,
                    runtime.protocol_version,
                    &cancel,
                )
                .await?;
            let (prompts, next) = parse_prompt_list(&result)?;
            all.extend(prompts);
            match next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        all.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(all)
    }

    pub async fn get_prompt(
        &self,
        server_id: &str,
        name: &str,
        arguments: serde_json::Value,
        cancel: &CancellationToken,
    ) -> Result<McpOperationOutcome<McpPromptResult>, McpRuntimeError> {
        let runtime = self
            .get_server(server_id)
            .ok_or(McpRuntimeError::ServerNotFound)?;
        let transport = runtime
            .transport
            .as_ref()
            .ok_or(McpRuntimeError::Transport("not connected".to_string()))?;
        let params = serde_json::json!({ "name": name, "arguments": arguments });
        let result = self
            .send(
                transport.as_ref(),
                "prompts/get",
                params,
                runtime.protocol_version,
                cancel,
            )
            .await?;
        Ok(parse_prompt_get(&result))
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
