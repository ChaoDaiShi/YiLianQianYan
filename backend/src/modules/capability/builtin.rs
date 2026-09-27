// ============================================================
// Tool-registry-backed providers: Builtin tools + MCP tools.
//
// These map the *names* in the agent tool registry into descriptors. They
// never execute anything and never invoke MCP `tools/call`.
// ============================================================

use std::sync::Arc;

use async_trait::async_trait;

use super::model::{
    CapabilityDescriptor, CapabilityId, CapabilityKind, CapabilityMetadata, CapabilityPermission,
    CapabilityProviderKind, CapabilityRisk, CapabilityRuntimeStatus,
};
use super::provider::{CapabilityProvider, CapabilityProviderError};
use crate::tools::registry::ToolRegistry;
use crate::tools::trait_def::RiskLevel;

fn map_risk(risk: RiskLevel) -> CapabilityRisk {
    match risk {
        RiskLevel::Low => CapabilityRisk::Low,
        RiskLevel::Medium => CapabilityRisk::Medium,
        RiskLevel::High | RiskLevel::Critical => CapabilityRisk::High,
    }
}

fn tool_descriptor(
    id: &str,
    kind: CapabilityKind,
    provider: CapabilityProviderKind,
    info: &crate::tools::trait_def::ToolInfo,
) -> CapabilityDescriptor {
    CapabilityDescriptor {
        id: CapabilityId::new(id).expect("tool capability id is valid"),
        kind,
        provider,
        name: info.name.clone(),
        description: info.description.clone(),
        input_schema: Some(info.parameters.clone()),
        risk: map_risk(info.risk_level),
        permissions: Vec::<CapabilityPermission>::new(),
        status: CapabilityRuntimeStatus::Ready,
        enabled: true,
        metadata: CapabilityMetadata {
            source_id: Some(info.name.clone()),
            source_name: Some(info.name.clone()),
            tags: vec![],
            runtime_ready: true,
            ..Default::default()
        },
    }
}

/// Discovers built-in tools (excluding `mcp_*` and `subagent_*` adapters).
pub struct BuiltinToolProvider {
    registry: Arc<ToolRegistry>,
}

impl BuiltinToolProvider {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl CapabilityProvider for BuiltinToolProvider {
    fn provider_kind(&self) -> CapabilityProviderKind {
        CapabilityProviderKind::Builtin
    }

    async fn discover(&self) -> Result<Vec<CapabilityDescriptor>, CapabilityProviderError> {
        let descriptors = self
            .registry
            .list_tools()
            .into_iter()
            .filter(|info| !info.name.starts_with("mcp_") && !info.name.starts_with("subagent_"))
            .map(|info| {
                tool_descriptor(
                    &format!("builtin.tool.{}", info.name),
                    CapabilityKind::Tool,
                    CapabilityProviderKind::Builtin,
                    &info,
                )
            })
            .collect();
        Ok(descriptors)
    }
}

/// Discovers MCP tools (the `mcp_*` adapters already registered in the runtime
/// registry, which only contains servers that probed successfully).
pub struct McpToolProvider {
    registry: Arc<ToolRegistry>,
}

impl McpToolProvider {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl CapabilityProvider for McpToolProvider {
    fn provider_kind(&self) -> CapabilityProviderKind {
        CapabilityProviderKind::Mcp
    }

    async fn discover(&self) -> Result<Vec<CapabilityDescriptor>, CapabilityProviderError> {
        let descriptors = self
            .registry
            .list_tools()
            .into_iter()
            .filter(|info| info.name.starts_with("mcp_"))
            .map(|info| {
                let remote = info
                    .name
                    .strip_prefix("mcp_")
                    .unwrap_or(&info.name)
                    .to_string();
                let mut descriptor = tool_descriptor(
                    &format!("mcp.{remote}"),
                    CapabilityKind::McpTool,
                    CapabilityProviderKind::Mcp,
                    &info,
                );
                // Display metadata only; source_id remains the exact trusted
                // registry binding, even for identically named remote tools.
                if let Some(tool) = self.registry.get(&info.name) {
                    if let Ok(security) = tool.security_descriptor(&serde_json::json!({})) {
                        if let Some(crate::safety::ResourceDescriptor::Mcp {
                            server_id,
                            tool_name,
                        }) = security.resources.first()
                        {
                            descriptor.name = tool_name.clone();
                            descriptor.metadata.source_name = Some(server_id.clone());
                        }
                    }
                }
                if let Some(rest) = info.description.strip_prefix("[MCP:") {
                    if let Some((source, description)) = rest.split_once(']') {
                        descriptor.metadata.source_name = Some(source.to_string());
                        descriptor.description = description.trim().to_string();
                    }
                }
                descriptor.permissions.push(CapabilityPermission {
                    permission: "mcp.invoke".into(),
                    required: true,
                });
                descriptor
            })
            .collect();
        Ok(descriptors)
    }
}
