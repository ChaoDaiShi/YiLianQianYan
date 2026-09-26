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

use super::header_schema::scan_tool_header_bindings;
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
    pub resources: Vec<McpResourceDescriptor>,
    pub resource_templates: Vec<McpResourceTemplate>,
    pub prompts: Vec<McpPromptDescriptor>,
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
            resources: Vec::new(),
            resource_templates: Vec::new(),
            prompts: Vec::new(),
            last_error: None,
            last_refresh: None,
            transport: None,
        }
    }
}

/// Parse a remote cache hint (`ttlMs` + `cacheScope`) from a response. Returns
/// `(0, Private)` when absent — the modern protocol's conservative default.
fn parse_cache_hint(result: &serde_json::Value) -> (u64, super::cache::CacheScope) {
    let meta = result.get("_meta");
    let ttl_ms = meta
        .and_then(|m| m.get("ttlMs"))
        .or_else(|| result.get("ttlMs"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let scope = match meta
        .and_then(|m| m.get("cacheScope"))
        .and_then(|v| v.as_str())
    {
        Some("public") => super::cache::CacheScope::Public,
        _ => super::cache::CacheScope::Private,
    };
    (ttl_ms, scope)
}

pub struct McpRuntimeManager {
    servers: RwLock<HashMap<String, Arc<McpServerRuntime>>>,
    next_id: AtomicI64,
    cache: super::cache::McpCache,
    resolver: Arc<crate::secret::SecretResolver>,
}

impl Default for McpRuntimeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl McpRuntimeManager {
    pub fn new() -> Self {
        Self::with_resolver(Arc::new(crate::secret::SecretResolver::new(Arc::new(
            crate::secret::InMemorySecretStore::new(),
        ))))
    }

    /// Construct a manager backed by an explicit SecretResolver (production).
    pub fn with_resolver(resolver: Arc<crate::secret::SecretResolver>) -> Self {
        Self {
            servers: RwLock::new(HashMap::new()),
            next_id: AtomicI64::new(1),
            cache: super::cache::McpCache::new(),
            resolver,
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
        &self,
        config: &McpTransportConfig,
    ) -> Result<Arc<dyn McpTransport>, McpRuntimeError> {
        match config {
            McpTransportConfig::Stdio {
                command,
                args,
                env,
                env_secret_refs,
            } => Ok(Arc::new(StdioTransport::new(
                command.clone(),
                args.clone(),
                env.clone(),
                env_secret_refs.clone(),
                Arc::clone(&self.resolver),
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
            attach_request_metadata(&mut params, env!("CARGO_PKG_VERSION"));
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
        let transport = self.build_transport(&runtime.config)?;
        let cancel = CancellationToken::new();
        let negotiation = transport.connect(&cancel).await?;

        // Capability-gated catalog discovery: never send an unsupported RPC.
        let tools = if negotiation.capabilities.tools {
            self.tools_list_wire(&*transport, negotiation.protocol_version)
                .await?
        } else {
            Vec::new()
        };
        let resources = if negotiation.capabilities.resources {
            self.resources_list_wire(&*transport, negotiation.protocol_version)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let resource_templates = if negotiation.capabilities.resources {
            self.resource_templates_wire(&*transport, negotiation.protocol_version)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let prompts = if negotiation.capabilities.prompts {
            self.prompts_list_wire(&*transport, negotiation.protocol_version)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        // Rebuild the runtime entry with the real negotiated version + catalog.
        let refreshed = Arc::new(McpServerRuntime {
            server_id: runtime.server_id.clone(),
            name: runtime.name.clone(),
            config: runtime.config.clone(),
            protocol_version: negotiation.protocol_version,
            status: McpRuntimeStatus::Ready,
            capabilities: negotiation.capabilities,
            tools,
            resources,
            resource_templates,
            prompts,
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
            for mut tool in tools {
                match scan_tool_header_bindings(&tool.input_schema) {
                    Ok(bindings) => {
                        tool.header_bindings = bindings;
                        all.push(tool);
                    }
                    Err(_) => {
                        tracing::warn!(tool = %tool.name, "excluding tool with invalid x-mcp-header");
                    }
                }
            }
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

    async fn resources_list_wire(
        &self,
        transport: &dyn McpTransport,
        protocol_version: McpProtocolVersion,
    ) -> Result<Vec<McpResourceDescriptor>, McpRuntimeError> {
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
                    transport,
                    "resources/list",
                    params,
                    protocol_version,
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

    async fn resource_templates_wire(
        &self,
        transport: &dyn McpTransport,
        protocol_version: McpProtocolVersion,
    ) -> Result<Vec<McpResourceTemplate>, McpRuntimeError> {
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
                    transport,
                    "resources/templates/list",
                    params,
                    protocol_version,
                    &cancel,
                )
                .await?;
            let (templates, next) = parse_resource_template_list(&result)?;
            all.extend(templates);
            match next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        all.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(all)
    }

    async fn prompts_list_wire(
        &self,
        transport: &dyn McpTransport,
        protocol_version: McpProtocolVersion,
    ) -> Result<Vec<McpPromptDescriptor>, McpRuntimeError> {
        let cancel = CancellationToken::new();
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_MCP_LIST_PAGES {
            let mut params = serde_json::json!({});
            if let Some(c) = &cursor {
                params["cursor"] = serde_json::json!(c);
            }
            let result = self
                .send(transport, "prompts/list", params, protocol_version, &cancel)
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

    pub async fn list_tools(&self, server_id: &str) -> Result<Vec<McpTool>, McpRuntimeError> {
        let runtime = self
            .get_server(server_id)
            .ok_or(McpRuntimeError::ServerNotFound)?;
        if !runtime.capabilities.tools {
            return Err(McpRuntimeError::CapabilityUnsupported("tools".to_string()));
        }
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
        if !runtime.capabilities.tools {
            return Err(McpRuntimeError::CapabilityUnsupported("tools".to_string()));
        }
        let transport = runtime
            .transport
            .as_ref()
            .ok_or(McpRuntimeError::Transport("not connected".to_string()))?;
        // Unknown tool fails closed (agents may only call discovered catalog tools).
        if !runtime.tools.iter().any(|t| t.name == tool_name) {
            return Err(McpRuntimeError::ToolNotFound(tool_name.to_string()));
        }
        // x-mcp-header: derive Mcp-Param-* headers from the tool's bindings.
        let extra_headers = runtime
            .tools
            .iter()
            .find(|t| t.name == tool_name)
            .map(|tool| {
                super::header_schema::extract_header_values(&arguments, &tool.header_bindings).map(
                    |values| {
                        values
                            .into_iter()
                            .map(|(name, value)| (super::protocol::param_header(&name), value))
                            .collect::<std::collections::BTreeMap<_, _>>()
                    },
                )
            })
            .transpose()
            .map_err(|e| McpRuntimeError::Protocol(e))?
            .unwrap_or_default();

        let mut params = serde_json::json!({ "name": tool_name, "arguments": arguments });
        if runtime.protocol_version == McpProtocolVersion::V2026_07_28 {
            attach_request_metadata(&mut params, env!("CARGO_PKG_VERSION"));
        }
        let request = JsonRpcRequest::new(self.next_id(), "tools/call", Some(params));
        let options = super::transport::McpRequestOptions { extra_headers };
        let message = transport
            .send_with_options(&request, &options, cancel)
            .await?;
        let result = match message {
            JsonRpcMessage::Success(s) => s.result,
            JsonRpcMessage::Error(e) => return Err(McpRuntimeError::ServerError(e.error.message)),
            JsonRpcMessage::Notification(_) => return Err(McpRuntimeError::InvalidResponse),
        };
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
        if !runtime.capabilities.resources {
            return Err(McpRuntimeError::CapabilityUnsupported(
                "resources".to_string(),
            ));
        }
        Ok(runtime.resources.clone())
    }

    pub async fn list_resource_templates(
        &self,
        server_id: &str,
    ) -> Result<Vec<McpResourceTemplate>, McpRuntimeError> {
        let runtime = self
            .get_server(server_id)
            .ok_or(McpRuntimeError::ServerNotFound)?;
        if !runtime.capabilities.resources {
            return Err(McpRuntimeError::CapabilityUnsupported(
                "resources".to_string(),
            ));
        }
        Ok(runtime.resource_templates.clone())
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
        if !runtime.capabilities.resources {
            return Err(McpRuntimeError::CapabilityUnsupported(
                "resources".to_string(),
            ));
        }
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
                let (ttl_ms, scope) = parse_cache_hint(&result);
                if ttl_ms > 0 {
                    self.cache.put(
                        &cache_key,
                        super::cache::McpCacheValue::ResourceRead(contents.clone()),
                        ttl_ms,
                        scope,
                        Self::now_ms(),
                    );
                }
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
        if !runtime.capabilities.prompts {
            return Err(McpRuntimeError::CapabilityUnsupported(
                "prompts".to_string(),
            ));
        }
        Ok(runtime.prompts.clone())
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
        if !runtime.capabilities.prompts {
            return Err(McpRuntimeError::CapabilityUnsupported(
                "prompts".to_string(),
            ));
        }
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

    /// Shut down one server's transport + invalidate its cache.
    pub async fn shutdown_server(&self, server_id: &str) {
        if let Some(runtime) = self.get_server(server_id) {
            if let Some(transport) = &runtime.transport {
                transport.shutdown().await;
            }
        }
        self.invalidate_server_cache(server_id);
    }

    /// Shut down and remove a server from the manager.
    pub async fn remove_server(&self, server_id: &str) {
        self.shutdown_server(server_id).await;
        self.servers.write().remove(server_id);
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
