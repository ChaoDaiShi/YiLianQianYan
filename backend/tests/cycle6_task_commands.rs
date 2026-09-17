//! Narrow Cycle 6 v1 tests for persistent Task command control.

use std::path::Path;

use serde_json::json;
use yilian_backend::db::Database;
use yilian_backend::safety::ControlSession;
use yilian_backend::server::AppServer;
use yilian_backend::shared::command::{CommandRequest, CommandRouter, CommandStatus};
use yilian_backend::shared::event::EventHub;
use yilian_backend::task::validation::ValidationPolicy;
use yilian_backend::task::{
    GraphRevision, NodeExecutionStatus, RetryPolicy, TaskCommandService, TaskExecutionControlState,
    TaskGraph, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind, TaskWorldRuntime,
};

fn id(raw: &str) -> TaskNodeId {
    TaskNodeId::new(raw).expect("valid node id")
}

fn graph_id() -> TaskGraphId {
    TaskGraphId::new("cycle6-task").expect("valid graph id")
}

fn setup() -> (Database, EventHub, TaskWorldRuntime, TaskGraphId) {
    let database = Database::new(Path::new(":memory:")).expect("database opens");
    let events = EventHub::new(32);
    let runtime = TaskWorldRuntime::new(&database, events.clone()).expect("runtime loads");
    let graph = TaskGraph::new(
        graph_id(),
        GraphRevision::initial(),
        vec![
            TaskNode::new(id("done"), TaskNodeKind::Work, "已验证步骤", json!({})).unwrap(),
            TaskNode::new(id("active"), TaskNodeKind::Work, "可暂停步骤", json!({}))
                .unwrap()
                .with_retry_policy(RetryPolicy::new(2).unwrap()),
            TaskNode::new(id("pending"), TaskNodeKind::Work, "等待步骤", json!({})).unwrap(),
        ],
        vec![],
    )
    .expect("valid graph");
    runtime
        .create_graph(graph.id.clone(), graph.nodes, graph.edges, 100)
        .expect("graph creates");
    (database, events, runtime, graph_id())
}

#[test]
fn pause_persists_blocks_new_dispatch_and_preserves_verified_nodes() {
    let (database, events, runtime, graph) = setup();
    let done = runtime
        .start_execution(&graph, &id("done"), 1, 101)
        .expect("done attempt starts");
    runtime
        .complete_execution(
            &graph,
            &done.id,
            json!({"ok": true}),
            ValidationPolicy::StructuredResult,
            102,
        )
        .expect("done attempt verifies");
    let active = runtime
        .start_execution(&graph, &id("active"), 1, 103)
        .expect("active attempt starts");

    let service = TaskCommandService::new(runtime.clone());
    let projection = service.pause(&graph, 104).expect("pause succeeds");

    assert_eq!(projection.overall_status, "paused");
    assert_eq!(
        runtime
            .execution_control(&graph)
            .expect("control persists")
            .state,
        TaskExecutionControlState::Paused
    );
    assert_eq!(
        runtime
            .find_execution(&active.id)
            .expect("active attempt remains queryable")
            .1
            .status,
        NodeExecutionStatus::Cancelled
    );
    let detail = runtime.get_graph_detail(&graph).expect("detail loads");
    assert_eq!(
        detail.nodes[0].status,
        yilian_backend::task::TaskNodeStatus::Succeeded
    );
    assert!(
        service
            .status(&graph, 105)
            .expect("status loads")
            .attention_required
    );

    assert!(runtime
        .start_execution(&graph, &id("pending"), 1, 106)
        .is_err());

    let reloaded = TaskWorldRuntime::new(&database, events).expect("runtime reloads");
    assert_eq!(
        reloaded
            .execution_control(&graph)
            .expect("persisted control reloads")
            .state,
        TaskExecutionControlState::Paused
    );
}

#[test]
fn resume_uses_persisted_control_and_continues_pending_and_paused_work() {
    let (_database, _events, runtime, graph) = setup();
    let active = runtime
        .start_execution(&graph, &id("active"), 1, 101)
        .expect("active attempt starts");
    let service = TaskCommandService::new(runtime.clone());
    service.pause(&graph, 102).expect("pause succeeds");

    let projection = service.resume(&graph, 103).expect("resume succeeds");
    assert_eq!(projection.overall_status, "working");
    assert_eq!(
        runtime
            .execution_control(&graph)
            .expect("control loads")
            .state,
        TaskExecutionControlState::Running
    );

    let pending_attempts = runtime
        .list_node_executions(&graph, &id("pending"))
        .expect("pending attempts load");
    assert_eq!(pending_attempts.len(), 1);
    assert_eq!(pending_attempts[0].attempt, 1);
    let active_attempts = runtime
        .list_node_executions(&graph, &id("active"))
        .expect("active attempts load");
    assert_eq!(active_attempts.len(), 2);
    assert_eq!(active_attempts[1].attempt, active.attempt + 1);
    assert_eq!(active_attempts[1].status, NodeExecutionStatus::Dispatching);
}

#[test]
fn resume_redrives_ready_work_even_when_control_is_already_running() {
    let (_database, _events, runtime, graph) = setup();
    let service = TaskCommandService::new(runtime.clone());

    let projection = service.resume(&graph, 101).expect("resume succeeds");
    assert_eq!(projection.overall_status, "working");
    let attempts = runtime
        .list_node_executions(&graph, &id("pending"))
        .expect("ready work is persisted");
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].status, NodeExecutionStatus::Dispatching);
}

