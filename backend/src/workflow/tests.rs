// ============================================================
// Workflow DAG definition & validation tests.
// ============================================================

use super::*;

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
