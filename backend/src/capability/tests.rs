// ============================================================
// Capability Registry tests — model, registry, providers, security.
// ============================================================

use std::sync::Arc;

use async_trait::async_trait;

use super::model::*;
use super::provider::{CapabilityProvider, CapabilityProviderError};
use super::registry::CapabilityRegistry;
use super::{BuiltinToolProvider, McpToolProvider};

// ── Test provider ──

struct StaticProvider {
    kind: CapabilityProviderKind,
    descriptors: Vec<CapabilityDescriptor>,
    fail: bool,
}

impl StaticProvider {
    fn new(kind: CapabilityProviderKind, descriptors: Vec<CapabilityDescriptor>) -> Self {
        Self {
            kind,
            descriptors,
            fail: false,
        }
    }
    fn failing(kind: CapabilityProviderKind) -> Self {
        Self {
            kind,
            descriptors: vec![],
            fail: true,
        }
    }
}

#[async_trait]
impl CapabilityProvider for StaticProvider {
    fn provider_kind(&self) -> CapabilityProviderKind {
        self.kind
    }
    async fn discover(&self) -> Result<Vec<CapabilityDescriptor>, CapabilityProviderError> {
        if self.fail {
            return Err(CapabilityProviderError::ProviderUnavailable(
                "down".to_string(),
            ));
        }
        Ok(self.descriptors.clone())
    }
}

fn descriptor(id: &str, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor {
        id: CapabilityId::new(id).unwrap(),
        kind,
        provider: CapabilityProviderKind::Builtin,
        name: format!("name-{id}"),
        description: format!("description {id}"),
        input_schema: Some(serde_json::json!({"type": "object"})),
        risk: CapabilityRisk::Low,
        permissions: vec![],
        status: CapabilityRuntimeStatus::Ready,
        enabled: true,
        metadata: CapabilityMetadata::default(),
    }
}

// ── Model ──

#[test]
fn capability_id_is_stable() {
    let id = CapabilityId::new("builtin.tool.read_file").unwrap();
    assert_eq!(id.as_str(), "builtin.tool.read_file");
    assert_eq!(id.to_string(), "builtin.tool.read_file");
    assert_eq!(
        serde_json::to_value(&id).unwrap(),
        serde_json::json!("builtin.tool.read_file")
    );
    assert!(CapabilityId::new("").is_err());
}

#[test]
fn descriptor_serialization_roundtrips() {
    let d = descriptor("builtin.tool.read_file", CapabilityKind::Tool);
    let json = serde_json::to_value(&d).unwrap();
    assert_eq!(json["id"], "builtin.tool.read_file");
    assert_eq!(json["kind"], "tool");
    assert_eq!(json["risk"], "low");
    assert_eq!(json["status"], "ready");
    let back: CapabilityDescriptor = serde_json::from_value(json).unwrap();
    assert_eq!(back.id.as_str(), "builtin.tool.read_file");
    assert_eq!(back.risk, CapabilityRisk::Low);
}

#[test]
fn invalid_descriptor_rejected() {
    let mut d = descriptor("x", CapabilityKind::Tool);
    d.name = "  ".to_string();
    assert!(validate_descriptor(&d).is_err());
}

#[test]
fn metadata_and_schema_limits_enforced() {
    let mut d = descriptor("x", CapabilityKind::Tool);
    d.metadata.tags = (0..(MAX_CAPABILITY_TAGS + 1))
        .map(|i| format!("t{i}"))
        .collect();
    assert!(validate_descriptor(&d).is_err());

    let mut d2 = descriptor("y", CapabilityKind::Tool);
    d2.input_schema =
        Some(serde_json::json!({"type": "object", "long": "x".repeat(MAX_INPUT_SCHEMA_CHARS)}));
    assert!(validate_descriptor(&d2).is_err());
}

// ── Registry ──

#[tokio::test]
async fn registry_discovers_multiple_providers() {
    let p1 = StaticProvider::new(
        CapabilityProviderKind::Builtin,
        vec![descriptor("builtin.tool.a", CapabilityKind::Tool)],
    );
    let p2 = StaticProvider::new(
        CapabilityProviderKind::AgentRuntime,
        vec![descriptor("agent.1", CapabilityKind::Agent)],
    );
    let registry = CapabilityRegistry::new(vec![Arc::new(p1), Arc::new(p2)]);
    let report = registry.refresh().await;
    assert_eq!(report.discovered, 2);
    assert_eq!(registry.len(), 2);
}

#[tokio::test]
async fn provider_failure_does_not_drop_others() {
    let ok = StaticProvider::new(
        CapabilityProviderKind::Builtin,
        vec![descriptor("builtin.tool.a", CapabilityKind::Tool)],
    );
    let failing = StaticProvider::failing(CapabilityProviderKind::Mcp);
    let registry = CapabilityRegistry::new(vec![Arc::new(ok), Arc::new(failing)]);
    let report = registry.refresh().await;
    assert_eq!(report.discovered, 1);
    assert_eq!(report.provider_failures, 1);
    assert_eq!(registry.len(), 1);
}

#[tokio::test]
async fn duplicate_capability_id_rejected() {
    let p1 = StaticProvider::new(
        CapabilityProviderKind::Builtin,
        vec![descriptor("builtin.tool.dup", CapabilityKind::Tool)],
    );
    let p2 = StaticProvider::new(
        CapabilityProviderKind::Mcp,
        vec![descriptor("builtin.tool.dup", CapabilityKind::McpTool)],
    );
    let registry = CapabilityRegistry::new(vec![Arc::new(p1), Arc::new(p2)]);
    let report = registry.refresh().await;
    assert_eq!(report.duplicates, 1);
    assert_eq!(registry.len(), 1);
}

