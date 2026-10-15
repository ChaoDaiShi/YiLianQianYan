//! Capability snapshot collection and bounded projection for ProofLoop planning.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/01-capability-and-compiler.md §3

use super::contracts::{CapabilityKindSpec, CapabilitySnapshot, CapabilitySpec};
use crate::modules::capability::{CapabilityDescriptor, CapabilityKind, CapabilityRuntimeStatus};

pub const MAX_SNAPSHOT_CAPABILITIES: usize = 100;
pub const MAX_SNAPSHOT_CONTEXT_CHARS: usize = 32_000;

pub fn collect_capability_snapshot(
    registry: &crate::modules::capability::CapabilityRegistry,
    allowed_ids: Option<&[String]>,
    now_ms: i64,
) -> CapabilitySnapshot {
    let descriptors = registry.list();
    build_capability_snapshot(&descriptors, allowed_ids, now_ms)
}

pub fn build_capability_snapshot(
    descriptors: &[CapabilityDescriptor],
    allowed_ids: Option<&[String]>,
    now_ms: i64,
) -> CapabilitySnapshot {
    let mut sorted_descriptors = descriptors.to_vec();
    sorted_descriptors.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));

    let mut capabilities = Vec::new();
    let mut total_chars = 2; // The serialized capability array's opening and closing brackets.

    for desc in &sorted_descriptors {
        let cap_id = desc.id.as_str().to_string();
        if let Some(allowed) = allowed_ids {
            if !allowed.iter().any(|a| a == &cap_id) {
                continue;
            }
        }

        let kind = match desc.kind {
            CapabilityKind::McpTool => CapabilityKindSpec::McpTool,
            CapabilityKind::Workflow => CapabilityKindSpec::Workflow,
            _ => continue, // ProofLoop currently only orchestrates mcp_tool and workflow
        };

        let is_ready = desc.enabled
            && desc.status == CapabilityRuntimeStatus::Ready
            && desc.metadata.runtime_ready;

        let readiness = if is_ready {
            "ready".to_string()
        } else {
            "unavailable".to_string()
        };

        let unavailable_reason = if is_ready {
            None
        } else {
            Some(format!(
                "status: {:?}, enabled: {}, runtime_ready: {}",
                desc.status, desc.enabled, desc.metadata.runtime_ready
            ))
        };

        let output_provenance = if desc.output_schema.is_some() {
            "declared".to_string()
        } else {
            "unknown".to_string()
        };

        let spec = CapabilitySpec {
            capability_id: cap_id.clone(),
            executor_ref: format!("capability://{cap_id}"),
            kind,
            name: desc.name.clone(),
            description: desc.description.clone(),
            input_schema: desc.input_schema.clone(),
            output_schema: desc.output_schema.clone(),
            output_path_prefix: vec!["structured".to_string()],
            output_provenance,
            risk: format!("{:?}", desc.risk).to_lowercase(),
            readiness,
            unavailable_reason,
        };

        let spec_chars = serde_json::to_string(&spec)
            .expect("CapabilitySpec serialization is infallible")
            .chars()
            .count();
        let separator_chars = if capabilities.is_empty() { 0 } else { 1 };
        let projected_chars = total_chars + separator_chars + spec_chars;
        if projected_chars > MAX_SNAPSHOT_CONTEXT_CHARS {
            break;
        }
        total_chars = projected_chars;
        capabilities.push(spec);
        if capabilities.len() >= MAX_SNAPSHOT_CAPABILITIES {
            break;
        }
    }

    CapabilitySnapshot {
        snapshot_id: uuid::Uuid::new_v4().to_string(),
        captured_at_ms: now_ms,
        capabilities,
    }
}
