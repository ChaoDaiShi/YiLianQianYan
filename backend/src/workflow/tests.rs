// ============================================================
// Workflow DAG definition & validation tests.
// ============================================================

use super::*;
use crate::config::types::SandboxConfig;
use crate::db::Database;
use crate::execution::{ExecutionContext, ExecutionId};
use crate::safety::SecurityExecutionGateway;
use crate::secret::{InMemorySecretStore, SecretResolver};
use crate::tools::{Tool, ToolRegistry, ToolResult};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

fn test_resolver() -> Arc<SecretResolver> {
    Arc::new(SecretResolver::new(Arc::new(InMemorySecretStore::new())))
}

fn default_config(kind: WorkflowNodeKind) -> WorkflowNodeConfig {
    match kind {
        WorkflowNodeKind::Agent => WorkflowNodeConfig::Agent {
            prompt: String::new(),
        },
        WorkflowNodeKind::Tool => WorkflowNodeConfig::Tool {
            tool_name: String::new(),
            arguments: serde_json::json!({}),
        },
        WorkflowNodeKind::Subagent => WorkflowNodeConfig::Subagent {
            subagent_name: String::new(),
            task: String::new(),
        },
        WorkflowNodeKind::Condition => WorkflowNodeConfig::Condition {
            when: WorkflowCondition::Always,
        },
        WorkflowNodeKind::Output => WorkflowNodeConfig::Output { template: None },
    }
}

fn node(id: &str) -> WorkflowNodeDefinition {
    WorkflowNodeDefinition {
        id: WorkflowNodeId::new(id).unwrap(),
        kind: WorkflowNodeKind::Agent,
        config: default_config(WorkflowNodeKind::Agent),
    }
}

fn node_of_kind(id: &str, kind: WorkflowNodeKind) -> WorkflowNodeDefinition {
    WorkflowNodeDefinition {
        id: WorkflowNodeId::new(id).unwrap(),
        kind,
        config: default_config(kind),
    }
}

fn tool_node(id: &str, tool_name: &str, arguments: serde_json::Value) -> WorkflowNodeDefinition {
    WorkflowNodeDefinition {
        id: WorkflowNodeId::new(id).unwrap(),
        kind: WorkflowNodeKind::Tool,
        config: WorkflowNodeConfig::Tool {
            tool_name: tool_name.to_string(),
            arguments,
        },
    }
}

fn subagent_node(id: &str, subagent_name: &str, task: &str) -> WorkflowNodeDefinition {
    WorkflowNodeDefinition {
        id: WorkflowNodeId::new(id).unwrap(),
        kind: WorkflowNodeKind::Subagent,
        config: WorkflowNodeConfig::Subagent {
            subagent_name: subagent_name.to_string(),
            task: task.to_string(),
        },
    }
}

fn edge(from: &str, to: &str) -> WorkflowEdgeDefinition {
    WorkflowEdgeDefinition {
        from: WorkflowNodeId::new(from).unwrap(),
        to: WorkflowNodeId::new(to).unwrap(),
    }
}

fn graph(
    entry: &str,
    nodes: Vec<WorkflowNodeDefinition>,
    edges: Vec<WorkflowEdgeDefinition>,
) -> WorkflowGraphDefinition {
    WorkflowGraphDefinition {
        schema_version: WORKFLOW_GRAPH_SCHEMA_VERSION,
        entry_node_id: WorkflowNodeId::new(entry).unwrap(),
        nodes,
        edges,
    }
}

#[test]
fn valid_linear_graph_passes_validation() {
    let g = graph(
        "start",
        vec![node("start"), node("a"), node("b"), node("output")],
        vec![edge("start", "a"), edge("a", "b"), edge("b", "output")],
    );
    assert_eq!(g.validate(), Ok(()));
}

#[test]
fn valid_branching_graph_passes_validation() {
    let g = graph(
        "start",
        vec![node("start"), node("a"), node("b"), node("output")],
        vec![
            edge("start", "a"),
            edge("start", "b"),
            edge("a", "output"),
            edge("b", "output"),
        ],
    );
    assert_eq!(g.validate(), Ok(()));
}

#[test]
fn empty_graph_is_rejected() {
    let g = graph("start", vec![], vec![]);
    assert_eq!(g.validate(), Err(WorkflowValidationError::EmptyGraph));
}

#[test]
fn duplicate_node_id_is_rejected() {
    let g = graph("start", vec![node("start"), node("start")], vec![]);
    assert_eq!(
        g.validate(),
        Err(WorkflowValidationError::DuplicateNodeId("start".into()))
    );
}

#[test]
fn missing_entry_node_is_rejected() {
    let g = graph(
        "ghost",
        vec![node("start"), node("a")],
        vec![edge("start", "a")],
    );
    assert_eq!(
        g.validate(),
        Err(WorkflowValidationError::EntryNodeMissing("ghost".into()))
    );
}

#[test]
fn entry_node_with_incoming_edge_is_rejected() {
    let g = graph(
        "start",
        vec![node("start"), node("a")],
        vec![edge("a", "start")],
    );
    assert_eq!(
        g.validate(),
        Err(WorkflowValidationError::EntryNodeHasIncomingEdge(
            "start".into()
        ))
    );
}

#[test]
fn edge_with_missing_endpoint_is_rejected() {
    let g = graph("start", vec![node("start")], vec![edge("start", "ghost")]);
    assert_eq!(
        g.validate(),
        Err(WorkflowValidationError::MissingEdgeEndpoint(
            "start".into(),
            "ghost".into()
        ))
    );
}

#[test]
fn self_edge_is_rejected() {
    let g = graph(
        "start",
        vec![node("start"), node("a")],
        vec![edge("start", "start"), edge("start", "a")],
    );
    assert_eq!(
        g.validate(),
        Err(WorkflowValidationError::SelfEdge("start".into()))
    );
}

#[test]
fn duplicate_edge_is_rejected() {
    let g = graph(
        "start",
        vec![node("start"), node("a")],
        vec![edge("start", "a"), edge("start", "a")],
    );
    assert_eq!(
        g.validate(),
        Err(WorkflowValidationError::DuplicateEdge(
            "start".into(),
            "a".into()
        ))
    );
}

#[test]
fn cycle_is_rejected() {
    let g = graph(
        "start",
        vec![node("start"), node("a"), node("b")],
        vec![edge("start", "a"), edge("a", "b"), edge("b", "a")],
    );
    assert!(matches!(
        g.validate(),
        Err(WorkflowValidationError::CycleDetected(_))
    ));
}