#[test]
fn retry_redrives_other_ready_nodes_without_losing_the_new_attempt() {
    let (_database, _events, runtime, graph) = setup();
    let failed = runtime
        .start_execution(&graph, &id("active"), 1, 101)
        .expect("active attempt starts");
    runtime
        .fail_execution(&graph, &failed.id, "provider_error", "temporary", 102)
        .expect("active attempt fails");

    let service = TaskCommandService::new(runtime.clone());
    let projection = service
        .retry(&graph, &id("active"), 103)
        .expect("retry succeeds");
    assert_eq!(projection.overall_status, "working");
    let active_attempts = runtime
        .list_node_executions(&graph, &id("active"))
        .expect("active attempts load");
    assert_eq!(active_attempts.len(), 2);
    assert_eq!(active_attempts[1].status, NodeExecutionStatus::Dispatching);
    let pending_attempts = runtime
        .list_node_executions(&graph, &id("pending"))
        .expect("pending attempts load");
    assert_eq!(pending_attempts.len(), 1);
    assert_eq!(pending_attempts[0].status, NodeExecutionStatus::Dispatching);
}

#[test]
fn rerun_redrives_the_stale_branch_and_keeps_prior_attempt_history() {
    let (_database, _events, runtime, graph) = setup();
    let done = runtime
        .start_execution(&graph, &id("done"), 1, 101)
        .expect("done attempt starts");
    runtime
        .complete_execution(
            &graph,
            &done.id,
            json!({"verified": true}),
            ValidationPolicy::StructuredResult,
            102,
        )
        .expect("done attempt verifies");
    let service = TaskCommandService::new(runtime.clone());

    service
        .rerun(&graph, &id("done"), 103)
        .expect("rerun succeeds");
    let attempts = runtime
        .list_node_executions(&graph, &id("done"))
        .expect("rerun attempts load");
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].status, NodeExecutionStatus::Stale);
    assert_eq!(attempts[1].status, NodeExecutionStatus::Dispatching);
}

#[test]
fn current_command_returns_task_and_current_node_semantics() {
    let (_database, _events, runtime, graph) = setup();
    runtime
        .start_execution(&graph, &id("active"), 1, 101)
        .expect("active attempt starts");
    let service = TaskCommandService::new(runtime);

    let current = service.current(&graph, 102).expect("current loads");
    assert_eq!(current.task_id, graph.as_str());
    assert_eq!(current.current_node_id.as_deref(), Some("active"));
    assert_eq!(current.current_node_summary.as_deref(), Some("可暂停步骤"));
}

#[test]
fn stable_task_commands_are_registered_with_bounded_payloads() {
    let (_database, _events, runtime, graph) = setup();
    let service = TaskCommandService::new(runtime);
    let router = CommandRouter::new();
    service
        .register(&router)
        .expect("task command handlers register");

    for command in ["task.status", "task.current"] {
        let result = router.execute(CommandRequest::new(
            command,
            format!("request-{command}"),
            "cycle6-test",
            json!({"graph_id": graph.as_str()}),
        ));
        assert_eq!(result.status, CommandStatus::Succeeded, "{command}");
    }

    let missing = router.execute(CommandRequest::new(
        "task.status",
        "request-missing",
        "cycle6-test",
        json!({}),
    ));
    assert_eq!(missing.status, CommandStatus::Failed);
    assert_eq!(missing.error.unwrap().code, "invalid_payload");
}

#[test]
fn cancel_and_rerun_commands_do_not_require_internal_supervisor_access() {
    let (_database, _events, runtime, graph) = setup();
    let service = TaskCommandService::new(runtime);
    let router = CommandRouter::new();
    service.register(&router).expect("handlers register");

    let cancel = router.execute(CommandRequest::new(
        "task.cancel",
        "request-cancel",
        "cycle6-test",
        json!({"graph_id": graph.as_str()}),
    ));
    assert_eq!(cancel.status, CommandStatus::Succeeded);

    let rerun = router.execute(CommandRequest::new(
        "task.rerun",
        "request-rerun",
        "cycle6-test",
        json!({"graph_id": graph.as_str(), "node_id": "pending"}),
    ));
    assert_eq!(rerun.status, CommandStatus::Failed);
    assert_eq!(rerun.error.unwrap().code, "task_cancelled");
}

#[test]
fn production_server_registers_task_and_approval_commands() {
    let path = std::env::temp_dir().join(format!("cycle6-task-server-{}.db", uuid::Uuid::new_v4()));
    let server = AppServer::new_with_control_session(&path, ".", ControlSession::generate())
        .expect("production server constructs");
    let graph = TaskGraph::new(
        TaskGraphId::new("cycle6-production-task").unwrap(),
        GraphRevision::initial(),
        vec![TaskNode::new(id("current"), TaskNodeKind::Work, "当前节点", json!({})).unwrap()],
        vec![],
    )
    .expect("graph valid");
    server
        .task_world
        .create_graph(graph.id.clone(), graph.nodes, graph.edges, 100)
        .expect("graph persists");

    for command in [
        "task.pause",
        "task.resume",
        "task.cancel",
        "task.retry",
        "task.rerun",
        "task.status",
        "task.current",
    ] {
        let mut payload = json!({"graph_id": graph.id.as_str()});
        if matches!(command, "task.retry" | "task.rerun") {
            payload["node_id"] = json!("current");
        }
        let result = server.command_router.execute(CommandRequest::new(
            command,
            format!("request-{command}"),
            "cycle6-test",
            payload,
        ));
        assert_ne!(result.status, CommandStatus::NotFound, "{command}");
    }

    let approval = server.command_router.execute(CommandRequest::new(
        "task.approval.resolve",
        "request-approval-registration",
        "cycle6-test",
        json!({"resolution": "approve", "conversation_id": "missing"}),
    ));
    assert_eq!(approval.status, CommandStatus::Failed);
    assert_eq!(approval.error.unwrap().code, "approval_not_found");
    std::fs::remove_file(path).ok();
}