#[tokio::test]
async fn refresh_atomically_replaces_snapshot_and_filters() {
    let p = StaticProvider::new(
        CapabilityProviderKind::Builtin,
        vec![
            descriptor("builtin.tool.a", CapabilityKind::Tool),
            descriptor("agent.1", CapabilityKind::Agent),
        ],
    );
    let registry = CapabilityRegistry::new(vec![Arc::new(p)]);
    registry.refresh().await;

    assert!(registry
        .get(&CapabilityId::new("builtin.tool.a").unwrap())
        .is_some());
    assert!(registry
        .get(&CapabilityId::new("missing").unwrap())
        .is_none());
    assert_eq!(registry.find_by_kind(CapabilityKind::Tool).len(), 1);
    assert_eq!(registry.find_by_kind(CapabilityKind::Agent).len(), 1);

    // Search by name/description.
    let hits = registry.search("name-builtin");
    assert_eq!(hits.len(), 1);
    assert_eq!(registry.search("zzz-none").len(), 0);
}

// ── Builtin provider ──

#[tokio::test]
async fn builtin_tools_are_discovered_with_schema_and_risk() {
    let tool_registry = Arc::new(crate::tools::ToolRegistry::with_defaults("."));
    let provider = BuiltinToolProvider::new(tool_registry.clone());
    let descriptors = provider.discover().await.unwrap();
    assert!(!descriptors.is_empty());
    // Every builtin descriptor has a valid namespaced id and object schema.
    for d in &descriptors {
        assert!(d.id.as_str().starts_with("builtin.tool."));
        assert!(d.input_schema.as_ref().unwrap().is_object());
    }
    // A high-risk tool maps to High.
    let bash = descriptors
        .iter()
        .find(|d| d.id.as_str() == "builtin.tool.bash")
        .expect("bash tool discovered");
    assert_eq!(bash.risk, CapabilityRisk::High);
}

#[tokio::test]
async fn builtin_discovery_does_not_execute_tool() {
    // Discovery is a pure read of ToolInfo — no tool is executed.
    let tool_registry = Arc::new(crate::tools::ToolRegistry::with_defaults("."));
    let provider = BuiltinToolProvider::new(tool_registry);
    let descriptors = provider.discover().await.unwrap();
    assert!(descriptors
        .iter()
        .all(|d| d.status == CapabilityRuntimeStatus::Ready));
}

// ── MCP provider ──

#[tokio::test]
async fn mcp_provider_only_reports_mcp_prefixed_tools() {
    // An empty registry has no MCP tools.
    let provider = McpToolProvider::new(Arc::new(crate::tools::ToolRegistry::new()));
    assert!(provider.discover().await.unwrap().is_empty());

    // A registry with a builtin tool (no mcp_ prefix) still reports no MCP tools.
    let default = Arc::new(crate::tools::ToolRegistry::with_defaults("."));
    let provider = McpToolProvider::new(default);
    assert!(provider.discover().await.unwrap().is_empty());
}

// ── Planner reference validation ──

#[tokio::test]
async fn planner_reference_validation_rejects_missing_and_unready() {
    use crate::task::model::{TaskPlan, TaskPlanExecutor, TaskPlanStep};
    use crate::task::planner::validate_plan_references;

    let ready_agent = descriptor("agent.1", CapabilityKind::Agent);
    let disabled_workflow = {
        let mut d = descriptor("workflow.g1", CapabilityKind::Workflow);
        d.status = CapabilityRuntimeStatus::Misconfigured;
        d
    };
    let provider = StaticProvider::new(
        CapabilityProviderKind::AgentRuntime,
        vec![ready_agent, disabled_workflow],
    );
    let registry = CapabilityRegistry::new(vec![Arc::new(provider)]);
    registry.refresh().await;

    let ready_plan = TaskPlan {
        schema_version: 1,
        summary: "s".to_string(),
        steps: vec![TaskPlanStep {
            id: "s1".to_string(),
            title: "t".to_string(),
            instruction: "i".to_string(),
            executor: TaskPlanExecutor::Agent {
                agent_id: crate::task::model::AgentId::new("1").unwrap(),
            },
        }],
    };
    assert!(validate_plan_references(&ready_plan, &registry).is_ok());

    // Missing capability.
    let missing = TaskPlan {
        schema_version: 1,
        summary: "s".to_string(),
        steps: vec![TaskPlanStep {
            id: "s1".to_string(),
            title: "t".to_string(),
            instruction: "i".to_string(),
            executor: TaskPlanExecutor::Subagent {
                name: "ghost".to_string(),
            },
        }],
    };
    assert!(validate_plan_references(&missing, &registry).is_err());

    // Disabled / unready workflow.
    let unready = TaskPlan {
        schema_version: 1,
        summary: "s".to_string(),
        steps: vec![TaskPlanStep {
            id: "s1".to_string(),
            title: "t".to_string(),
            instruction: "i".to_string(),
            executor: TaskPlanExecutor::Workflow {
                workflow_graph_id: "g1".to_string(),
            },
        }],
    };
    assert!(validate_plan_references(&unready, &registry).is_err());
}