#[test]
fn unreachable_node_is_rejected() {
    let g = graph(
        "start",
        vec![node("start"), node("a"), node("b")],
        vec![edge("start", "a")],
    );
    assert_eq!(
        g.validate(),
        Err(WorkflowValidationError::UnreachableNode("b".into()))
    );
}

#[test]
fn invalid_node_id_is_rejected() {
    assert!(WorkflowNodeId::new("").is_err());
    assert!(WorkflowNodeId::new("has space").is_err());
    assert!(WorkflowNodeId::new("中文").is_err());
    assert!(WorkflowNodeId::new("a/b").is_err());
    assert!(WorkflowNodeId::new("a\\b").is_err());
    assert!(WorkflowNodeId::new("a\nb").is_err());
    assert!(WorkflowNodeId::new("a".repeat(65)).is_err());
}

#[test]
fn too_many_nodes_is_rejected() {
    let nodes: Vec<_> = (0..=64).map(|i| node(&format!("n{}", i))).collect();
    let g = graph("n0", nodes, vec![]);
    assert_eq!(g.validate(), Err(WorkflowValidationError::TooManyNodes(65)));
}

#[test]
fn too_many_edges_is_rejected() {
    let nodes: Vec<_> = (0..64).map(|i| node(&format!("n{}", i))).collect();
    let mut edges = Vec::new();
    'outer: for i in 0..64 {
        for j in 0..64 {
            if i == j {
                continue;
            }
            edges.push(edge(&format!("n{}", i), &format!("n{}", j)));
            if edges.len() > MAX_WORKFLOW_EDGES {
                break 'outer;
            }
        }
    }
    let g = graph("n0", nodes, edges);
    assert_eq!(
        g.validate(),
        Err(WorkflowValidationError::TooManyEdges(257))
    );
}

#[test]
fn unsupported_schema_version_is_rejected() {
    let mut g = graph("start", vec![node("start")], vec![]);
    g.schema_version = WORKFLOW_GRAPH_SCHEMA_VERSION + 1;
    assert_eq!(
        g.validate(),
        Err(WorkflowValidationError::UnsupportedSchemaVersion(
            WORKFLOW_GRAPH_SCHEMA_VERSION + 1
        ))
    );
}

#[test]
fn graph_serde_roundtrips() {
    let g = graph(
        "start",
        vec![
            node_of_kind("start", WorkflowNodeKind::Agent),
            node_of_kind("research", WorkflowNodeKind::Subagent),
            node_of_kind("save", WorkflowNodeKind::Tool),
            node_of_kind("done", WorkflowNodeKind::Output),
        ],
        vec![
            edge("start", "research"),
            edge("research", "save"),
            edge("save", "done"),
        ],
    );

    let json = serde_json::to_value(&g).unwrap();
    assert_eq!(json["entry_node_id"], "start");
    assert_eq!(json["nodes"][0]["kind"], "agent");
    assert_eq!(json["nodes"][1]["kind"], "subagent");
    assert_eq!(json["nodes"][2]["kind"], "tool");
    assert_eq!(json["nodes"][3]["kind"], "output");
    assert_eq!(json["edges"][0]["from"], "start");
    assert_eq!(json["edges"][0]["to"], "research");

    let back: WorkflowGraphDefinition = serde_json::from_value(json).unwrap();
    assert_eq!(back, g);
}

#[test]
fn node_id_serde_transparent_and_display() {
    let id = WorkflowNodeId::new("start-node.1").unwrap();
    assert_eq!(id.as_str(), "start-node.1");
    assert_eq!(id.to_string(), "start-node.1");
    assert_eq!(
        serde_json::to_value(&id).unwrap(),
        serde_json::json!("start-node.1")
    );
    let back: WorkflowNodeId = serde_json::from_value(serde_json::json!("start-node.1")).unwrap();
    assert_eq!(back, id);
}

// ============================================================
// Workflow run state machine tests.
// ============================================================

fn id(s: &str) -> WorkflowNodeId {
    WorkflowNodeId::new(s).unwrap()
}

fn ctx() -> ExecutionContext {
    ExecutionContext::new(
        ExecutionId::new("exec-1").unwrap(),
        "local-user",
        "researcher",
        None,
        1_000,
    )
}

fn run(
    entry: &str,
    nodes: Vec<WorkflowNodeDefinition>,
    edges: Vec<WorkflowEdgeDefinition>,
) -> WorkflowRun {
    WorkflowRun::new(
        WorkflowRunId::generate(),
        ctx(),
        graph(entry, nodes, edges),
        1_000,
    )
    .unwrap()
}

#[test]
fn run_initializes_entry_ready() {
    let r = run(
        "start",
        vec![node("start"), node("a"), node("b")],
        vec![edge("start", "a"), edge("a", "b")],
    );
    assert_eq!(r.status, WorkflowRunStatus::Created);
    assert_eq!(r.node(&id("start")).unwrap().status, NodeRunStatus::Ready);
}

#[test]
fn run_initializes_other_nodes_pending() {
    let r = run(
        "start",
        vec![node("start"), node("a"), node("b")],
        vec![edge("start", "a"), edge("a", "b")],
    );
    assert_eq!(r.node(&id("a")).unwrap().status, NodeRunStatus::Pending);
    assert_eq!(r.node(&id("b")).unwrap().status, NodeRunStatus::Pending);
}

#[test]
fn dependency_completion_makes_next_node_ready() {
    let mut r = run(
        "start",
        vec![node("start"), node("a"), node("b")],
        vec![edge("start", "a"), edge("a", "b")],
    );
    let start = id("start");
    r.transition_node(&start, NodeRunStatus::Running, 1)
        .unwrap();
    r.transition_node(&start, NodeRunStatus::Completed, 2)
        .unwrap();
    // `a` is now eligible but still pending until the runner promotes it.
    assert_eq!(r.ready_nodes(), vec![id("a")]);
    assert_eq!(r.node(&id("a")).unwrap().status, NodeRunStatus::Pending);
}

