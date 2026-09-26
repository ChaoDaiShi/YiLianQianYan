// ============================================================
// Unified Capability Model — describes and discovers runtime abilities.
//
// A CapabilityDescriptor answers "what capabilities exist right now". It is a
// *discovery/display* layer only: it never grants permission and never executes
// anything. Real authorization remains with the Security Execution Gateway and
// real execution with the existing runtimes.
// ============================================================

use serde::{Deserialize, Serialize};

/// Bounds enforced on descriptors before they enter the registry.
pub const MAX_CAPABILITY_DESCRIPTION_CHARS: usize = 2000;
pub const MAX_CAPABILITY_TAGS: usize = 32;
pub const MAX_INPUT_SCHEMA_CHARS: usize = 32_000;
pub const MAX_METADATA_CHARS: usize = 16_000;
pub const MAX_PERMISSIONS: usize = 32;

/// A stable, namespaced capability identifier (e.g. `builtin.tool.read_file`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityId(String);

impl CapabilityId {
    pub fn new(raw: impl Into<String>) -> Result<Self, CapabilityModelError> {
        let raw = raw.into();
        if raw.is_empty() || raw.chars().any(char::is_control) {
            return Err(CapabilityModelError::InvalidId);
        }
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    Tool,
    McpTool,
    Subagent,
    Agent,
    Workflow,
    Skill,
    McpResource,
    McpResourceTemplate,
    McpPrompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityProviderKind {
    Builtin,
    Mcp,
    Subagent,
    AgentRuntime,
    WorkflowRuntime,
    SkillRuntime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRuntimeStatus {
    Ready,
    Unavailable,
    Disabled,
    Misconfigured,
    Degraded,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRisk {
    Low,
    Medium,
    High,
    /// Composite capabilities (agent / workflow / subagent) whose real risk is
    /// decided dynamically by the Security Gateway at execution time.
    Dynamic,
}

/// A stable, display-only permission hint. Real authorization is RBAC via the
/// Security Gateway — this is never a grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityPermission {
    pub permission: String,
    pub required: bool,
}

/// Bounded, secret-free metadata. `extra` is capped and must never contain
/// API keys, tokens, MCP env, or secrets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub runtime_ready: bool,
    #[serde(default)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    pub id: CapabilityId,
    pub kind: CapabilityKind,
    pub provider: CapabilityProviderKind,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    pub risk: CapabilityRisk,
    #[serde(default)]
    pub permissions: Vec<CapabilityPermission>,
    pub status: CapabilityRuntimeStatus,
    pub enabled: bool,
    #[serde(default)]
    pub metadata: CapabilityMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CapabilityModelError {
    #[error("capability id is invalid")]
    InvalidId,
    #[error("capability name must not be empty")]
    EmptyName,
    #[error("capability description exceeds {0} characters")]
    DescriptionTooLong(usize),
    #[error("capability has too many tags")]
    TooManyTags,
    #[error("capability has too many permissions")]
    TooManyPermissions,
    #[error("capability input schema exceeds {0} characters")]
    SchemaTooLong(usize),
    #[error("capability metadata exceeds {0} characters")]
    MetadataTooLong(usize),
    #[error("capability input schema must be a JSON object")]
    InvalidSchema,
}

/// Validate a descriptor before it enters the registry.
pub fn validate_descriptor(descriptor: &CapabilityDescriptor) -> Result<(), CapabilityModelError> {
    if descriptor.name.trim().is_empty() {
        return Err(CapabilityModelError::EmptyName);
    }
    if descriptor.description.chars().count() > MAX_CAPABILITY_DESCRIPTION_CHARS {
        return Err(CapabilityModelError::DescriptionTooLong(
            MAX_CAPABILITY_DESCRIPTION_CHARS,
        ));
    }
    if descriptor.metadata.tags.len() > MAX_CAPABILITY_TAGS {
        return Err(CapabilityModelError::TooManyTags);
    }
    if descriptor.permissions.len() > MAX_PERMISSIONS {
        return Err(CapabilityModelError::TooManyPermissions);
    }
    if let Some(schema) = &descriptor.input_schema {
        if !schema.is_object() {
            return Err(CapabilityModelError::InvalidSchema);
        }
        if schema.to_string().chars().count() > MAX_INPUT_SCHEMA_CHARS {
            return Err(CapabilityModelError::SchemaTooLong(MAX_INPUT_SCHEMA_CHARS));
        }
    }
    let metadata_json = serde_json::to_string(&descriptor.metadata).unwrap_or_default();
    if metadata_json.chars().count() > MAX_METADATA_CHARS {
        return Err(CapabilityModelError::MetadataTooLong(MAX_METADATA_CHARS));
    }
    Ok(())
}
