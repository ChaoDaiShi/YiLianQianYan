use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::json;
use yilian_backend::db::Database;
use yilian_backend::shared::event::EventHub;
use yilian_backend::task::{
    CanvasViewError, GraphRevision, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind,
    TaskWorldRuntime, TaskWorldRuntimeError,
};

fn temp_database(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("yilian-cycle3-{label}-{}.db", uuid::Uuid::new_v4()))
}

fn remove_database(path: &Path) {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let candidate = if suffix.is_empty() {
            path.to_path_buf()
        } else {
            PathBuf::from(format!("{}{}", path.display(), suffix))
        };
        let _ = std::fs::remove_file(candidate);
    }
}

#[test]
fn canvas_view_roundtrips_independently_from_graph_revision() {
    let path = temp_database("view-roundtrip");
    let graph_id = TaskGraphId::new("canvas-persisted").unwrap();
    {
        let database = Database::new(&path).unwrap();
        let runtime = TaskWorldRuntime::new(&database, EventHub::new(8)).unwrap();
        runtime
            .create_graph(
                graph_id.clone(),
                vec![TaskNode::new(
                    TaskNodeId::new("root").unwrap(),
                    TaskNodeKind::Work,
                    "Root",
                    json!({"instruction": "prepare"}),
                )
                .unwrap()],
                Vec::new(),
                100,
            )
            .unwrap();

        let mut view = runtime.get_canvas_view(&graph_id).unwrap();
        view.node_layouts[0].x = 901.0;
        let saved = runtime
            .save_canvas_view(&graph_id, view, 1, 200)
            .expect("view write succeeds");

        assert_eq!(saved.view_revision, 2);
        assert_eq!(
            runtime.get_graph(&graph_id).unwrap().revision,
            GraphRevision::initial()
        );
    }

    let database = Database::new(&path).unwrap();
    let runtime = TaskWorldRuntime::new(&database, EventHub::new(8)).unwrap();
    let view = runtime.get_canvas_view(&graph_id).unwrap();
    assert_eq!(view.view_revision, 2);
    assert_eq!(view.node_layouts[0].x, 901.0);
    assert_eq!(
        runtime.get_graph(&graph_id).unwrap().revision,
        GraphRevision::initial()
    );
    remove_database(&path);
}

#[test]
fn canvas_migration_registers_v1_owner_and_both_projection_tables() {
    let path = temp_database("migration");
    {
        let database = Database::new(&path).unwrap();
        TaskWorldRuntime::new(&database, EventHub::new(8)).unwrap();
    }

    let connection = Connection::open(&path).unwrap();
    let migration: (String, String) = connection
        .query_row(
            "SELECT name, owner FROM schema_migrations WHERE version=1001",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        migration,
        ("1001_task_canvas_views".into(), "v1_task_world".into())
    );
    for table in ["task_canvas_views", "task_graph_revision_history"] {
        let exists: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "missing migration table {table}");
    }
    remove_database(&path);
}

#[test]
fn corrupt_canvas_json_fails_closed_when_view_is_loaded() {
    let path = temp_database("corrupt-json");
    let graph_id = TaskGraphId::new("corrupt-canvas").unwrap();
    {
        let database = Database::new(&path).unwrap();
        let runtime = TaskWorldRuntime::new(&database, EventHub::new(8)).unwrap();
        runtime
            .create_graph(
                graph_id.clone(),
                vec![TaskNode::new(
                    TaskNodeId::new("root").unwrap(),
                    TaskNodeKind::Work,
                    "Root",
                    json!({}),
                )
                .unwrap()],
                Vec::new(),
                100,
            )
            .unwrap();
        let view = runtime.get_canvas_view(&graph_id).unwrap();
        runtime
            .save_canvas_view(&graph_id, view, 1, 200)
            .expect("initial canvas write succeeds");
    }

    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE task_canvas_views SET viewport_json='not-json' WHERE graph_id=?1",
            [&graph_id.to_string()],
        )
        .unwrap();
    drop(connection);

    let database = Database::new(&path).unwrap();
    let runtime = TaskWorldRuntime::new(&database, EventHub::new(8)).unwrap();
    let error = runtime.get_canvas_view(&graph_id).unwrap_err();
    assert!(matches!(error, TaskWorldRuntimeError::Persistence(_)));
    remove_database(&path);
}

#[test]
fn canvas_write_rejects_a_view_for_another_graph() {
    let path = temp_database("identity");
    let graph_id = TaskGraphId::new("identity-canvas").unwrap();
    let other_graph_id = TaskGraphId::new("other-canvas").unwrap();
    let database = Database::new(&path).unwrap();
    let runtime = TaskWorldRuntime::new(&database, EventHub::new(8)).unwrap();
    runtime
        .create_graph(
            graph_id.clone(),
            vec![TaskNode::new(
                TaskNodeId::new("root").unwrap(),
                TaskNodeKind::Work,
                "Root",
                json!({}),
            )
            .unwrap()],
            Vec::new(),
            100,
        )
        .unwrap();
    let mut view = runtime.get_canvas_view(&graph_id).unwrap();
    view.graph_id = other_graph_id;
    let error = runtime
        .save_canvas_view(&graph_id, view, 1, 200)
        .unwrap_err();
    assert!(matches!(
        error,
        TaskWorldRuntimeError::Canvas(CanvasViewError::GraphIdentityMismatch)
    ));
    remove_database(&path);
}

#[test]
fn semantic_revision_history_is_bounded_to_the_latest_thirty_two_entries() {
    let path = temp_database("history-bound");
    let graph_id = TaskGraphId::new("history-canvas").unwrap();
    let database = Database::new(&path).unwrap();
    let runtime = TaskWorldRuntime::new(&database, EventHub::new(8)).unwrap();
    runtime
        .create_graph(
            graph_id.clone(),
            vec![TaskNode::new(
                TaskNodeId::new("root").unwrap(),
                TaskNodeKind::Work,
                "Root",
                json!({}),
            )
            .unwrap()],
            Vec::new(),
            100,
        )
        .unwrap();

    for index in 0..40 {
        let node_id = format!("node-{index}");
        runtime
            .add_node(
                &graph_id,
                TaskNode::new(
                    TaskNodeId::new(node_id).unwrap(),
                    TaskNodeKind::Work,
                    format!("Node {index}"),
                    json!({}),
                )
                .unwrap(),
                index as u64 + 1,
                index as i64 + 101,
            )
            .unwrap();
    }

    let detail = runtime.get_graph_detail(&graph_id).unwrap();
    assert_eq!(detail.revisions.len(), 32);
    assert_eq!(detail.revisions.first().unwrap().revision, 10);
    assert_eq!(detail.revisions.last().unwrap().revision, 41);
    remove_database(&path);
}