#[test]
fn two_dependencies_all_must_complete() {
    let mut r = run(
        "start",
        vec![node("start"), node("a"), node("b"), node("join")],
        vec![
            edge("start", "a"),
            edge("start", "b"),
            edge("a", "join"),
            edge("b", "join"),
        ],
    );
    let start = id("start");
    let a = id("a");
    let b = id("b");
    let join = id("join");

    r.transition_node(&start, NodeRunStatus::Running, 1)
        .unwrap();
    r.transition_node(&start, NodeRunStatus::Completed, 2)
        .unwrap();
    assert_eq!(r.ready_nodes(), vec![a.clone(), b.clone()]);

    // Complete only `a`; `join` must not be eligible yet.
    r.transition_node(&a, NodeRunStatus::Ready, 3).unwrap();
    r.transition_node(&a, NodeRunStatus::Running, 4).unwrap();
    r.transition_node(&a, NodeRunStatus::Completed, 5).unwrap();
    assert!(!r.ready_nodes().contains(&join));

    // Complete `b`; now `join` becomes eligible.
    r.transition_node(&b, NodeRunStatus::Ready, 6).unwrap();
    r.transition_node(&b, NodeRunStatus::Running, 7).unwrap();
    r.transition_node(&b, NodeRunStatus::Completed, 8).unwrap();
    assert_eq!(r.ready_nodes(), vec![join]);
}

#[test]
fn join_node_readiness() {
    let mut r = run(
        "start",
        vec![node("start"), node("a"), node("b"), node("c"), node("join")],
        vec![
            edge("start", "a"),
            edge("start", "b"),
            edge("start", "c"),
            edge("a", "join"),
            edge("b", "join"),
            edge("c", "join"),
        ],
    );
    let start = id("start");
    r.transition_node(&start, NodeRunStatus::Running, 1)
        .unwrap();
    r.transition_node(&start, NodeRunStatus::Completed, 2)
        .unwrap();

    // Complete parents in arbitrary order (c, a, b).
    for (n, t) in [("c", 3), ("a", 4), ("b", 5)] {
        let nid = id(n);
        r.transition_node(&nid, NodeRunStatus::Ready, t).unwrap();
        r.transition_node(&nid, NodeRunStatus::Running, t + 1)
            .unwrap();
        r.transition_node(&nid, NodeRunStatus::Completed, t + 2)
            .unwrap();
    }
    assert_eq!(r.ready_nodes(), vec![id("join")]);
}

#[test]
fn invalid_transition_is_rejected() {
    let mut r = run(
        "start",
        vec![node("start"), node("a")],
        vec![edge("start", "a")],
    );
    let start = id("start");
    // Ready -> Ready is illegal.
    assert!(r.transition_node(&start, NodeRunStatus::Ready, 1).is_err());
    // Ready -> Completed is illegal (must pass through Running).
    assert!(r
        .transition_node(&start, NodeRunStatus::Completed, 1)
        .is_err());
    // Ready -> Running is legal.
    assert!(r.transition_node(&start, NodeRunStatus::Running, 1).is_ok());
}

#[test]
fn completed_node_is_immutable() {
    let mut r = run(
        "start",
        vec![node("start"), node("a")],
        vec![edge("start", "a")],
    );
    let start = id("start");
    r.transition_node(&start, NodeRunStatus::Running, 1)
        .unwrap();
    r.transition_node(&start, NodeRunStatus::Completed, 2)
        .unwrap();
    assert!(r
        .transition_node(&start, NodeRunStatus::Running, 3)
        .is_err());
    assert!(r.transition_node(&start, NodeRunStatus::Failed, 3).is_err());
}

#[test]
fn waiting_approval_resumes_to_running() {
    let mut r = run(
        "start",
        vec![node("start"), node("a")],
        vec![edge("start", "a")],
    );
    let start = id("start");
    let a = id("a");

    r.transition_node(&start, NodeRunStatus::Running, 1)
        .unwrap();
    r.transition_node(&start, NodeRunStatus::WaitingApproval, 2)
        .unwrap();
    assert_eq!(r.status, WorkflowRunStatus::WaitingApproval);

    // Resume the same node, then finish the run.
    r.transition_node(&start, NodeRunStatus::Running, 3)
        .unwrap();
    assert_eq!(r.status, WorkflowRunStatus::Running);
    r.transition_node(&start, NodeRunStatus::Completed, 4)
        .unwrap();
    r.transition_node(&a, NodeRunStatus::Ready, 5).unwrap();
    r.transition_node(&a, NodeRunStatus::Running, 6).unwrap();
    r.transition_node(&a, NodeRunStatus::Completed, 7).unwrap();
    assert_eq!(r.status, WorkflowRunStatus::Completed);
}

#[test]
fn failure_propagates_to_run_status() {
    let mut r = run(
        "start",
        vec![node("start"), node("a")],
        vec![edge("start", "a")],
    );
    let start = id("start");
    r.transition_node(&start, NodeRunStatus::Running, 1)
        .unwrap();
    r.transition_node(&start, NodeRunStatus::Failed, 2).unwrap();
    assert_eq!(r.status, WorkflowRunStatus::Failed);
}

#[test]
fn cancel_marks_run_and_pending_nodes_cancelled() {
    let mut r = run(
        "start",
        vec![node("start"), node("a"), node("b")],
        vec![edge("start", "a"), edge("a", "b")],
    );
    let start = id("start");
    r.transition_node(&start, NodeRunStatus::Running, 1)
        .unwrap();
    r.cancel(2);
    assert_eq!(r.status, WorkflowRunStatus::Cancelled);
    assert_eq!(r.node(&start).unwrap().status, NodeRunStatus::Cancelled);
    assert_eq!(r.node(&id("a")).unwrap().status, NodeRunStatus::Cancelled);
    assert_eq!(r.node(&id("b")).unwrap().status, NodeRunStatus::Cancelled);
}

#[test]
fn branch_ready_calculation() {
    let mut r = run(
        "start",
        vec![node("start"), node("a"), node("b"), node("output")],
        vec![
            edge("start", "a"),
            edge("start", "b"),
            edge("a", "output"),
            edge("b", "output"),
        ],
    );
    let start = id("start");
    r.transition_node(&start, NodeRunStatus::Running, 1)
        .unwrap();
    r.transition_node(&start, NodeRunStatus::Completed, 2)
        .unwrap();
    assert_eq!(r.ready_nodes(), vec![id("a"), id("b")]);
}

#[test]
fn run_rejects_invalid_definition() {
    let bad = graph(
        "start",
        vec![node("start"), node("a")],
        vec![edge("a", "a")],
    );
    assert!(WorkflowRun::new(WorkflowRunId::generate(), ctx(), bad, 1_000).is_err());
}

#[test]
fn run_id_generates_unique_values() {
    let a = WorkflowRunId::generate();
    let b = WorkflowRunId::generate();
    assert!(!a.as_str().is_empty());
    assert_ne!(a, b);
}

// ============================================================
// Deterministic runner tests (mock executors).
// ============================================================

