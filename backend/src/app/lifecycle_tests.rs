use super::*;
use crate::browser::BrowserRuntime;
use crate::integrations::secret::InMemorySecretStore;
use crate::modules::task::validation::ValidationPolicy;
use crate::modules::task::{
    ExecutorResolver, TaskExecutionControlState, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind,
    TaskWorldRuntime,
};
use crate::shared::event::EventHub;
use serde_json::json;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn shutdown_persists_active_and_waiting_graphs_for_the_next_run() {
    let root = std::env::temp_dir().join(format!("yilian-exit-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("bundle")).unwrap();
    let server = AppServer::new_with_control_session_store_and_browser(
        &root.join("exit.sqlite"),
        root.to_str().unwrap(),
        ControlSession::new("s".repeat(64)).unwrap(),
        Arc::new(InMemorySecretStore::new()),
        Arc::new(BrowserRuntime::for_local_fixture(root.join("bundle"))),
    )
    .unwrap();
    let task_world = &server.task_world;
    let [running, waiting, goal_chat, idle, completed] =
        ["running", "waiting", "goal-chat", "idle", "completed"]
            .map(|id| TaskGraphId::new(id).unwrap());
    let node = TaskNodeId::new("one").unwrap();
    let provider = "capability://fixture.wait";
    for graph in [&running, &waiting, &goal_chat, &idle, &completed] {
        let input = if graph == &waiting {
            json!({"executor_ref":provider})
        } else {
            json!({})
        };
        task_world
            .create_graph(
                graph.clone(),
                vec![TaskNode::new(node.clone(), TaskNodeKind::Work, "One", input).unwrap()],
                vec![],
                1,
            )
            .unwrap();
    }
    let attempt = task_world.start_execution(&running, &node, 1, 2).unwrap();
    let lease = task_world
        .claim_execution_dispatch(&running, &attempt.id)
        .unwrap();
    server
        .audit_recorder
        .record(crate::safety::AuditEventInput {
            event_type: crate::safety::AuditEventType::ExecutionStarted,
            correlation_id: attempt.id.to_string(),
            request_id: attempt.id.to_string(),
            subject_id: "local-user".into(),
            role_key: "owner".into(),
            tool_call_id: Some(attempt.id.to_string()),
            tool_name: Some("fixture".into()),
            actions: vec!["mcp.invoke".into()],
            ..Default::default()
        })
        .unwrap();
    let mut resolver = ExecutorResolver::new();
    resolver.register_capability("fixture.wait");
    let approval = task_world
        .start_execution_with_resolver(&waiting, &node, 1, resolver, 2)
        .unwrap();
    let approval_lease = task_world
        .claim_execution_dispatch(&waiting, &approval.id)
        .unwrap();
    task_world
        .mark_execution_waiting_approval(&waiting, &approval.id, "fixture-approval", 3)
        .unwrap();
    drop(approval_lease);
    let completed_attempt = task_world.start_execution(&completed, &node, 1, 2).unwrap();
    task_world
        .complete_execution(
            &completed,
            &completed_attempt.id,
            json!({"status":"success"}),
            ValidationPolicy::StructuredResult,
            3,
        )
        .unwrap();
    let chat = CancellationToken::new();
    let _goal = task_world
        .register_goal_chat(&goal_chat, chat.clone())
        .unwrap();
    let ordinary_chat = CancellationToken::new();
    server
        .active_tasks
        .lock()
        .insert("ordinary-chat".into(), ordinary_chat.clone());

    shutdown(&server).await;

    assert!(lease.cancellation_token().is_cancelled());
    assert!(chat.is_cancelled());
    assert!(ordinary_chat.is_cancelled());
    assert!(task_world.start_execution(&idle, &node, 1, 4).is_err());
    drop(lease);
    drop(_goal);
    drop(server);

    let db = crate::db::Database::new(&root.join("exit.sqlite")).unwrap();
    let reopened = TaskWorldRuntime::new(&db, EventHub::new(16)).unwrap();
    for graph in [&running, &waiting, &goal_chat] {
        assert_eq!(
            reopened.execution_control(graph).unwrap().state,
            TaskExecutionControlState::Paused,
            "{graph}"
        );
    }
    for graph in [&idle, &completed] {
        assert_eq!(
            reopened.execution_control(graph).unwrap().state,
            TaskExecutionControlState::Running,
            "{graph}"
        );
    }
    assert_eq!(
        reopened.execution_control(&waiting).unwrap().paused_nodes,
        vec![node]
    );
    assert_eq!(
        reopened
            .find_execution(&attempt.id)
            .unwrap()
            .1
            .failure_code(),
        Some("external_mutation_uncertain")
    );
    assert_eq!(
        reopened
            .find_execution(&approval.id)
            .unwrap()
            .1
            .failure_code(),
        Some("paused")
    );
}
