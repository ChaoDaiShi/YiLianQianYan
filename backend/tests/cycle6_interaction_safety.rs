//! Narrow Cycle 6 v1 safety tests for approvals and graph proposals.

use std::path::Path;
use std::sync::Arc;

use serde_json::json;
use yilian_backend::db::Database;
use yilian_backend::interaction::{
    ApprovalVoiceAdapter, ApprovalVoiceDecision, GraphProposalService,
};
use yilian_backend::safety::{ApprovalStatus, ApprovalStore};
use yilian_backend::shared::command::{CommandRequest, CommandRouter, CommandStatus};
use yilian_backend::shared::event::EventHub;
use yilian_backend::shared::interaction::{InteractionTarget, TargetResolution};
use yilian_backend::task::{
    TaskEdge, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind, TaskWorldRuntime,
};
use yilian_backend::tools::RiskLevel;

fn pending(store: &ApprovalStore, conversation_id: &str, tool_call_id: &str) -> String {
    store
        .create(
            conversation_id.to_string(),
            tool_call_id.to_string(),
            "bash".to_string(),
            json!({"command": "echo safe"}),
            RiskLevel::High,
            "test approval".to_string(),
            "cycle6-user".to_string(),
        )
        .approval_id
}

fn graph_id() -> TaskGraphId {
    TaskGraphId::new("cycle6-proposal").expect("valid graph id")
}

fn node(id: &str, title: &str) -> TaskNode {
    TaskNode::new(
        TaskNodeId::new(id).expect("valid node id"),
        TaskNodeKind::Work,
        title,
        json!({}),
    )
    .expect("valid task node")
}

#[test]
fn bare_approval_is_fail_closed_and_ambiguity_does_not_consume() {
    let empty = Arc::new(ApprovalStore::new());
    let adapter = ApprovalVoiceAdapter::new(empty);
    assert!(matches!(
        adapter.resolve_unique_for_context(ApprovalVoiceDecision::Approve, "conversation-a"),
        TargetResolution::Missing { .. }
    ));

    let store = Arc::new(ApprovalStore::new());
    let first_id = pending(&store, "conversation-a", "call-a");
    let one = ApprovalVoiceAdapter::new(store.clone());
    assert!(matches!(
        one.resolve_unique_for_context(ApprovalVoiceDecision::Approve, "conversation-a"),
        TargetResolution::Resolved {
            target: InteractionTarget::Approval { .. }
        }
    ));

    let second_id = pending(&store, "conversation-b", "call-b");
    assert!(matches!(
        one.resolve_unique_for_context(ApprovalVoiceDecision::Reject, "conversation-a"),
        TargetResolution::Resolved { .. }
    ));
    assert!(matches!(
        one.resolve_unique_for_context(ApprovalVoiceDecision::Reject, "conversation-c"),
        TargetResolution::Missing { .. }
    ));

    let third_id = pending(&store, "conversation-a", "call-a-2");
    let ambiguous = one.resolve_unique_for_context(ApprovalVoiceDecision::Reject, "conversation-a");
    assert!(matches!(ambiguous, TargetResolution::Ambiguous { .. }));
    assert_eq!(
        store.get(&first_id).unwrap().status,
        ApprovalStatus::Pending
    );
    assert_eq!(
        store.get(&second_id).unwrap().status,
        ApprovalStatus::Pending
    );
    assert_eq!(
        store.get(&third_id).unwrap().status,
        ApprovalStatus::Pending
    );
}

#[test]
fn approval_resolution_enters_through_the_shared_command_router() {
    let store = Arc::new(ApprovalStore::new());
    let approval_id = pending(&store, "conversation-router", "call-router");
    let adapter = ApprovalVoiceAdapter::new(store.clone());
    let router = CommandRouter::new();
    adapter
        .register(&router)
        .expect("approval command registers");

    let result = router.execute(CommandRequest::new(
        "task.approval.resolve",
        "request-approval",
        "cycle6-test",
        json!({"resolution": "approve", "conversation_id": "conversation-router"}),
    ));
    assert_eq!(result.status, CommandStatus::Succeeded);
    assert_eq!(
        store.get(&approval_id).unwrap().status,
        ApprovalStatus::Approved
    );
}

