// ============================================================
// McpToolAdapter — safe namespaced Tool-trait adapter for MCP tools.
//
// Converts externally-discovered MCP tool metadata into a
// [`Tool`](crate::tools::trait_def::Tool) so it can be listed and
// registered in a temporary ToolRegistry and exposed as an
// OpenAI-compatible tool definition.
//
// Security posture:
//   - risk_level is always High (remote side effects are unknown)
//   - security_descriptor() emits an MCP-specific descriptor
//     (McpInvoke / McpServer / High / ExternalService), validated through
//     the standard profile chain — no bypass.
//   - execute() calls the real stdio executor. Approval decisions remain the
//     SecurityExecutionGateway's responsibility; the adapter never evaluates
//     roles itself.
//
// The adapter retains a private clone of the full [`McpServer`] config for
// execution. It is never Serialized, and Debug output omits the server config
// (which may contain command / env / secrets).
// ============================================================

use async_trait::async_trait;

use crate::db::McpServer;
use crate::mcp::McpTool;
use crate::safety::{
    DescriptorError, PermissionId, ResourceDescriptor, ResourceScope, SideEffectKind,
    ToolSecurityDescriptor,
};
use crate::tools::trait_def::{RiskLevel, Tool, ToolResult};

/// Maximum length of the final exposed tool name.
const MAX_EXPOSED_TOOL_NAME_LEN: usize = 64;
/// Maximum description characters (UTF-8 safe truncation).
const MAX_MCP_TOOL_DESCRIPTION_CHARS: usize = 1000;
/// Length of the server namespace prefix derived from `server.id`.
const SERVER_NAMESPACE_LEN: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum McpToolAdapterError {
    #[error("MCP server id is empty")]
    EmptyServerId,
    #[error("MCP tool name is empty")]
    EmptyToolName,
    #[error("MCP tool name cannot be safely exposed")]
    InvalidToolName,
    #[error("MCP inputSchema must be a JSON object")]
    InvalidInputSchema,
    #[error("MCP exposed tool name collides with an existing name: {0}")]
    NameCollision(String),
}

/// A Tool-trait adapter around a single remote MCP tool.
///
/// `exposed_name` is what the LLM / Agent sees; `remote_tool_name` is the
/// name that must be sent verbatim in a future `tools/call`. The full
/// `server` config is kept privately for execution.
pub struct McpToolAdapter {
    exposed_name: String,
    server_id: String,
    server_name: String,
    remote_tool_name: String,
    description: String,
    input_schema: serde_json::Value,
    server: McpServer,
}

// Manual Debug: never print the full server config (may contain command/env/secrets).
impl std::fmt::Debug for McpToolAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpToolAdapter")
            .field("exposed_name", &self.exposed_name)
            .field("server_id", &self.server_id)
            .field("server_name", &self.server_name)
            .field("remote_tool_name", &self.remote_tool_name)
            .field("description", &self.description)
            .field("input_schema", &self.input_schema)
            .finish()
    }
}

impl McpToolAdapter {
    pub fn new(server: &McpServer, tool: &McpTool) -> Result<Self, McpToolAdapterError> {
        if server.id.trim().is_empty() {
            return Err(McpToolAdapterError::EmptyServerId);
        }
        let remote_tool_name = tool.name.trim();
        if remote_tool_name.is_empty() {
            return Err(McpToolAdapterError::EmptyToolName);
        }
        if !tool.input_schema.is_object() {
            return Err(McpToolAdapterError::InvalidInputSchema);
        }

        let server_ns = derive_server_namespace(&server.id)?;
        let tool_ns =
            sanitize_tool_name(remote_tool_name).ok_or(McpToolAdapterError::InvalidToolName)?;
        let exposed_name = build_exposed_name(&server_ns, &tool_ns)?;

        // Description: prefix with an MCP marker, truncate the raw description first.
        let fallback_desc = format!("MCP tool {remote_tool_name} from {}", server.name);
        let raw_desc = tool.description.as_deref().unwrap_or(&fallback_desc);
        let truncated =
            crate::utils::text::truncate_chars(raw_desc, MAX_MCP_TOOL_DESCRIPTION_CHARS);
        let description = format!("[MCP:{}] {}", server.name, truncated);

        Ok(Self {
            exposed_name,
            server_id: server.id.clone(),
            server_name: server.name.clone(),
            remote_tool_name: remote_tool_name.to_string(),
            description,
            input_schema: tool.input_schema.clone(),
            server: server.clone(),
        })
    }