fn noop_persist(_run: &WorkflowRun) -> Result<(), String> {
    Ok(())
}

struct MockExecutor;

#[async_trait]
impl WorkflowNodeExecutor for MockExecutor {
    async fn execute(
        &self,
        _context: &ExecutionContext,
        _run_id: &WorkflowRunId,
        node: &WorkflowNodeDefinition,
    ) -> Result<NodeExecutionOutcome, WorkflowExecutionError> {
        Ok(match node.id.as_str() {
            "fail" => NodeExecutionOutcome::Failed { error: None },
            "approve" => NodeExecutionOutcome::WaitingApproval {
                approval_id: "a1".to_string(),
            },
            _ => NodeExecutionOutcome::Completed { result: None },
        })
    }
}

struct RecordingExecutor {
    calls: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl WorkflowNodeExecutor for RecordingExecutor {
    async fn execute(
        &self,
        _context: &ExecutionContext,
        _run_id: &WorkflowRunId,
        node: &WorkflowNodeDefinition,
    ) -> Result<NodeExecutionOutcome, WorkflowExecutionError> {
        self.calls.lock().unwrap().push(node.id.to_string());
        Ok(NodeExecutionOutcome::Completed { result: None })
    }
}

struct CancellingExecutor {
    cancel: CancellationToken,
    trigger: String,
}

#[async_trait]
impl WorkflowNodeExecutor for CancellingExecutor {
    async fn execute(
        &self,
        _context: &ExecutionContext,
        _run_id: &WorkflowRunId,
        node: &WorkflowNodeDefinition,
    ) -> Result<NodeExecutionOutcome, WorkflowExecutionError> {
        if node.id.as_str() == self.trigger.as_str() {
            self.cancel.cancel();
        }
        Ok(NodeExecutionOutcome::Completed { result: None })
    }
}

#[tokio::test]
async fn runner_completes_linear_dag() {
    let mut r = run(
        "start",
        vec![node("start"), node("a"), node("b"), node("output")],
        vec![edge("start", "a"), edge("a", "b"), edge("b", "output")],
    );
    let runner = WorkflowRunner::new(MockExecutor);
    runner
        .run(&mut r, &CancellationToken::new(), noop_persist)
        .await
        .unwrap();
    assert_eq!(r.status, WorkflowRunStatus::Completed);
    for name in ["start", "a", "b", "output"] {
        assert_eq!(r.node(&id(name)).unwrap().status, NodeRunStatus::Completed);
    }
}

#[tokio::test]
async fn runner_executes_branches_in_definition_order() {
    let mut r = run(
        "start",
        vec![node("start"), node("a"), node("b"), node("output")],
        vec![
            edge("start", "a"),
            edge("start", "b"),
            edge("a", "output"),
            edge("b", "output"),
        ],
    );
    let calls = Arc::new(Mutex::new(Vec::new()));
    let runner = WorkflowRunner::new(RecordingExecutor {
        calls: calls.clone(),
    });
    runner
        .run(&mut r, &CancellationToken::new(), noop_persist)
        .await
        .unwrap();
    assert_eq!(r.status, WorkflowRunStatus::Completed);
    assert_eq!(
        calls.lock().unwrap().clone(),
        vec!["start", "a", "b", "output"]
    );
}

#[tokio::test]
async fn runner_respects_join_dependency() {
    let mut r = run(
        "start",
        vec![node("start"), node("a"), node("b"), node("join")],
        vec![
            edge("start", "a"),
            edge("start", "b"),
            edge("a", "join"),
            edge("b", "join"),
        ],
    );
    let calls = Arc::new(Mutex::new(Vec::new()));
    let runner = WorkflowRunner::new(RecordingExecutor {
        calls: calls.clone(),
    });
    runner
        .run(&mut r, &CancellationToken::new(), noop_persist)
        .await
        .unwrap();
    assert_eq!(r.status, WorkflowRunStatus::Completed);
    assert_eq!(
        calls.lock().unwrap().clone(),
        vec!["start", "a", "b", "join"]
    );
}

#[tokio::test]
async fn runner_failure_stops_downstream() {
    let mut r = run(
        "start",
        vec![node("start"), node("fail"), node("after")],
        vec![edge("start", "fail"), edge("fail", "after")],
    );
    let runner = WorkflowRunner::new(MockExecutor);
    runner
        .run(&mut r, &CancellationToken::new(), noop_persist)
        .await
        .unwrap();
    assert_eq!(r.status, WorkflowRunStatus::Failed);
    assert_eq!(r.node(&id("fail")).unwrap().status, NodeRunStatus::Failed);
    assert_eq!(r.node(&id("after")).unwrap().status, NodeRunStatus::Pending);
}

#[tokio::test]
async fn runner_pauses_for_approval_and_does_not_run_downstream() {
    let mut r = run(
        "start",
        vec![node("start"), node("approve"), node("after")],
        vec![edge("start", "approve"), edge("approve", "after")],
    );
    let runner = WorkflowRunner::new(MockExecutor);
    runner
        .run(&mut r, &CancellationToken::new(), noop_persist)
        .await
        .unwrap();
    assert_eq!(r.status, WorkflowRunStatus::WaitingApproval);
    assert_eq!(
        r.node(&id("approve")).unwrap().status,
        NodeRunStatus::WaitingApproval
    );
    assert_eq!(r.node(&id("after")).unwrap().status, NodeRunStatus::Pending);
}

#[tokio::test]
async fn runner_cancellation_stops_remaining_nodes() {
    let mut r = run(
        "start",
        vec![
            node("start"),
            node("a"),
            node("b"),
            node("c"),
            node("output"),
        ],
        vec![
            edge("start", "a"),
            edge("a", "b"),
            edge("b", "c"),
            edge("c", "output"),
        ],
    );
    let cancel = CancellationToken::new();
    let runner = WorkflowRunner::new(CancellingExecutor {
        cancel: cancel.clone(),
        trigger: "b".to_string(),
    });
    runner.run(&mut r, &cancel, noop_persist).await.unwrap();
    assert_eq!(r.status, WorkflowRunStatus::Cancelled);
    assert_eq!(r.node(&id("a")).unwrap().status, NodeRunStatus::Completed);
    assert_eq!(r.node(&id("b")).unwrap().status, NodeRunStatus::Completed);
    assert_eq!(r.node(&id("c")).unwrap().status, NodeRunStatus::Cancelled);
    assert_eq!(
        r.node(&id("output")).unwrap().status,
        NodeRunStatus::Cancelled
    );
}

#[tokio::test]
async fn runner_checkpoints_run_state() {
    let temp_path = std::env::temp_dir().join(format!("yilian-runner-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&temp_path).unwrap();
    let mut r = run(
        "start",
        vec![node("start"), node("a"), node("output")],
        vec![edge("start", "a"), edge("a", "output")],
    );
    let run_id = r.run_id.clone();
    db.create_workflow_run("g1", &r).unwrap();

    let runner = WorkflowRunner::new(MockExecutor);
    runner
        .run(&mut r, &CancellationToken::new(), |run| {
            db.update_workflow_run("g1", run)
        })
        .await
        .unwrap();

    let stored = db.get_workflow_run(&run_id).unwrap().unwrap();
    assert_eq!(stored.run.status, WorkflowRunStatus::Completed);
    assert_eq!(stored.run.node_states, r.node_states);

    let _ = std::fs::remove_file(&temp_path);
}

#[tokio::test]
async fn runner_paused_run_has_consistent_ready_state() {
    let mut r = run(
        "start",
        vec![node("start"), node("approve"), node("after")],
        vec![edge("start", "approve"), edge("approve", "after")],
    );
    let runner = WorkflowRunner::new(MockExecutor);
    runner
        .run(&mut r, &CancellationToken::new(), noop_persist)
        .await
        .unwrap();
    assert_eq!(r.status, WorkflowRunStatus::WaitingApproval);
    assert!(r.ready_nodes().is_empty());
    assert_eq!(r.node(&id("after")).unwrap().status, NodeRunStatus::Pending);
}

// ============================================================
// Production executor tests (SecurityExecutionGateway routing).
// ============================================================

struct CountingTool {
    name: &'static str,
    executions: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for CountingTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        "counts executions for workflow executor tests"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object" })
    }

    async fn execute(&self, _args: serde_json::Value) -> ToolResult {
        self.executions.fetch_add(1, Ordering::SeqCst);
        ToolResult::success("executed")
    }
}

fn gateway_with_tool(
    name: &'static str,
    executions: Arc<AtomicUsize>,
) -> (Arc<SecurityExecutionGateway>, Database, std::path::PathBuf) {
    let db_path =
        std::env::temp_dir().join(format!("yilian-wf-gw-{name}-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&db_path).unwrap();
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CountingTool { name, executions }));
    let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
        SandboxConfig::default(),
        "workspace",
        Arc::new(registry),
    )
    .with_db(Arc::new(db.clone_connection()));
    (Arc::new(gateway), db, db_path)
}

fn subagent_gateway() -> (Arc<SecurityExecutionGateway>, std::path::PathBuf) {
    let db_path =
        std::env::temp_dir().join(format!("yilian-wf-subagent-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&db_path).unwrap();
    let definition = crate::server::DiscoveredSubagent {
        name: "researcher".to_string(),
        description: "research".to_string(),
        path: ".agents/agents/researcher/AGENT.md".to_string(),
        allowed_tools: vec!["read_file".to_string()],
        model: Some("deepseek-v4-flash".to_string()),
        workdir: None,
        instructions: "body".to_string(),
    };
    let adapter = crate::tools::SubagentToolAdapter::new(&definition).unwrap();
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(adapter));
    let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
        SandboxConfig::default(),
        "workspace",
        Arc::new(registry),
    )
    .with_db(Arc::new(db.clone_connection()));
    (Arc::new(gateway), db_path)
}

fn seed_restricted_subject(db: &Database, subject_id: &str) {
    let conn = db.conn();
    conn.execute(
        "INSERT OR IGNORE INTO security_subjects
            (subject_id, subject_type, provider, external_ref, display_name,
             status, created_at, updated_at)
         VALUES (?1, 'local_user', 'built_in', NULL, 'Restricted', 'active', 0, 0)",
        [subject_id],
    )
    .unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO security_role_bindings
            (binding_id, subject_id, role_key, source, effective_at, expires_at, revoked_at)
         VALUES (?1, ?2, 'restricted', 'built_in', 0, NULL, NULL)",
        rusqlite::params![format!("{subject_id}-restricted"), subject_id],
    )
    .unwrap();
}