#[test]
fn atomic_bare_resolution_rejects_two_pending_without_consuming_either() {
    let store = Arc::new(ApprovalStore::new());
    let first_id = pending(&store, "conversation-router", "call-router-1");
    let second_id = pending(&store, "conversation-router", "call-router-2");
    let adapter = ApprovalVoiceAdapter::new(store.clone());
    let router = CommandRouter::new();
    adapter
        .register(&router)
        .expect("approval command registers");

    let result = router.execute(CommandRequest::new(
        "task.approval.resolve",
        "request-ambiguous",
        "cycle6-test",
        json!({"resolution": "reject", "conversation_id": "conversation-router"}),
    ));
    assert_eq!(result.status, CommandStatus::Failed);
    assert_eq!(result.error.unwrap().code, "approval_ambiguous");
    assert_eq!(
        store.get(&first_id).unwrap().status,
        ApprovalStatus::Pending
    );
    assert_eq!(
        store.get(&second_id).unwrap().status,
        ApprovalStatus::Pending
    );
}

#[test]
fn approval_command_cannot_consume_an_approval_from_another_conversation() {
    let store = Arc::new(ApprovalStore::new());
    let approval_id = pending(&store, "conversation-owner", "call-owner");
    let adapter = ApprovalVoiceAdapter::new(store.clone());
    let router = CommandRouter::new();
    adapter
        .register(&router)
        .expect("approval command registers");

    let result = router.execute(CommandRequest::new(
        "task.approval.resolve",
        "request-cross-context",
        "cycle6-test",
        json!({"resolution": "approve", "conversation_id": "conversation-other"}),
    ));
    assert_eq!(result.status, CommandStatus::Failed);
    assert_eq!(result.error.unwrap().code, "approval_not_found");
    assert_eq!(
        store.get(&approval_id).unwrap().status,
        ApprovalStatus::Pending
    );
}

#[test]
fn parallelization_is_a_preview_and_leaves_live_graph_unchanged() {
    let database = Database::new(Path::new(":memory:")).expect("database opens");
    let runtime = TaskWorldRuntime::new(&database, EventHub::new(32)).expect("runtime loads");
    let graph_id = graph_id();
    let test_id = TaskNodeId::new("test").unwrap();
    let docs_id = TaskNodeId::new("docs").unwrap();
    runtime
        .create_graph(
            graph_id.clone(),
            vec![node("test", "运行测试"), node("docs", "整理文档")],
            vec![TaskEdge::new(test_id.clone(), docs_id.clone())],
            100,
        )
        .expect("graph creates");
    let before = runtime.get_graph(&graph_id).expect("live graph exists");

    let service = GraphProposalService::new(runtime.clone());
    let proposal = service
        .propose(&graph_id, "让测试和文档并行")
        .expect("proposal builds");

    assert_eq!(proposal.graph_id, graph_id.to_string());
    assert_eq!(proposal.base_revision, before.revision.value());
    assert_eq!(proposal.candidate_revision, before.revision.value() + 1);
    assert!(proposal.requires_confirmation);
    assert!(proposal.validation.valid);
    assert!(!proposal.operations.is_empty());
    assert_eq!(proposal.operations[0].operation, "remove_dependency");
    assert_eq!(proposal.operations[0].from_node_id.as_deref(), Some("test"));
    assert_eq!(proposal.operations[0].to_node_id.as_deref(), Some("docs"));

    let after = runtime.get_graph(&graph_id).expect("live graph remains");
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.edges, before.edges);
}

#[test]
fn graph_proposal_fails_closed_when_matching_operations_exceed_limit() {
    let database = Database::new(Path::new(":memory:")).expect("database opens");
    let runtime = TaskWorldRuntime::new(&database, EventHub::new(32)).expect("runtime loads");
    let graph_id = TaskGraphId::new("cycle6-proposal-limit").unwrap();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for index in 0..17 {
        let test = format!("test-{index}");
        let docs = format!("docs-{index}");
        nodes.push(node(&test, &format!("运行测试 {index}")));
        nodes.push(node(&docs, &format!("整理文档 {index}")));
        edges.push(TaskEdge::new(
            TaskNodeId::new(test).unwrap(),
            TaskNodeId::new(docs).unwrap(),
        ));
    }
    runtime
        .create_graph(graph_id.clone(), nodes, edges, 100)
        .expect("graph creates");

    let service = GraphProposalService::new(runtime);
    let error = service
        .propose(&graph_id, "让测试和文档并行")
        .expect_err("oversized proposal must fail closed");
    assert!(error.to_string().contains("16"));
}
