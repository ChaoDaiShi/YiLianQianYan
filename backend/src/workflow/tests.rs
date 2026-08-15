// ============================================================
// Workflow DAG definition & validation tests.
// ============================================================

use super::*;
use crate::execution::{ExecutionContext, ExecutionId};

fn node(id: &str) -> WorkflowNodeDefinition {
    WorkflowNodeDefinition {
        id: WorkflowNodeId::new(id).unwrap(),
        kind: WorkflowNodeKind::Agent,
    }
}

fn node_of_kind(id: &str, kind: WorkflowNodeKind) -> WorkflowNodeDefinition {
    WorkflowNodeDefinition {
        id: WorkflowNodeId::new(id).unwrap(),
        kind,
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