fn run_with_subject(
    entry: &str,
    nodes: Vec<WorkflowNodeDefinition>,
    edges: Vec<WorkflowEdgeDefinition>,
    subject_id: &str,
) -> WorkflowRun {
    let ctx = ExecutionContext::new(
        ExecutionId::new("exec-1").unwrap(),
        subject_id,
        "researcher",
        None,
        1_000,
    );
    WorkflowRun::new(
        WorkflowRunId::generate(),
        ctx,
        graph(entry, nodes, edges),
        1_000,
    )
    .unwrap()
}

async fn run_with_gateway(gateway: Arc<SecurityExecutionGateway>, run: &mut WorkflowRun) {
    let approval_store = Arc::new(crate::safety::ApprovalStore::new());
    let runner = WorkflowRunner::new(SecurityGatewayNodeExecutor::new(gateway, approval_store));
    runner
        .run(run, &CancellationToken::new(), noop_persist)
        .await
        .unwrap();
}

#[tokio::test]
async fn production_tool_allow_executes_through_gateway() {
    let executions = Arc::new(AtomicUsize::new(0));
    let (gateway, _db, db_path) = gateway_with_tool("read_file", executions.clone());
    let mut r = run(
        "read",
        vec![tool_node(
            "read",
            "read_file",
            serde_json::json!({"path": "README.md"}),
        )],
        vec![],
    );
    run_with_gateway(gateway, &mut r).await;
    assert_eq!(r.status, WorkflowRunStatus::Completed);
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn production_high_risk_tool_pauses_for_approval() {
    let executions = Arc::new(AtomicUsize::new(0));
    let (gateway, _db, db_path) = gateway_with_tool("bash", executions.clone());
    let mut r = run(
        "bash",
        vec![tool_node(
            "bash",
            "bash",
            serde_json::json!({"command": "git push origin develop"}),
        )],
        vec![],
    );
    run_with_gateway(gateway, &mut r).await;
    assert_eq!(r.status, WorkflowRunStatus::WaitingApproval);
    assert_eq!(
        r.node(&id("bash")).unwrap().status,
        NodeRunStatus::WaitingApproval
    );
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn production_restricted_tool_denies() {
    let executions = Arc::new(AtomicUsize::new(0));
    let (gateway, db, db_path) = gateway_with_tool("write_file", executions.clone());
    db.conn()
        .execute(
            "UPDATE security_role_bindings SET role_key='restricted'
             WHERE subject_id='local-user' AND revoked_at IS NULL",
            [],
        )
        .unwrap();
    let mut r = run(
        "write",
        vec![tool_node(
            "write",
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "x"}),
        )],
        vec![],
    );
    run_with_gateway(gateway, &mut r).await;
    assert_eq!(r.status, WorkflowRunStatus::Failed);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn production_subagent_requires_approval() {
    let (gateway, db_path) = subagent_gateway();
    let mut r = run(
        "sub",
        vec![subagent_node("sub", "researcher", "研究当前仓库")],
        vec![],
    );
    run_with_gateway(gateway, &mut r).await;
    assert_eq!(r.status, WorkflowRunStatus::WaitingApproval);
    assert_eq!(
        r.node(&id("sub")).unwrap().status,
        NodeRunStatus::WaitingApproval
    );
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn production_unknown_tool_fails_closed() {
    let executions = Arc::new(AtomicUsize::new(0));
    let (gateway, _db, db_path) = gateway_with_tool("read_file", executions.clone());
    let mut r = run(
        "nope",
        vec![tool_node("nope", "does_not_exist", serde_json::json!({}))],
        vec![],
    );
    run_with_gateway(gateway, &mut r).await;
    assert_eq!(r.status, WorkflowRunStatus::Failed);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn production_nonexistent_subagent_fails_closed() {
    let (gateway, db_path) = subagent_gateway();
    let mut r = run(
        "sub",
        vec![subagent_node("sub", "ghost", "do something")],
        vec![],
    );
    run_with_gateway(gateway, &mut r).await;
    assert_eq!(r.status, WorkflowRunStatus::Failed);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn production_no_next_node_after_approval() {
    let executions = Arc::new(AtomicUsize::new(0));
    let (gateway, _db, db_path) = gateway_with_tool("bash", executions.clone());
    let mut r = run(
        "bash",
        vec![
            tool_node(
                "bash",
                "bash",
                serde_json::json!({"command": "git push origin develop"}),
            ),
            tool_node(
                "after",
                "read_file",
                serde_json::json!({"path": "README.md"}),
            ),
        ],
        vec![edge("bash", "after")],
    );
    run_with_gateway(gateway, &mut r).await;
    assert_eq!(r.status, WorkflowRunStatus::WaitingApproval);
    assert_eq!(r.node(&id("after")).unwrap().status, NodeRunStatus::Pending);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn production_subject_is_preserved() {
    let executions = Arc::new(AtomicUsize::new(0));
    let (gateway, db, db_path) = gateway_with_tool("write_file", executions.clone());
    seed_restricted_subject(&db, "restricted-user");
    let mut r = run_with_subject(
        "write",
        vec![tool_node(
            "write",
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "x"}),
        )],
        vec![],
        "restricted-user",
    );
    run_with_gateway(gateway, &mut r).await;
    // The run's subject is "restricted-user", not "local-user" (owner) — so the
    // write must be denied rather than allowed.
    assert_eq!(r.status, WorkflowRunStatus::Failed);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    let _ = std::fs::remove_file(&db_path);
}

// ============================================================
// Approval pause / resume tests.
// ============================================================

type PausedWorkflow = (
    Arc<SecurityExecutionGateway>,
    Arc<crate::safety::ApprovalStore>,
    Database,
    String,           // run_id
    String,           // approval_id
    Arc<AtomicUsize>, // executions
    std::path::PathBuf,
);

async fn run_paused_workflow() -> PausedWorkflow {
    let executions = Arc::new(AtomicUsize::new(0));
    let db_path =
        std::env::temp_dir().join(format!("yilian-wf-resume-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&db_path).unwrap();
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CountingTool {
        name: "bash",
        executions: executions.clone(),
    }));
    let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
        SandboxConfig::default(),
        "workspace",
        Arc::new(registry),
    )
    .with_db(Arc::new(db.clone_connection()));
    let gateway = Arc::new(gateway);
    let approval_store = Arc::new(crate::safety::ApprovalStore::new());

    let mut run = run(
        "bash",
        vec![tool_node(
            "bash",
            "bash",
            serde_json::json!({"command": "git push origin develop"}),
        )],
        vec![],
    );
    let run_id = run.run_id.to_string();
    db.create_workflow_run("g1", &run).unwrap();

    let runner = WorkflowRunner::new(SecurityGatewayNodeExecutor::new(
        Arc::clone(&gateway),
        Arc::clone(&approval_store),
    ));
    let db2 = db.clone_connection();
    runner
        .run(&mut run, &CancellationToken::new(), move |r| {
            db2.update_workflow_run("g1", r)
        })
        .await
        .unwrap();

    assert_eq!(run.status, WorkflowRunStatus::WaitingApproval);
    let pending = approval_store.list_pending();
    assert_eq!(pending.len(), 1);
    let approval_id = pending[0].approval_id.clone();

    (
        gateway,
        approval_store,
        db,
        run_id,
        approval_id,
        executions,
        db_path,
    )
}

#[tokio::test]
async fn workflow_approval_binds_run_and_node() {
    let (_gateway, approval_store, _db, run_id, approval_id, _exec, db_path) =
        run_paused_workflow().await;
    let approval = approval_store.get(&approval_id).unwrap();
    assert_eq!(approval.workflow_run_id.as_deref(), Some(run_id.as_str()));
    assert_eq!(approval.workflow_node_id.as_deref(), Some("bash"));
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn workflow_approve_resumes_and_completes() {
    let (gateway, approval_store, db, run_id, approval_id, executions, db_path) =
        run_paused_workflow().await;
    resolve_workflow_approval(
        &gateway,
        &approval_store,
        &db,
        &approval_id,
        "local-user",
        true,
        &CancellationToken::new(),
    )
    .await
    .unwrap();

    let stored = db
        .get_workflow_run(&WorkflowRunId::new(run_id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(stored.run.status, WorkflowRunStatus::Completed);
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn workflow_reject_stops_run() {
    let (gateway, approval_store, db, run_id, approval_id, executions, db_path) =
        run_paused_workflow().await;
    resolve_workflow_approval(
        &gateway,
        &approval_store,
        &db,
        &approval_id,
        "local-user",
        false,
        &CancellationToken::new(),
    )
    .await
    .unwrap();

    let stored = db
        .get_workflow_run(&WorkflowRunId::new(run_id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(stored.run.status, WorkflowRunStatus::Failed);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn workflow_double_approve_cannot_execute_twice() {
    let (gateway, approval_store, db, _run_id, approval_id, executions, db_path) =
        run_paused_workflow().await;
    resolve_workflow_approval(
        &gateway,
        &approval_store,
        &db,
        &approval_id,
        "local-user",
        true,
        &CancellationToken::new(),
    )
    .await
    .unwrap();

    // Second consume must fail (replay protection).
    assert!(resolve_workflow_approval(
        &gateway,
        &approval_store,
        &db,
        &approval_id,
        "local-user",
        true,
        &CancellationToken::new(),
    )
    .await
    .is_err());
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn workflow_subject_mismatch_rejected() {
    let (gateway, approval_store, db, _run_id, approval_id, executions, db_path) =
        run_paused_workflow().await;
    assert!(resolve_workflow_approval(
        &gateway,
        &approval_store,
        &db,
        &approval_id,
        "attacker",
        true,
        &CancellationToken::new(),
    )
    .await
    .is_err());
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn workflow_cancel_stops_run() {
    let (_gateway, approval_store, db, run_id, approval_id, executions, db_path) =
        run_paused_workflow().await;
    cancel_workflow_approval(&approval_store, &db, &approval_id, "local-user")
        .await
        .unwrap();

    let stored = db
        .get_workflow_run(&WorkflowRunId::new(run_id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(stored.run.status, WorkflowRunStatus::Cancelled);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn workflow_wrong_run_rejected() {
    let executions = Arc::new(AtomicUsize::new(0));
    let (gateway, db, db_path) = gateway_with_tool("bash", executions.clone());
    let approval_store = Arc::new(crate::safety::ApprovalStore::new());
    let approval = approval_store.create_workflow(
        "exec-x".to_string(),
        "non-existent-run".to_string(),
        "bash".to_string(),
        "call-1".to_string(),
        "bash".to_string(),
        serde_json::json!({"command": "x"}),
        crate::tools::RiskLevel::High,
        "reason".to_string(),
        "local-user".to_string(),
    );

    let result = resolve_workflow_approval(
        &gateway,
        &approval_store,
        &db,
        &approval.approval_id,
        "local-user",
        true,
        &CancellationToken::new(),
    )
    .await;
    assert!(result.is_err());
    let _ = std::fs::remove_file(&db_path);
}

// ============================================================
// Node results + LLM-only Agent node tests.
// ============================================================

#[test]
fn node_result_truncates_to_bounded_chars() {
    let long = "x".repeat(MAX_WORKFLOW_NODE_RESULT_CHARS * 2);
    let result = NodeRunResult::new(long);
    assert!(result.summary.chars().count() <= MAX_WORKFLOW_NODE_RESULT_CHARS);
}

#[test]
fn node_result_truncation_is_utf8_safe() {
    let long = "这是一个很长的中文结果。".repeat(4000);
    let result = NodeRunResult::new(long);
    assert!(result.summary.chars().count() <= MAX_WORKFLOW_NODE_RESULT_CHARS);
    assert!(result.summary.is_char_boundary(result.summary.len()));
}

#[test]
fn safe_tool_summary_replaces_binary_payload() {
    assert_eq!(
        safe_tool_result_summary("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAA..."),
        "二进制结果已生成"
    );
    assert_eq!(
        safe_tool_result_summary("data:audio/wav;base64,UklGR..."),
        "二进制结果已生成"
    );
    assert_eq!(safe_tool_result_summary("普通文本结果"), "普通文本结果");
}

struct ResultExecutor {
    summary: String,
}

#[async_trait]
impl WorkflowNodeExecutor for ResultExecutor {
    async fn execute(
        &self,
        _context: &ExecutionContext,
        _run_id: &WorkflowRunId,
        _node: &WorkflowNodeDefinition,
    ) -> Result<NodeExecutionOutcome, WorkflowExecutionError> {
        Ok(NodeExecutionOutcome::Completed {
            result: Some(NodeRunResult::new(self.summary.clone())),
        })
    }
}

#[tokio::test]
async fn runner_persists_node_result() {
    let mut r = run(
        "start",
        vec![node("start"), node("a")],
        vec![edge("start", "a")],
    );
    let runner = WorkflowRunner::new(ResultExecutor {
        summary: "研究完成".to_string(),
    });
    runner
        .run(&mut r, &CancellationToken::new(), noop_persist)
        .await
        .unwrap();
    assert_eq!(r.status, WorkflowRunStatus::Completed);
    assert_eq!(
        r.node(&id("a")).unwrap().result.as_ref().unwrap().summary,
        "研究完成"
    );
}

struct FailExecutor {
    message: String,
    fail_on: &'static str,
}

#[async_trait]
impl WorkflowNodeExecutor for FailExecutor {
    async fn execute(
        &self,
        _context: &ExecutionContext,
        _run_id: &WorkflowRunId,
        node: &WorkflowNodeDefinition,
    ) -> Result<NodeExecutionOutcome, WorkflowExecutionError> {
        if node.id.as_str() == self.fail_on {
            Ok(NodeExecutionOutcome::Failed {
                error: Some(self.message.clone()),
            })
        } else {
            Ok(NodeExecutionOutcome::Completed { result: None })
        }
    }
}

#[tokio::test]
async fn runner_persists_failed_error_summary() {
    let mut r = run(
        "start",
        vec![node("start"), node("a")],
        vec![edge("start", "a")],
    );
    let runner = WorkflowRunner::new(FailExecutor {
        message: "工具执行失败".to_string(),
        fail_on: "a",
    });
    runner
        .run(&mut r, &CancellationToken::new(), noop_persist)
        .await
        .unwrap();
    assert_eq!(r.status, WorkflowRunStatus::Failed);
    assert_eq!(
        r.node(&id("a")).unwrap().error.as_deref(),
        Some("工具执行失败")
    );
}

fn agent_node(id: &str, prompt: &str) -> WorkflowNodeDefinition {
    WorkflowNodeDefinition {
        id: WorkflowNodeId::new(id).unwrap(),
        kind: WorkflowNodeKind::Agent,
        config: WorkflowNodeConfig::Agent {
            prompt: prompt.to_string(),
        },
    }
}

struct FakeAgentExecutor {
    text: String,
    fail: bool,
    cancel_on_generate: Option<CancellationToken>,
}

#[async_trait]
impl WorkflowAgentExecutor for FakeAgentExecutor {
    async fn generate(&self, _prompt: &str) -> Result<String, WorkflowExecutionError> {
        if let Some(token) = &self.cancel_on_generate {
            token.cancel();
        }
        if self.fail {
            Err(WorkflowExecutionError::Execution("llm failed".to_string()))
        } else {
            Ok(self.text.clone())
        }
    }
}

fn empty_gateway_and_store() -> (
    Arc<SecurityExecutionGateway>,
    Arc<crate::safety::ApprovalStore>,
) {
    (
        Arc::new(SecurityExecutionGateway::new()),
        Arc::new(crate::safety::ApprovalStore::new()),
    )
}

#[tokio::test]
async fn agent_node_completes_with_text_result_and_no_approval() {
    let (gateway, store) = empty_gateway_and_store();
    let fake = FakeAgentExecutor {
        text: "研究结论".to_string(),
        fail: false,
        cancel_on_generate: None,
    };
    let executor = SecurityGatewayNodeExecutor::new(gateway, store.clone())
        .with_agent_executor(Arc::new(fake));
    let runner = WorkflowRunner::new(executor);
    let mut r = WorkflowRun::new(
        WorkflowRunId::generate(),
        ctx(),
        graph("think", vec![agent_node("think", "研究这个仓库")], vec![]),
        1_000,
    )
    .unwrap();
    runner
        .run(&mut r, &CancellationToken::new(), noop_persist)
        .await
        .unwrap();
    assert_eq!(r.status, WorkflowRunStatus::Completed);
    assert_eq!(
        r.node(&id("think"))
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .summary,
        "研究结论"
    );
    assert!(
        store.list_pending().is_empty(),
        "agent node must not create approvals"
    );
}

#[tokio::test]
async fn agent_node_llm_error_fails_node() {
    let (gateway, store) = empty_gateway_and_store();
    let fake = FakeAgentExecutor {
        text: String::new(),
        fail: true,
        cancel_on_generate: None,
    };
    let executor =
        SecurityGatewayNodeExecutor::new(gateway, store).with_agent_executor(Arc::new(fake));
    let runner = WorkflowRunner::new(executor);
    let mut r = WorkflowRun::new(
        WorkflowRunId::generate(),
        ctx(),
        graph("think", vec![agent_node("think", "研究")], vec![]),
        1_000,
    )
    .unwrap();
    runner
        .run(&mut r, &CancellationToken::new(), noop_persist)
        .await
        .unwrap();
    assert_eq!(r.status, WorkflowRunStatus::Failed);
    assert_eq!(r.node(&id("think")).unwrap().status, NodeRunStatus::Failed);
}

#[tokio::test]
async fn agent_node_cancellation_stops_downstream() {
    let cancel = CancellationToken::new();
    let (gateway, store) = empty_gateway_and_store();
    let fake = FakeAgentExecutor {
        text: "done".to_string(),
        fail: false,
        cancel_on_generate: Some(cancel.clone()),
    };
    let executor =
        SecurityGatewayNodeExecutor::new(gateway, store).with_agent_executor(Arc::new(fake));
    let runner = WorkflowRunner::new(executor);
    let mut r = WorkflowRun::new(
        WorkflowRunId::generate(),
        ctx(),
        graph(
            "think",
            vec![agent_node("think", "x"), node("a"), node("b")],
            vec![edge("think", "a"), edge("a", "b")],
        ),
        1_000,
    )
    .unwrap();
    runner.run(&mut r, &cancel, noop_persist).await.unwrap();
    assert_eq!(r.status, WorkflowRunStatus::Cancelled);
    assert_eq!(r.node(&id("a")).unwrap().status, NodeRunStatus::Cancelled);
    assert_eq!(r.node(&id("b")).unwrap().status, NodeRunStatus::Cancelled);
}

// ── Production LLM-only agent executor vs a local mock HTTP LLM server ──

use crate::config::types::ModelConfig;
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};

#[derive(Clone)]
struct MockChatState {
    responses: Arc<Mutex<Vec<serde_json::Value>>>,
}

async fn mock_chat_handler(
    State(state): State<MockChatState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut responses = state.responses.lock().unwrap();
    if responses.is_empty() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "no more responses" })),
        );
    }
    (StatusCode::OK, Json(responses.remove(0)))
}

async fn start_mock_chat_server(
    responses: Vec<serde_json::Value>,
) -> (String, tokio::task::JoinHandle<()>) {
    let state = MockChatState {
        responses: Arc::new(Mutex::new(responses)),
    };
    let app = Router::new()
        .route("/chat/completions", post(mock_chat_handler))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, handle)
}

fn mock_agent_config(base_url: &str) -> ModelConfig {
    ModelConfig {
        name: "mock-model".to_string(),
        base_url: base_url.to_string(),
        api_key: "mock-key".to_string(),
        invoke_timeout_ms: 5_000,
        ..Default::default()
    }
}

fn chat_response(content: Option<&str>, tool_calls: bool) -> serde_json::Value {
    let message = if tool_calls {
        serde_json::json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call-1",
                "type": "function",
                "function": { "name": "bash", "arguments": "{}" }
            }]
        })
    } else {
        serde_json::json!({
            "role": "assistant",
            "content": content,
        })
    };
    serde_json::json!({
        "id": "mock-1",
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": if tool_calls { "tool_calls" } else { "stop" }
        }]
    })
}

#[tokio::test]
async fn llm_agent_executor_returns_text_from_mock_server() {
    let (addr, _handle) =
        start_mock_chat_server(vec![chat_response(Some("研究完成"), false)]).await;
    let agent = LlmWorkflowAgentExecutor::new(&mock_agent_config(&addr), test_resolver());
    let text = agent.generate("研究").await.unwrap();
    assert_eq!(text, "研究完成");
}

#[tokio::test]
async fn llm_agent_executor_fails_closed_on_tool_calls() {
    let (addr, _handle) = start_mock_chat_server(vec![chat_response(None, true)]).await;
    let agent = LlmWorkflowAgentExecutor::new(&mock_agent_config(&addr), test_resolver());
    let error = agent.generate("研究").await.unwrap_err();
    assert!(
        error.to_string().contains("unsupported tool calls"),
        "got: {error}"
    );
}

#[tokio::test]
async fn llm_agent_executor_fails_closed_on_llm_error() {
    // Server returns 500 → LlmClient returns an Api error → node fails closed.
    let state = MockChatState {
        responses: Arc::new(Mutex::new(vec![])),
    };
    let app = Router::new()
        .route("/chat/completions", post(mock_chat_handler))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    let _handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let agent = LlmWorkflowAgentExecutor::new(&mock_agent_config(&addr), test_resolver());
    let error = agent.generate("研究").await.unwrap_err();
    assert!(
        matches!(error, WorkflowExecutionError::Execution(_)),
        "expected an execution error, got {error:?}"
    );
}
