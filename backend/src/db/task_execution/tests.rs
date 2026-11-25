use super::*;
use crate::modules::task::{NodeContext, TaskGraphId, TaskNodeId};
use serde_json::json;
use std::path::Path;

fn context() -> NodeContext {
    NodeContext::new(
        "goal",
        "instructions",
        vec!["resource://one".to_string()],
        Vec::new(),
        Vec::new(),
        vec!["capability.read".to_string()],
        Vec::new(),
        vec!["status == success".to_string()],
    )
    .unwrap()
}

fn execution(id: &str, attempt: u32) -> NodeExecution {
    NodeExecution::new(
        NodeExecutionId::new(id).unwrap(),
        TaskGraphId::new("graph").unwrap(),
        TaskNodeId::new("node").unwrap(),
        attempt,
        None,
        context(),
        100 + i64::from(attempt),
    )
    .unwrap()
}

fn database() -> Database {
    let database = Database::new(Path::new(":memory:")).unwrap();
    database.initialize_task_world_schema().unwrap();
    database.initialize_task_execution_schema().unwrap();
    database
}

#[test]
fn migration_1002_is_registered_in_the_v1_namespace() {
    let database = database();
    let conn = database.conn();
    let metadata: (String, String) = conn
        .query_row(
            "SELECT name, owner FROM schema_migrations WHERE version=1002",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        metadata,
        (
            "1002_task_node_executions".to_string(),
            "v1_task_world".to_string()
        )
    );
}

#[test]
fn repeated_attempts_coexist_as_append_only_rows() {
    let database = database();
    let first = execution("execution-1", 1);
    let second = execution("execution-2", 2);

    database.save_node_execution(&first).unwrap();
    database.save_node_execution(&second).unwrap();

    let history = database
        .load_node_executions(
            &TaskGraphId::new("graph").unwrap(),
            &TaskNodeId::new("node").unwrap(),
        )
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].attempt, 1);
    assert_eq!(history[1].attempt, 2);
    assert_ne!(history[0].id, history[1].id);
}

#[test]
fn active_attempts_reload_as_recovery_required_not_running() {
    let database = database();
    let mut active = execution("execution-active", 1);
    active.transition(NodeExecutionStatus::Ready, 101).unwrap();
    active
        .transition(NodeExecutionStatus::Dispatching, 102)
        .unwrap();
    active
        .transition(NodeExecutionStatus::Running, 103)
        .unwrap();
    database.save_node_execution(&active).unwrap();

    let recovered = database.recover_node_executions(200).unwrap();
    assert_eq!(recovered, 1);
    let loaded = database
        .load_node_execution(&NodeExecutionId::new("execution-active").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(loaded.status, NodeExecutionStatus::Failed);
    assert_eq!(loaded.failure_code(), Some("execution_interrupted"));
    assert_eq!(loaded.output, None);
    assert_eq!(json!(loaded.validation), json!(null));
}
