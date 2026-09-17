use serde_json::json;
use yilian_backend::task::{
    CanvasView, GraphRevision, TaskGraph, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind,
};

fn graph(revision: u64, ids: &[&str]) -> TaskGraph {
    let nodes = ids
        .iter()
        .map(|id| {
            TaskNode::new(
                TaskNodeId::new(*id).unwrap(),
                TaskNodeKind::Work,
                format!("Task {id}"),
                json!({"instruction": format!("Do {id}")}),
            )
            .unwrap()
        })
        .collect();
    TaskGraph::new(
        TaskGraphId::new("canvas-test").unwrap(),
        GraphRevision::new(revision).unwrap(),
        nodes,
        Vec::new(),
    )
    .unwrap()
}

#[test]
fn canvas_view_reconciles_without_rearranging_existing_manual_locations() {
    let original = graph(1, &["a", "b"]);
    let mut view = CanvasView::initial(&original, 100).unwrap();
    view.node_layouts
        .iter_mut()
        .find(|layout| layout.node_id.as_str() == "a")
        .unwrap()
        .x = 777.0;

    let changed = graph(2, &["a", "c"]);
    view.reconcile(&changed, 200).unwrap();

    assert_eq!(view.view_revision, 1);
    assert_eq!(view.graph_revision_seen, 2);
    assert_eq!(
        view.node_layouts
            .iter()
            .find(|layout| layout.node_id.as_str() == "a")
            .unwrap()
            .x,
        777.0
    );
    assert!(view
        .node_layouts
        .iter()
        .all(|layout| layout.node_id.as_str() != "b"));
    assert!(view
        .node_layouts
        .iter()
        .any(|layout| layout.node_id.as_str() == "c"));
}

#[test]
fn canvas_view_rejects_non_finite_and_unbounded_values() {
    let graph = graph(1, &["a"]);
    let mut view = CanvasView::initial(&graph, 100).unwrap();

    view.viewport.zoom = f64::NAN;
    assert!(view.validate_for_graph(&graph).is_err());

    let mut view = CanvasView::initial(&graph, 100).unwrap();
    view.viewport.zoom = 99.0;
    assert!(view.validate_for_graph(&graph).is_err());

    let mut view = CanvasView::initial(&graph, 100).unwrap();
    view.node_layouts[0].width = 10_000.0;
    assert!(view.validate_for_graph(&graph).is_err());
}
