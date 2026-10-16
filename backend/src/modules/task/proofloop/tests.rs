use super::compiler::compile_candidate;
use super::contracts::*;
use super::drafts::DraftService;
use crate::db::Database;
use crate::modules::task::runtime::TaskWorldRuntime;
use crate::modules::task::task_graph::TaskGraphId;
use crate::shared::event::EventHub;
use serde::Deserialize;

#[derive(Deserialize)]
struct FixturePlanBundle {
    contract: GoalContract,
    snapshot: CapabilitySnapshot,
    candidate: CandidatePlan,
}

#[test]
fn test_all_six_fixtures_deserialize() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proofloop");

    // 1. declared-two-node.json
    let content = std::fs::read_to_string(base.join("declared-two-node.json")).unwrap();
    let bundle: FixturePlanBundle = serde_json::from_str(&content).unwrap();
    assert_eq!(bundle.contract.schema_version, 1);
    assert_eq!(bundle.candidate.nodes.len(), 2);
    assert_eq!(bundle.candidate.edges.len(), 1);

    // 2. missing-output-schema.json
    let content = std::fs::read_to_string(base.join("missing-output-schema.json")).unwrap();
    let bundle2: FixturePlanBundle = serde_json::from_str(&content).unwrap();
    assert_eq!(bundle2.candidate.nodes.len(), 2);

    // 3. invalid-capability.json
    let content = std::fs::read_to_string(base.join("invalid-capability.json")).unwrap();
    let bundle3: FixturePlanBundle = serde_json::from_str(&content).unwrap();
    assert_eq!(
        bundle3.candidate.nodes[0].capability_id,
        "mcp__fictional_unregistered_tool"
    );

    // 4. invalid-edge-binding.json
    let content = std::fs::read_to_string(base.join("invalid-edge-binding.json")).unwrap();
    let bundle4: FixturePlanBundle = serde_json::from_str(&content).unwrap();
    assert_eq!(bundle4.candidate.edges.len(), 2);

    // 5. goal-report-incomplete.json
    let content = std::fs::read_to_string(base.join("goal-report-incomplete.json")).unwrap();
    let val5: serde_json::Value = serde_json::from_str(&content).unwrap();
    let report: AcceptanceReport =
        serde_json::from_value(val5["expected_acceptance_report"].clone()).unwrap();
    assert_eq!(report.status, GoalStatus::Fail);

    // 6. repair-one-branch.json
    let content = std::fs::read_to_string(base.join("repair-one-branch.json")).unwrap();
    let val6: serde_json::Value = serde_json::from_str(&content).unwrap();
    let repair: RepairProposal = serde_json::from_value(val6["proposal"].clone()).unwrap();
    assert_eq!(repair.round, 1);
    assert_eq!(repair.rerun_root_node_id, "node-report");
}

#[test]
fn test_compile_declared_two_node_success() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proofloop");
    let content = std::fs::read_to_string(base.join("declared-two-node.json")).unwrap();
    let bundle: FixturePlanBundle = serde_json::from_str(&content).unwrap();

    let graph_id = TaskGraphId::generate();
    let result = compile_candidate(
        &bundle.contract,
        &bundle.candidate,
        &bundle.snapshot,
        &graph_id,
        &[],
    );

    assert_eq!(result.status, CompileStatus::Ready);
    assert!(result.diagnostics.is_empty());
    assert!(result.pending_inputs.is_empty());

    let graph = result.graph.expect("TaskGraph must be generated");
    assert_eq!(graph.id, graph_id);
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);

    // Verify collect node
    let collect_node = graph
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "collect")
        .unwrap();
    assert_eq!(
        collect_node.input["executor_ref"],
        "capability://mcp.fixture_catalog"
    );

    // Verify compare carries capability_bindings
    let compare_node = graph
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "compare")
        .unwrap();
    let bindings = &compare_node.input["capability_bindings"];
    assert_eq!(bindings.as_array().unwrap().len(), 1);
}

#[test]
fn test_compile_missing_output_schema_needs_input() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proofloop");
    let content = std::fs::read_to_string(base.join("missing-output-schema.json")).unwrap();
    let bundle: FixturePlanBundle = serde_json::from_str(&content).unwrap();

    let graph_id = TaskGraphId::generate();
    let result = compile_candidate(
        &bundle.contract,
        &bundle.candidate,
        &bundle.snapshot,
        &graph_id,
        &[],
    );

    assert_eq!(result.status, CompileStatus::NeedsInput);
    assert!(result.graph.is_none());
    assert_eq!(result.pending_inputs.len(), 1);
    assert_eq!(result.pending_inputs[0].node_id, "process");
    assert_eq!(
        result.pending_inputs[0].reason,
        PendingInputReason::UnknownOutput
    );
}

