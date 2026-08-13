// ============================================================
// McpToolAdapter — safe namespaced Tool-trait adapter for MCP tools.
//
// Converts externally-discovered MCP tool metadata into a
// [`Tool`](crate::tools::trait_def::Tool) so it can be listed and
// registered in a temporary ToolRegistry and exposed as an
// OpenAI-compatible tool definition.
//
// Deliberately fail-closed:
//   - risk_level is always High (remote side effects are unknown)
//   - execute() always errors (tools/call is not implemented yet)
//   - security_descriptor() keeps the default UnknownTool fail-closed
//     behaviour — a namespaced `mcp_*` name is not a builtin tool.
// ============================================================

use async_trait::async_trait;

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
/// name that must be sent verbatim in a future `tools/call`.
#[derive(Debug)]
pub struct McpToolAdapter {
    exposed_name: String,
    server_id: String,
    server_name: String,
    remote_tool_name: String,
    description: String,
    input_schema: serde_json::Value,
}

impl McpToolAdapter {
    pub fn new(server: &crate::db::McpServer, tool: &McpTool) -> Result<Self, McpToolAdapterError> {
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

    async fn execute(&self, _args: serde_json::Value) -> ToolResult {
        ToolResult::error("MCP tool execution is not enabled yet")
    }
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
    async fn execute_fails_closed() {
        let server = mcp_server("550e8400-e29b-41d4-a716-446655440000", "fs");
        let tool = mcp_tool("search", None, serde_json::json!({"type": "object"}));
        let adapter = McpToolAdapter::new(&server, &tool).unwrap();

        let result = adapter.execute(serde_json::json!({"q": "x"})).await;
        assert!(!result.ok);
        assert!(result.content.contains("not enabled"));
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
}