    /// Construct an adapter while enforcing exposed-name uniqueness against
    /// an accumulator of already-occupied names.
    ///
    /// This is the boundary where production registrations should guard
    /// against collisions (including built-in tool names pre-populated into
    /// `occupied_names`). Returns [`McpToolAdapterError::NameCollision`] if
    /// the derived exposed name is already taken.
    pub fn new_unique(
        server: &crate::db::McpServer,
        tool: &McpTool,
        occupied_names: &mut std::collections::HashSet<String>,
    ) -> Result<Self, McpToolAdapterError> {
        let adapter = Self::new(server, tool)?;
        if !occupied_names.insert(adapter.name().to_string()) {
            return Err(McpToolAdapterError::NameCollision(
                adapter.name().to_string(),
            ));
        }
        Ok(adapter)
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    pub fn remote_tool_name(&self) -> &str {
        &self.remote_tool_name
    }
}

/// Derive a short ASCII-numeric namespace from a server id.
/// Takes up to `SERVER_NAMESPACE_LEN` alphanumeric characters.
fn derive_server_namespace(server_id: &str) -> Result<String, McpToolAdapterError> {
    let ns: String = server_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(SERVER_NAMESPACE_LEN)
        .collect();
    if ns.is_empty() {
        Err(McpToolAdapterError::InvalidToolName)
    } else {
        Ok(ns)
    }
}

/// Sanitize a remote tool name into a stable ASCII identifier.
/// Returns `None` if nothing usable remains.
fn sanitize_tool_name(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    let mut prev_underscore = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Build `mcp_<server>_<tool>`, truncating the tool part if needed.
fn build_exposed_name(server_ns: &str, tool_ns: &str) -> Result<String, McpToolAdapterError> {
    let prefix = format!("mcp_{server_ns}_");
    let budget = MAX_EXPOSED_TOOL_NAME_LEN.saturating_sub(prefix.len());
    if budget == 0 {
        return Err(McpToolAdapterError::InvalidToolName);
    }
    let tool_part: String = tool_ns.chars().take(budget).collect();
    let tool_part = tool_part.trim_matches('_');
    if tool_part.is_empty() {
        return Err(McpToolAdapterError::InvalidToolName);
    }
    Ok(format!("{prefix}{tool_part}"))
}

#[async_trait]
impl Tool for McpToolAdapter {
    fn name(&self) -> &str {
        &self.exposed_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.input_schema.clone()
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    /// Produce an MCP-specific security descriptor.
    ///
    /// All MCP tools are treated as High-risk external-service invocations.
    /// The server / remote tool names come from the adapter's trusted binding,
    /// never from arguments or remote metadata. The descriptor is validated
    /// through the standard profile chain (no bypass).
    fn security_descriptor(
        &self,
        _args: &serde_json::Value,
    ) -> Result<ToolSecurityDescriptor, DescriptorError> {
        let descriptor = ToolSecurityDescriptor {
            tool_name: self.name().to_string(),
            requested_permissions: vec![PermissionId::McpInvoke.in_scope(ResourceScope::McpServer)],
            resources: vec![ResourceDescriptor::Mcp {
                server_id: self.server_id.clone(),
                tool_name: self.remote_tool_name.clone(),
            }],
            default_risk: RiskLevel::High,
            side_effects: vec![SideEffectKind::ExternalService],
        };
        descriptor.validate_for_tool(self.name())?;
        Ok(descriptor)
    }

    /// Execute the remote tool via the stdio executor.
    ///
    /// Protocol/execution errors are converted to a bounded [`ToolResult`]
    /// (never a panic, never propagated as an `Err`). Approval decisions are
    /// handled upstream by the SecurityExecutionGateway.
    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        match crate::mcp::call_stdio_tool(&self.server, &self.remote_tool_name, args).await {
            Ok(result) => result,
            Err(error) => {
                let safe = crate::utils::text::truncate_chars(&error.to_string(), 500);
                ToolResult::error(format!("MCP tool execution failed: {safe}"))
            }
        }
    }
}

/// Register a batch of discovered MCP tools into a runtime registry snapshot.
///
/// Pure and side-effect free: does NOT spawn, connect, or execute anything.
/// Each tool is wrapped in a namespaced [`McpToolAdapter`] via `new_unique`.
///
/// Returns the number of tools successfully registered. Individual tools with
/// invalid metadata or a name collision are skipped (logged as debug) without
/// failing the rest of the batch.
pub fn register_discovered_mcp_tools(
    registry: &mut crate::tools::ToolRegistry,
    occupied_names: &mut std::collections::HashSet<String>,
    server: &McpServer,
    tools: &[McpTool],
) -> usize {
    let mut registered = 0usize;
    for tool in tools {
        match McpToolAdapter::new_unique(server, tool, occupied_names) {
            Ok(adapter) => {
                registry.register(std::sync::Arc::new(adapter));
                registered += 1;
            }
            Err(e) => {
                tracing::debug!(
                    server_id = %server.id,
                    tool_name = %tool.name,
                    error = %e,
                    "skipping MCP tool with invalid metadata or name collision"
                );
            }
        }
    }
    registered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::McpServer;
    use crate::safety::{PermissionId, ResourceDescriptor, ResourceScope, SideEffectKind};

    fn mcp_server(id: &str, name: &str) -> McpServer {
        McpServer {
            id: id.to_string(),
            name: name.to_string(),
            transport: "stdio".to_string(),
            command: Some("echo".to_string()),
            args: None,
            url: None,
            env: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn mcp_tool(name: &str, description: Option<&str>, schema: serde_json::Value) -> McpTool {
        McpTool {
            name: name.to_string(),
            description: description.map(str::to_string),
            input_schema: schema,
        }
    }

    #[test]
    fn adapter_builds_namespaced_name() {
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "filesystem");
        let tool = mcp_tool(
            "read-file",
            Some("Read a file"),
            serde_json::json!({"type": "object"}),
        );
        let adapter = McpToolAdapter::new(&server, &tool).unwrap();
        assert_eq!(adapter.name(), "mcp_550e8400_read_file");
        assert_eq!(adapter.remote_tool_name(), "read-file");
        assert_eq!(adapter.server_id(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn adapter_does_not_collide_with_builtin_name() {
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "fs");
        let tool = mcp_tool("read_file", None, serde_json::json!({"type": "object"}));
        let adapter = McpToolAdapter::new(&server, &tool).unwrap();
        assert_ne!(adapter.name(), "read_file");
        assert!(adapter.name().starts_with("mcp_550e8400_"));
    }

    #[test]
    fn two_servers_same_tool_do_not_collide() {
        let server_a = mcp_server("aaaaaaaa-0000-0000-0000-000000000000", "A");
        let server_b = mcp_server("bbbbbbbb-0000-0000-0000-000000000000", "B");
        let tool = mcp_tool("search", None, serde_json::json!({"type": "object"}));
        let a = McpToolAdapter::new(&server_a, &tool).unwrap();
        let b = McpToolAdapter::new(&server_b, &tool).unwrap();
        assert_ne!(a.name(), b.name());
        assert_eq!(a.name(), "mcp_aaaaaaaa_search");
        assert_eq!(b.name(), "mcp_bbbbbbbb_search");
    }

    #[test]
    fn description_has_mcp_prefix() {
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "filesystem");
        let tool = mcp_tool(
            "read-file",
            Some("Read a file"),
            serde_json::json!({"type": "object"}),
        );
        let adapter = McpToolAdapter::new(&server, &tool).unwrap();
        assert!(adapter.description().starts_with("[MCP:filesystem] "));
        assert!(adapter.description().contains("Read a file"));
    }

    #[test]
    fn description_is_truncated() {
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "filesystem");
        let long = "长".repeat(3000);
        let tool = mcp_tool(
            "read-file",
            Some(&long),
            serde_json::json!({"type": "object"}),
        );
        let adapter = McpToolAdapter::new(&server, &tool).unwrap();
        // Raw description truncated to <= 1000 chars (plus "..." ellipsis),
        // then the MCP prefix is prepended.
        let prefix_len = "[MCP:filesystem] ".chars().count();
        assert!(adapter.description().chars().count() <= 1000 + 3 + prefix_len);
        assert!(!adapter.description().contains(&"长".repeat(1001)));
    }

    #[test]
    fn input_schema_is_preserved() {
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "fs");
        let schema = serde_json::json!({
            "type": "object",
            "properties": { "query": { "type": "string" } },
            "required": ["query"]
        });
        let tool = mcp_tool("search", None, schema.clone());
        let adapter = McpToolAdapter::new(&server, &tool).unwrap();
        assert_eq!(adapter.parameters(), schema);
    }

    #[test]
    fn non_object_schema_is_rejected() {
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "fs");
        for bad in [serde_json::json!([]), serde_json::json!("abc")] {
            let tool = mcp_tool("x", None, bad);
            assert!(matches!(
                McpToolAdapter::new(&server, &tool),
                Err(McpToolAdapterError::InvalidInputSchema)
            ));
        }
    }

    #[test]
    fn unrepresentable_tool_name_is_rejected() {
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "fs");
        let tool = mcp_tool("中文工具", None, serde_json::json!({"type": "object"}));
        assert!(matches!(
            McpToolAdapter::new(&server, &tool),
            Err(McpToolAdapterError::InvalidToolName)
        ));
    }