#[test]
fn test_compile_missing_output_schema_with_confirmation_becomes_ready() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proofloop");
    let content = std::fs::read_to_string(base.join("missing-output-schema.json")).unwrap();
    let bundle: FixturePlanBundle = serde_json::from_str(&content).unwrap();

    let confirmation = BindingConfirmation {
        node_id: "process".to_string(),
        target_path: vec!["payload".to_string()],
        source_node_id: "fetch".to_string(),
        source_capability_id: "mcp.fixture_raw_fetch".to_string(),
        source_execution_id: "exec-123".to_string(),
        source_path: vec!["structured".to_string(), "raw_payload".to_string()],
        observed_type: "object".to_string(),
        confirmed_at_ms: 1776000000000,
    };

    let graph_id = TaskGraphId::generate();
    let result = compile_candidate(
        &bundle.contract,
        &bundle.candidate,
        &bundle.snapshot,
        &graph_id,
        &[confirmation],
    );

    assert_eq!(result.status, CompileStatus::Ready);
    assert!(result.graph.is_some());
    assert!(result.pending_inputs.is_empty());
}

#[test]
fn test_compile_invalid_capability_fails() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proofloop");
    let content = std::fs::read_to_string(base.join("invalid-capability.json")).unwrap();
    let bundle: FixturePlanBundle = serde_json::from_str(&content).unwrap();

    let graph_id = TaskGraphId::generate();
    let result = compile_candidate(
        &bundle.contract,
        &bundle.candidate,
        &bundle.snapshot,
        &graph_id,
        &[],
    );

    assert_eq!(result.status, CompileStatus::Invalid);
    assert!(result.graph.is_none());
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "proofloop_capability_unavailable"
            || d.code == "proofloop_capability_not_allowed"));
}

#[test]
fn test_compile_invalid_edge_binding_fails() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proofloop");
    let content = std::fs::read_to_string(base.join("invalid-edge-binding.json")).unwrap();
    let bundle: FixturePlanBundle = serde_json::from_str(&content).unwrap();

    let graph_id = TaskGraphId::generate();
    let result = compile_candidate(
        &bundle.contract,
        &bundle.candidate,
        &bundle.snapshot,
        &graph_id,
        &[],
    );

    assert_eq!(result.status, CompileStatus::Invalid);
    assert!(result.graph.is_none());
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "input_binding_source_not_dependency"));
}

#[test]
fn test_compile_cycle_detection_fails() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proofloop");
    let content = std::fs::read_to_string(base.join("declared-two-node.json")).unwrap();
    let mut bundle: FixturePlanBundle = serde_json::from_str(&content).unwrap();

    // Introduce a cycle: collect -> compare AND compare -> collect
    bundle.candidate.edges.push(CandidateEdge {
        from: "compare".to_string(),
        to: "collect".to_string(),
    });

    let graph_id = TaskGraphId::generate();
    let result = compile_candidate(
        &bundle.contract,
        &bundle.candidate,
        &bundle.snapshot,
        &graph_id,
        &[],
    );

    assert_eq!(result.status, CompileStatus::Invalid);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "cycle_detected"));
}

#[test]
fn test_compile_type_mismatch_fails() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proofloop");
    let content = std::fs::read_to_string(base.join("declared-two-node.json")).unwrap();
    let mut bundle: FixturePlanBundle = serde_json::from_str(&content).unwrap();

    // In declared-two-node, compare.title expects "string", but sources is "array".
    // Bind sources -> title
    bundle.candidate.nodes[1].bindings[0].target_path = vec!["title".to_string()];
    bundle.candidate.nodes[1].literal_input.remove("title");
    bundle.candidate.nodes[1]
        .literal_input
        .insert("sources".to_string(), serde_json::json!(["source1"]));

    let graph_id = TaskGraphId::generate();
    let result = compile_candidate(
        &bundle.contract,
        &bundle.candidate,
        &bundle.snapshot,
        &graph_id,
        &[],
    );

    assert_eq!(result.status, CompileStatus::Invalid);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "input_binding_schema_mismatch"));
}

#[test]
fn test_compile_literal_binding_collision_fails() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proofloop");
    let content = std::fs::read_to_string(base.join("declared-two-node.json")).unwrap();
    let mut bundle: FixturePlanBundle = serde_json::from_str(&content).unwrap();

    // Node compare has binding to "sources", now also give it literal_input for "sources"
    bundle.candidate.nodes[1]
        .literal_input
        .insert("sources".to_string(), serde_json::json!(["literal_source"]));

    let graph_id = TaskGraphId::generate();
    let result = compile_candidate(
        &bundle.contract,
        &bundle.candidate,
        &bundle.snapshot,
        &graph_id,
        &[],
    );

    assert_eq!(result.status, CompileStatus::Invalid);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "binding_target_collision"));
}

