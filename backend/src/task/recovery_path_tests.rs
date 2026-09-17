use super::*;
use crate::{db::Database, shared::event::EventHub};
use serde_json::json;
use std::path::Path;

fn setup() -> (
    Database,
    TaskWorldRuntime,
    EventHub,
    TaskGraphId,
    TaskNodeId,
    TaskNodeId,
) {
    let db = Database::new(Path::new(":memory:")).unwrap();
    let events = EventHub::new(64);
    let runtime = TaskWorldRuntime::new(&db, events.clone()).unwrap();
    let graph = TaskGraphId::new("review-recovery").unwrap();
    let a = TaskNodeId::new("a").unwrap();
    let b = TaskNodeId::new("b").unwrap();
    let nodes = [&a, &b]
        .into_iter()
        .map(|id| TaskNode::new(id.clone(), TaskNodeKind::Work, id.to_string(), json!({})).unwrap())
        .collect();
    runtime
        .create_graph(
            graph.clone(),
            nodes,
            vec![TaskEdge::new(a.clone(), b.clone())],
            1,
        )
        .unwrap();
    (db, runtime, events, graph, a, b)
}

fn complete(runtime: &TaskWorldRuntime, graph: &TaskGraphId, node: &TaskNodeId) {
    let execution = runtime.start_execution(graph, node, 1, 2).unwrap();
    runtime
        .complete_execution(
            graph,
            &execution.id,
            json!({"ok":true}),
            validation::ValidationPolicy::StructuredResult,
            3,
        )
        .unwrap();
}

#[test]
fn review_rejected_rerun_preserves_all_live_and_persisted_history() {
    let (db, runtime, events, graph, a, b) = setup();
    complete(&runtime, &graph, &a);
    runtime.start_execution(&graph, &b, 1, 4).unwrap();
    let before = serde_json::to_value(runtime.get_graph_detail(&graph).unwrap()).unwrap();
    let rows = serde_json::to_value(db.load_all_node_executions().unwrap()).unwrap();
    let mut receiver = events.subscribe();
    assert!(runtime.prepare_rerun_from_node(&graph, &a, 1, 5).is_err());
    assert_eq!(
        serde_json::to_value(runtime.get_graph_detail(&graph).unwrap()).unwrap(),
        before
    );
    assert_eq!(
        serde_json::to_value(db.load_all_node_executions().unwrap()).unwrap(),
        rows
    );
    assert!(
        receiver.try_recv().is_err(),
        "rejected recovery emitted a started fact"
    );
}

#[test]
fn review_rerun_persistence_failure_rolls_back_every_attempt() {
    let (db, runtime, events, graph, a, b) = setup();
    complete(&runtime, &graph, &a);
    complete(&runtime, &graph, &b);
    let before = serde_json::to_value(runtime.get_graph_detail(&graph).unwrap()).unwrap();
    let rows = serde_json::to_value(db.load_all_node_executions().unwrap()).unwrap();
    db.conn().execute_batch("CREATE TRIGGER fail_second_attempt BEFORE UPDATE ON task_node_executions WHEN OLD.node_id='b' BEGIN SELECT RAISE(ABORT, 'injected recovery write failure'); END;").unwrap();
    let mut receiver = events.subscribe();
    assert!(runtime.prepare_rerun_from_node(&graph, &a, 1, 5).is_err());
    assert_eq!(
        serde_json::to_value(runtime.get_graph_detail(&graph).unwrap()).unwrap(),
        before
    );
    assert_eq!(
        serde_json::to_value(db.load_all_node_executions().unwrap()).unwrap(),
        rows
    );
    assert!(receiver.try_recv().is_err());
}
