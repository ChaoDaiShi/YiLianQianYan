// ============================================================
// Unified Capability Registry — discovery, separated from execution.
//
//   model.rs            — CapabilityDescriptor + validation + limits
//   provider.rs         — CapabilityProvider trait + error
//   registry.rs         — CapabilityRegistry (atomic refresh, dedup, search)
//   builtin.rs          — Builtin + MCP tool providers
//   runtime_providers.rs— Subagent / Agent / Workflow / Skill providers
//
// The registry only DESCRIBES and DISCOVERS capabilities. It has no execute /
// invoke / run entry point. Authorization remains the Security Execution
// Gateway; execution remains the existing runtimes.
// ============================================================

pub mod builtin;
pub mod model;
pub mod provider;
pub mod registry;
pub mod runtime_providers;

#[cfg(test)]
mod tests;

pub use builtin::{BuiltinToolProvider, McpToolProvider};
pub use model::{
    validate_descriptor, CapabilityDescriptor, CapabilityId, CapabilityKind, CapabilityMetadata,
    CapabilityModelError, CapabilityPermission, CapabilityProviderKind, CapabilityRisk,
    CapabilityRuntimeStatus, MAX_CAPABILITY_DESCRIPTION_CHARS, MAX_CAPABILITY_TAGS,
    MAX_INPUT_SCHEMA_CHARS, MAX_METADATA_CHARS, MAX_PERMISSIONS,
};
pub use provider::{CapabilityProvider, CapabilityProviderError};
pub use registry::{CapabilityRegistry, RegistryRefreshReport};
pub use runtime_providers::{AgentProvider, SkillProvider, SubagentProvider, WorkflowProvider};