fn create_test_db() -> Database {
    Database::new(std::path::Path::new(":memory:")).unwrap()
}

#[test]
fn test_draft_service_update_and_confirm_workflow() {
    let db = create_test_db();
    let service = DraftService::new(db.clone());
    let now = 1776000000000;

    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proofloop");
    let content = std::fs::read_to_string(base.join("declared-two-node.json")).unwrap();
    let bundle: FixturePlanBundle = serde_json::from_str(&content).unwrap();

    let draft_id = "draft-test-01".to_string();
    let graph_id = TaskGraphId::generate();

    let initial_compilation = compile_candidate(
        &bundle.contract,
        &bundle.candidate,
        &bundle.snapshot,
        &graph_id,
        &[],
    );

    let draft = ProofDraft {
        planning_review: None,
        draft_id: draft_id.clone(),
        revision: 1,
        graph_id: graph_id.to_string(),
        contract: bundle.contract.clone(),
        snapshot: bundle.snapshot.clone(),
        candidate: Some(bundle.candidate.clone()),
        binding_confirmations: Vec::new(),
        compilation: initial_compilation,
        status: DraftStatus::ReadyForConfirmation,
        confirmed_graph_revision: None,
        planning_usage: PlanningUsage::default(),
    };

    // Save initial draft
    db.save_proof_draft(&draft, now).unwrap();

    // Verify loaded draft matches
    let loaded = service.get_draft(&draft_id).unwrap().unwrap();
    assert_eq!(loaded.revision, 1);
    assert_eq!(loaded.status, DraftStatus::ReadyForConfirmation);

    // Update draft with stale revision -> conflict
    let update_req_stale = UpdateProofDraftRequest {
        expected_revision: 99,
        contract: bundle.contract.clone(),
        candidate: Some(bundle.candidate.clone()),
        binding_confirmations: None,
    };
    let err = service
        .update_draft(&draft_id, update_req_stale, bundle.snapshot.clone(), now)
        .unwrap_err();
    assert_eq!(err.code, "proofloop_stale_revision");

    // Update draft with matching revision -> revision increments to 2
    let update_req_ok = UpdateProofDraftRequest {
        expected_revision: 1,
        contract: bundle.contract.clone(),
        candidate: Some(bundle.candidate.clone()),
        binding_confirmations: None,
    };
    let updated = service
        .update_draft(&draft_id, update_req_ok, bundle.snapshot.clone(), now)
        .unwrap();
    assert_eq!(updated.revision, 2);

    // Initialize runtime to test confirm_draft
    let events = EventHub::new(100);
    let runtime = TaskWorldRuntime::new(&db, events).unwrap();

    // Confirm draft with stale revision -> conflict
    let confirm_err = service
        .confirm_draft(&runtime, &draft_id, 1, &bundle.snapshot, now)
        .unwrap_err();
    assert_eq!(confirm_err.code, "proofloop_stale_revision");

    // Confirm draft with matching revision 2 -> success
    let confirm_res = service
        .confirm_draft(&runtime, &draft_id, 2, &bundle.snapshot, now)
        .unwrap();
    assert_eq!(confirm_res.graph_id, graph_id.to_string());
    assert_eq!(confirm_res.graph_revision, 1);

    // Verify draft status in DB is now confirmed
    let final_draft = service.get_draft(&draft_id).unwrap().unwrap();
    assert_eq!(final_draft.status, DraftStatus::Confirmed);
    assert_eq!(final_draft.confirmed_graph_revision, Some(1));

    // Verify graph is now in runtime and DB
    let graph = runtime
        .get_graph(&graph_id)
        .expect("Graph must be in runtime");
    assert_eq!(graph.nodes.len(), 2);

    // Test idempotency: confirming again returns same result
    let confirm_again = service
        .confirm_draft(&runtime, &draft_id, 2, &bundle.snapshot, now)
        .unwrap();
    assert_eq!(confirm_again.graph_id, graph_id.to_string());
    assert_eq!(confirm_again.graph_revision, 1);
}

// ── Line A Tests: Capabilities & Schema Paths ──