    #[test]
    fn empty_server_id_is_rejected() {
        let server = mcp_server("", "fs");
        let tool = mcp_tool("search", None, serde_json::json!({"type": "object"}));
        assert!(matches!(
            McpToolAdapter::new(&server, &tool),
            Err(McpToolAdapterError::EmptyServerId)
        ));
    }

    #[test]
    fn empty_tool_name_is_rejected() {
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "fs");
        let tool = mcp_tool("", None, serde_json::json!({"type": "object"}));
        assert!(matches!(
            McpToolAdapter::new(&server, &tool),
            Err(McpToolAdapterError::EmptyToolName)
        ));
    }

    #[test]
    fn adapter_can_be_registered_in_temporary_registry() {
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "fs");
        let tool = mcp_tool("search", None, serde_json::json!({"type": "object"}));
        let adapter = McpToolAdapter::new(&server, &tool).unwrap();

        let mut registry = crate::tools::ToolRegistry::new();
        registry.register(std::sync::Arc::new(adapter));
        assert!(registry.get("mcp_550e8400_search").is_some());
        assert_eq!(registry.list_tools().len(), 1);
    }

    #[test]
    fn openai_tool_definition_matches() {
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "filesystem");
        let schema =
            serde_json::json!({"type": "object", "properties": { "q": {"type": "string"} }});
        let tool = mcp_tool("search", Some("Search"), schema.clone());
        let adapter = McpToolAdapter::new(&server, &tool).unwrap();

        let def = adapter.to_openai_tool();
        assert_eq!(def["function"]["name"], "mcp_550e8400_search");
        assert!(def["function"]["description"]
            .as_str()
            .unwrap()
            .contains("[MCP:filesystem]"));
        assert_eq!(def["function"]["parameters"], schema);
    }

    #[test]
    fn risk_level_is_high_and_requires_approval() {
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "fs");
        let tool = mcp_tool("search", None, serde_json::json!({"type": "object"}));
        let adapter = McpToolAdapter::new(&server, &tool).unwrap();
        assert_eq!(adapter.risk_level(), RiskLevel::High);
        assert!(adapter.requires_approval());
    }

    #[tokio::test]
    async fn execute_is_wired_to_stdio_executor() {
        // command = None → call_stdio_tool rejects config before spawning.
        let mut server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "fs");
        server.command = None;
        let tool = mcp_tool("search", None, serde_json::json!({"type": "object"}));
        let adapter = McpToolAdapter::new(&server, &tool).unwrap();

        let result = adapter.execute(serde_json::json!({"q": "x"})).await;
        assert!(!result.ok);
        // Proves execute() now goes through the real executor and converts
        // the McpError into a bounded ToolResult.
        assert!(result.content.contains("MCP tool execution failed"));
        assert!(!result.content.contains("not enabled"));
    }

    #[tokio::test]
    async fn execute_never_panics_and_converts_errors_to_tool_result() {
        // A missing command is only one failure mode; the key is that the
        // Tool trait boundary never sees a Rust Err / panic.
        let mut server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "fs");
        server.command = None;
        let tool = mcp_tool("search", None, serde_json::json!({"type": "object"}));
        let adapter = McpToolAdapter::new(&server, &tool).unwrap();

        let result = adapter.execute(serde_json::json!({})).await;
        assert!(!result.ok);
        assert!(result.error.is_some());
    }

    #[test]
    fn security_descriptor_is_valid_mcp_descriptor() {
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "fs");
        let tool = mcp_tool("search", None, serde_json::json!({"type": "object"}));
        let adapter = McpToolAdapter::new(&server, &tool).unwrap();

        let desc = adapter
            .security_descriptor(&serde_json::json!({"q": "x"}))
            .unwrap();
        assert_eq!(desc.tool_name, adapter.name());
        assert_eq!(
            desc.requested_permissions,
            vec![PermissionId::McpInvoke.in_scope(ResourceScope::McpServer)]
        );
        assert_eq!(
            desc.resources,
            vec![ResourceDescriptor::Mcp {
                server_id: adapter.server_id().to_string(),
                tool_name: adapter.remote_tool_name().to_string(),
            }]
        );
        assert_eq!(desc.default_risk, RiskLevel::High);
        assert_eq!(desc.side_effects, vec![SideEffectKind::ExternalService]);
        // Must pass standard validation (no bypass).
        assert!(desc.validate_for_tool(adapter.name()).is_ok());
    }

    #[test]
    fn new_unique_detects_name_collision() {
        use std::collections::HashSet;
        let server_a = mcp_server("abcdef12-1111-0000-0000-000000000000", "A");
        let server_b = mcp_server("abcdef12-2222-0000-0000-000000000000", "B");
        let tool = mcp_tool("search", None, serde_json::json!({"type": "object"}));

        let mut occupied = HashSet::new();
        // First insert succeeds.
        McpToolAdapter::new_unique(&server_a, &tool, &mut occupied).unwrap();
        // Same 8-char namespace + same tool name collides.
        let err = McpToolAdapter::new_unique(&server_b, &tool, &mut occupied).unwrap_err();
        assert!(matches!(err, McpToolAdapterError::NameCollision(_)));
    }

    #[test]
    fn new_unique_allows_distinct_namespaces() {
        use std::collections::HashSet;
        let server_a = mcp_server("aaaaaaaa-0000-0000-0000-000000000000", "A");
        let server_b = mcp_server("bbbbbbbb-0000-0000-0000-000000000000", "B");
        let tool = mcp_tool("search", None, serde_json::json!({"type": "object"}));

        let mut occupied = HashSet::new();
        McpToolAdapter::new_unique(&server_a, &tool, &mut occupied).unwrap();
        McpToolAdapter::new_unique(&server_b, &tool, &mut occupied).unwrap();
    }

    #[test]
    fn new_unique_detects_builtin_name_collision() {
        use std::collections::HashSet;
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "fs");
        let tool = mcp_tool("search", None, serde_json::json!({"type": "object"}));

        let mut occupied = HashSet::new();
        // Simulate an already-registered builtin with the same exposed name.
        occupied.insert("mcp_550e8400_search".to_string());
        let err = McpToolAdapter::new_unique(&server, &tool, &mut occupied).unwrap_err();
        assert!(matches!(err, McpToolAdapterError::NameCollision(_)));
    }

    // ── register_discovered_mcp_tools (pure, no spawn) ──

    use std::collections::HashSet;

    fn register_helper(
        server: &McpServer,
        tools: &[McpTool],
        occupied: &mut HashSet<String>,
    ) -> (crate::tools::ToolRegistry, usize) {
        let mut registry = crate::tools::ToolRegistry::new();
        let n = register_discovered_mcp_tools(&mut registry, occupied, server, tools);
        (registry, n)
    }

    #[test]
    fn registry_helper_preserves_builtins_and_adds_mcp() {
        let mut registry = crate::tools::ToolRegistry::with_defaults(".");
        let mut occupied: HashSet<String> = registry
            .list_tools()
            .into_iter()
            .map(|info| info.name)
            .collect();
        let server = mcp_server("aaaaaaaa-0000-0000-0000-000000000000", "fs");
        let tools = vec![mcp_tool(
            "search",
            None,
            serde_json::json!({"type": "object"}),
        )];

        let n = register_discovered_mcp_tools(&mut registry, &mut occupied, &server, &tools);
        assert_eq!(n, 1);
        // Builtins preserved.
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("bash").is_some());
        // MCP tool present.
        assert!(registry.get("mcp_aaaaaaaa_search").is_some());
        // And exposed as an OpenAI function definition.
        let openai_tools = registry.to_openai_tools();
        let names: Vec<&str> = openai_tools
            .iter()
            .filter_map(|t| t["function"]["name"].as_str())
            .collect();
        assert!(names.contains(&"mcp_aaaaaaaa_search"));
    }

    #[test]
    fn registry_helper_registers_multiple_tools_from_one_server() {
        let mut occupied = HashSet::new();
        let server = mcp_server("aaaaaaaa-0000-0000-0000-000000000000", "fs");
        let tools = vec![
            mcp_tool("search", None, serde_json::json!({"type": "object"})),
            mcp_tool("read", None, serde_json::json!({"type": "object"})),
        ];
        let (registry, n) = register_helper(&server, &tools, &mut occupied);
        assert_eq!(n, 2);
        assert!(registry.get("mcp_aaaaaaaa_search").is_some());
        assert!(registry.get("mcp_aaaaaaaa_read").is_some());
    }

    #[test]
    fn registry_helper_handles_cross_server_same_tool() {
        let mut occupied = HashSet::new();
        let server_a = mcp_server("aaaaaaaa-0000-0000-0000-000000000000", "A");
        let server_b = mcp_server("bbbbbbbb-0000-0000-0000-000000000000", "B");
        let tools = vec![mcp_tool(
            "search",
            None,
            serde_json::json!({"type": "object"}),
        )];
        let (reg_a, n_a) = register_helper(&server_a, &tools, &mut occupied);
        let (reg_b, n_b) = register_helper(&server_b, &tools, &mut occupied);
        assert_eq!(n_a, 1);
        assert_eq!(n_b, 1);
        assert!(reg_a.get("mcp_aaaaaaaa_search").is_some());
        assert!(reg_b.get("mcp_bbbbbbbb_search").is_some());
    }

    #[test]
    fn registry_helper_collision_is_skipped_not_overwritten() {
        let mut occupied = HashSet::new();
        let server_a = mcp_server("abcdef12-1111-0000-0000-000000000000", "A");
        let server_b = mcp_server("abcdef12-2222-0000-0000-000000000000", "B");
        let tools = vec![mcp_tool(
            "search",
            None,
            serde_json::json!({"type": "object"}),
        )];

        let (reg_a, n_a) = register_helper(&server_a, &tools, &mut occupied);
        assert_eq!(n_a, 1);
        assert!(reg_a.get("mcp_abcdef12_search").is_some());

        // Second server with same 8-char namespace + same tool → collision → skipped.
        let (reg_b, n_b) = register_helper(&server_b, &tools, &mut occupied);
        assert_eq!(n_b, 0);
        assert!(reg_b.get("mcp_abcdef12_search").is_none());
    }

    #[test]
    fn registry_helper_invalid_metadata_skips_only_that_tool() {
        let mut occupied = HashSet::new();
        let server = mcp_server("aaaaaaaa-0000-0000-0000-000000000000", "fs");
        let tools = vec![
            mcp_tool("valid_search", None, serde_json::json!({"type": "object"})),
            mcp_tool("中文工具", None, serde_json::json!({"type": "object"})),
            mcp_tool("valid_read", None, serde_json::json!({"type": "object"})),
        ];
        let (registry, n) = register_helper(&server, &tools, &mut occupied);
        assert_eq!(n, 2);
        assert!(registry.get("mcp_aaaaaaaa_valid_search").is_some());
        assert!(registry.get("mcp_aaaaaaaa_valid_read").is_some());
        // The invalid (unrepresentable) tool was skipped, not fatal.
        assert_eq!(registry.list_tools().len(), 2);
    }
}
