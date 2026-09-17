use super::task_graph::*;

use serde_json::json;

fn graph_id(raw: &str) -> TaskGraphId {
    TaskGraphId::new(raw).expect("valid graph id")
}

fn node_id(raw: &str) -> TaskNodeId {
    TaskNodeId::new(raw).expect("valid node id")
}

fn node(raw: &str) -> TaskNode {
    TaskNode::new(
        node_id(raw),
        TaskNodeKind::Work,
        format!("node {raw}"),
        json!({ "source": raw }),
    )
    .expect("valid task node")
}

fn edge(from: &str, to: &str) -> TaskEdge {
    TaskEdge::new(node_id(from), node_id(to))
}

fn graph(nodes: Vec<TaskNode>, edges: Vec<TaskEdge>) -> TaskGraph {
    TaskGraph::new(graph_id("graph-1"), GraphRevision::initial(), nodes, edges)
        .expect("valid task graph")
}

#[test]
fn graph_accepts_multiple_roots_and_returns_deterministic_adjacency() {
    let graph = graph(
        vec![node("root-a"), node("root-b"), node("join")],
        vec![edge("root-a", "join"), edge("root-b", "join")],
    );

    assert_eq!(
        graph.dependencies(&node_id("join")),
        vec![node_id("root-a"), node_id("root-b")]
    );
    assert_eq!(graph.dependents(&node_id("root-a")), vec![node_id("join")]);
}

#[test]
fn graph_rejects_duplicate_edges() {
    let result = TaskGraph::new(
        graph_id("graph-1"),
        GraphRevision::initial(),
        vec![node("a"), node("b")],
        vec![edge("a", "b"), edge("a", "b")],
    );

    assert!(matches!(
        result,
        Err(TaskGraphValidationError::DuplicateEdge { .. })
    ));
}

#[test]
fn graph_rejects_cycles() {
    let result = TaskGraph::new(
        graph_id("graph-1"),
        GraphRevision::initial(),
        vec![node("a"), node("b")],
        vec![edge("a", "b"), edge("b", "a")],
    );

    assert!(matches!(
        result,
        Err(TaskGraphValidationError::CycleDetected(_))
    ));
}