#[test]
fn test_line_a_schema_paths_resolution_and_compatibility() {
    use super::schema_paths::*;

    let schema = serde_json::json!({
        "type": "object",
        "required": ["data", "meta"],
        "properties": {
            "data": {
                "type": "object",
                "required": ["items"],
                "properties": {
                    "items": { "type": "array" },
                    "count": { "type": "integer" }
                }
            },
            "meta": { "type": "string" }
        }
    });

    // Valid path resolution
    let res = resolve_property(&schema, &["data".into(), "items".into()]).unwrap();
    assert_eq!(res.property_type, "array");
    assert!(res.is_required);

    let res_count = resolve_property(&schema, &["data".into(), "count".into()]).unwrap();
    assert_eq!(res_count.property_type, "integer");
    assert!(!res_count.is_required);

    // Unknown property fails
    let err = resolve_property(&schema, &["data".into(), "missing".into()]).unwrap_err();
    assert_eq!(err.code, "proofloop_unknown_output");

    // Unsupported schema (non-object intermediate)
    let err_unsupported = resolve_property(&schema, &["meta".into(), "nested".into()]).unwrap_err();
    assert_eq!(err_unsupported.code, "proofloop_unsupported_schema");

    // Type compatibility
    assert!(are_types_compatible("string", "string"));
    assert!(are_types_compatible("integer", "number"));
    assert!(!are_types_compatible("string", "number"));
    assert!(!are_types_compatible("number", "integer"));
}

#[test]
fn test_line_a_capability_snapshot_budget_and_filtering() {
    use super::capabilities::{build_capability_snapshot, MAX_SNAPSHOT_CONTEXT_CHARS};
    use crate::modules::capability::{
        CapabilityDescriptor, CapabilityId, CapabilityKind, CapabilityMetadata,
        CapabilityProviderKind, CapabilityRisk, CapabilityRuntimeStatus,
    };

    let desc1 = CapabilityDescriptor {
        id: CapabilityId::new("mcp.tool_a").unwrap(),
        kind: CapabilityKind::McpTool,
        provider: CapabilityProviderKind::Mcp,
        name: "tool_a".into(),
        description: "desc a".into(),
        input_schema: Some(serde_json::json!({"type": "object"})),
        output_schema: Some(
            serde_json::json!({"type": "object", "properties": {"res": {"type": "string"}}}),
        ),
        risk: CapabilityRisk::High,
        permissions: vec![],
        status: CapabilityRuntimeStatus::Ready,
        enabled: true,
        metadata: CapabilityMetadata {
            runtime_ready: true,
            ..Default::default()
        },
    };

    let desc2 = CapabilityDescriptor {
        id: CapabilityId::new("mcp.tool_disabled").unwrap(),
        kind: CapabilityKind::McpTool,
        provider: CapabilityProviderKind::Mcp,
        name: "tool_disabled".into(),
        description: "desc disabled".into(),
        input_schema: None,
        output_schema: None,
        risk: CapabilityRisk::High,
        permissions: vec![],
        status: CapabilityRuntimeStatus::Disabled,
        enabled: false,
        metadata: CapabilityMetadata::default(),
    };

    let snapshot = build_capability_snapshot(&[desc1.clone(), desc2], None, 1000);
    assert_eq!(snapshot.capabilities.len(), 2);
    assert_eq!(snapshot.capabilities[0].readiness, "ready");
    assert_eq!(snapshot.capabilities[0].output_provenance, "declared");
    assert_eq!(snapshot.capabilities[1].readiness, "unavailable");
    assert_eq!(snapshot.capabilities[1].output_provenance, "unknown");

    // Filtering by allowed_ids
    let filtered = build_capability_snapshot(&[desc1], Some(&["mcp.tool_a".to_string()]), 1000);
    assert_eq!(filtered.capabilities.len(), 1);
    assert_eq!(filtered.capabilities[0].capability_id, "mcp.tool_a");

    // The serialized CapabilitySpec includes fields beyond the descriptive
    // text, so even a first item whose name alone reaches the limit must not
    // be admitted.
    let oversized = CapabilityDescriptor {
        id: CapabilityId::new("mcp.oversized").unwrap(),
        kind: CapabilityKind::McpTool,
        provider: CapabilityProviderKind::Mcp,
        name: "n".repeat(MAX_SNAPSHOT_CONTEXT_CHARS),
        description: String::new(),
        input_schema: None,
        output_schema: None,
        risk: CapabilityRisk::High,
        permissions: vec![],
        status: CapabilityRuntimeStatus::Ready,
        enabled: true,
        metadata: CapabilityMetadata {
            runtime_ready: true,
            ..Default::default()
        },
    };
    let too_large = build_capability_snapshot(&[oversized], None, 1000);
    assert!(too_large.capabilities.is_empty());
}

mod acceptance_and_repair;
mod planner;
