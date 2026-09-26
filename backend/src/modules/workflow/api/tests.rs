use super::*;

use crate::execution::{ExecutionContext, ExecutionId};
use crate::modules::workflow::application::run_service::execute_for_task_harness;
use crate::safety::ControlSession;
use crate::server::AppServer;
use crate::workflow::{
    NodeRunStatus, WorkflowEdgeDefinition, WorkflowGraphDefinition, WorkflowNodeConfig,
    WorkflowNodeDefinition, WorkflowNodeId, WorkflowNodeKind, WorkflowRun, WorkflowRunId,
    WorkflowRunStatus, WORKFLOW_GRAPH_SCHEMA_VERSION,
};
use tokio_util::sync::CancellationToken;

fn test_server() -> (std::path::PathBuf, Arc<AppServer>) {
    let path = std::env::temp_dir().join(format!("yilian-wf-api-{}.db", uuid::Uuid::new_v4()));
    let server =
        AppServer::new_with_control_session(&path, ".", ControlSession::generate()).unwrap();
    (path, Arc::new(server))
}

fn output_graph() -> WorkflowGraphDefinition {
    WorkflowGraphDefinition {
        schema_version: WORKFLOW_GRAPH_SCHEMA_VERSION,
        entry_node_id: WorkflowNodeId::new("done").unwrap(),
        nodes: vec![WorkflowNodeDefinition {
            id: WorkflowNodeId::new("done").unwrap(),
            kind: WorkflowNodeKind::Output,
            config: WorkflowNodeConfig::Output { template: None },
        }],
        edges: vec![],
    }
}

#[tokio::test]
async fn create_valid_graph_succeeds() {
    let (path, server) = test_server();
    let body = CreateWorkflowGraphRequest {
        name: Some("g1".to_string()),
        description: None,
        definition: output_graph(),
    };
    let result = create_workflow_graph(State(server.clone()), Json(body)).await;
    assert!(result.is_ok());
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn create_invalid_graph_is_rejected() {
    let (path, server) = test_server();
    let mut definition = output_graph();
    definition.edges.push(WorkflowEdgeDefinition {
        from: WorkflowNodeId::new("done").unwrap(),
        to: WorkflowNodeId::new("done").unwrap(),
    });
    let body = CreateWorkflowGraphRequest {
        name: None,
        description: None,
        definition,
    };
    let result = create_workflow_graph(State(server), Json(body)).await;
    assert!(result.is_err());
    let _ = std::fs::remove_file(&path);
}

// ── Async run lifecycle ──

async fn create_graph_id(server: &Arc<AppServer>) -> String {
    let body = CreateWorkflowGraphRequest {
        name: Some("g".to_string()),
        description: None,
        definition: output_graph(),
    };
    let result = create_workflow_graph(State(server.clone()), Json(body))
        .await
        .unwrap();
    result.0["id"].as_str().unwrap().to_string()
}

async fn wait_until_terminal(server: &Arc<AppServer>, run_id: &str) {
    let run_id = WorkflowRunId::new(run_id.to_string()).unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(stored) = server.db.get_workflow_run(&run_id).unwrap() {
            if stored.run.status.is_terminal() {
                return;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "run did not reach a terminal state in time"
        );
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
}

#[tokio::test]
async fn run_start_returns_immediately_and_completes() {
    let (path, server) = test_server();
    let graph_id = create_graph_id(&server).await;

    let result = run_workflow_graph(State(server.clone()), Path(graph_id.clone()))
        .await
        .unwrap();
    let run_id = result.0["run_id"].as_str().unwrap().to_string();
    assert_eq!(
        result.0["status"], "created",
        "start must return the created status"
    );

    // The run must be persisted immediately (visible before terminal).
    let run_id_typed = WorkflowRunId::new(run_id.clone()).unwrap();
    assert!(server.db.get_workflow_run(&run_id_typed).unwrap().is_some());

    wait_until_terminal(&server, &run_id).await;

    let stored = server.db.get_workflow_run(&run_id_typed).unwrap().unwrap();
    assert_eq!(stored.run.status, WorkflowRunStatus::Completed);

    // The active-run registry must be cleaned up after the terminal state.
    assert!(
        server.active_workflow_runs.lock().is_empty(),
        "active workflow run registry must be empty after terminal"
    );

    // The run view exposes the Output node result.
    let view = get_workflow_run(State(server.clone()), Path(run_id.clone()))
        .await
        .unwrap();
    assert_eq!(view.0["status"], "completed");
    assert_eq!(view.0["workflow_graph_id"], graph_id);
    assert_eq!(view.0["nodes"][0]["result"]["summary"], "工作流执行完成");
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn task_harness_execution_uses_the_callers_cancel_token() {
    let (path, server) = test_server();
    let graph_id = create_graph_id(&server).await;
    let cancel = CancellationToken::new();
    cancel.cancel();

    let result = execute_for_task_harness(server.clone(), &graph_id, cancel, None).await;

    assert_eq!(result, Err("workflow cancelled".to_string()));
    assert!(server.active_workflow_runs.lock().is_empty());
    let _ = std::fs::remove_file(&path);
}

fn paused_run_in_db(server: &Arc<AppServer>, graph_id: &str) -> (String, String) {
    let now = chrono::Utc::now().timestamp_millis();
    let ctx = ExecutionContext::new(
        ExecutionId::generate(),
        "local-user",
        "workflow-runner",
        None,
        now,
    );
    let mut run = WorkflowRun::new(WorkflowRunId::generate(), ctx, output_graph(), now).unwrap();
    let node_id = WorkflowNodeId::new("done").unwrap();
    run.transition_node(&node_id, NodeRunStatus::Running, now)
        .unwrap();
    run.transition_node(&node_id, NodeRunStatus::WaitingApproval, now)
        .unwrap();
    let run_id = run.run_id.to_string();
    server.db.create_workflow_run(graph_id, &run).unwrap();
    server.db.update_workflow_run(graph_id, &run).unwrap();

    let approval = server.approval_store.create_workflow(
        run.execution_context.execution_id.to_string(),
        run.run_id.to_string(),
        "done".to_string(),
        "tool-call-1".to_string(),
        "bash".to_string(),
        serde_json::json!({"command": "echo x"}),
        crate::tools::RiskLevel::High,
        "high-risk".to_string(),
        "local-user".to_string(),
    );
    (run_id, approval.approval_id)
}

#[tokio::test]
async fn cancel_workflow_run_cancels_active_token_and_persists() {
    let (path, server) = test_server();
    let graph_id = create_graph_id(&server).await;
    let (run_id, approval_id) = paused_run_in_db(&server, &graph_id);

    // Simulate an active runner registered in the registry.
    let token = CancellationToken::new();
    server
        .active_workflow_runs
        .lock()
        .insert(run_id.clone(), token.clone());

    let result = cancel_workflow_run(State(server.clone()), Path(run_id.clone()))
        .await
        .unwrap();
    assert_eq!(result.0["status"], "cancelled");
    assert!(
        token.is_cancelled(),
        "active runner token must be cancelled"
    );
    assert!(
        server.active_workflow_runs.lock().is_empty(),
        "cancelled run must be removed from the active registry"
    );

    let stored = server
        .db
        .get_workflow_run(&WorkflowRunId::new(run_id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(stored.run.status, WorkflowRunStatus::Cancelled);

    // The bound pending approval is also cancelled.
    assert_eq!(
        server
            .approval_store
            .get(&approval_id)
            .unwrap()
            .status
            .to_string(),
        "cancelled"
    );
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn cancel_terminal_run_is_stable() {
    let (path, server) = test_server();
    let graph_id = create_graph_id(&server).await;

    let start = run_workflow_graph(State(server.clone()), Path(graph_id.clone()))
        .await
        .unwrap();
    let run_id = start.0["run_id"].as_str().unwrap().to_string();
    wait_until_terminal(&server, &run_id).await;

    // Cancelling a completed run is a stable no-op returning the same state.
    let result = cancel_workflow_run(State(server.clone()), Path(run_id.clone()))
        .await
        .unwrap();
    assert_eq!(result.0["status"], "completed");
    let stored = server
        .db
        .get_workflow_run(&WorkflowRunId::new(run_id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(stored.run.status, WorkflowRunStatus::Completed);
    let _ = std::fs::remove_file(&path);
}

// ── Run history ──

fn persist_run(
    server: &Arc<AppServer>,
    graph_id: &str,
    status: WorkflowRunStatus,
    ts: i64,
) -> String {
    let ctx = ExecutionContext::new(
        ExecutionId::generate(),
        "local-user",
        "workflow-runner",
        None,
        ts,
    );
    let mut run = WorkflowRun::new(WorkflowRunId::generate(), ctx, output_graph(), ts).unwrap();
    let node_id = WorkflowNodeId::new("done").unwrap();
    // Force the run status directly so history filters can target it.
    run.node_mut(&node_id).unwrap().status = match status {
        WorkflowRunStatus::Completed => NodeRunStatus::Completed,
        WorkflowRunStatus::Failed => NodeRunStatus::Failed,
        WorkflowRunStatus::WaitingApproval => NodeRunStatus::WaitingApproval,
        WorkflowRunStatus::Cancelled => NodeRunStatus::Cancelled,
        _ => NodeRunStatus::Running,
    };
    run.status = status;
    run.updated_at = ts;
    let run_id = run.run_id.to_string();
    server.db.create_workflow_run(graph_id, &run).unwrap();
    run_id
}

#[tokio::test]
async fn list_workflow_runs_orders_newest_first_and_filters() {
    let (path, server) = test_server();
    let graph_a = create_graph_id(&server).await;
    let graph_b = create_graph_id(&server).await;

    let r1 = persist_run(&server, &graph_a, WorkflowRunStatus::Completed, 100);
    let r2 = persist_run(&server, &graph_a, WorkflowRunStatus::Failed, 200);
    let r3 = persist_run(&server, &graph_b, WorkflowRunStatus::Completed, 300);

    // All runs, newest first.
    let all = list_workflow_runs(
        State(server.clone()),
        Query(ListWorkflowRunsParams::default()),
    )
    .await;
    let runs = all.0["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0]["run_id"], r3);
    assert_eq!(runs[1]["run_id"], r2);
    assert_eq!(runs[2]["run_id"], r1);

    // Filter by graph id.
    let filtered = list_workflow_runs(
        State(server.clone()),
        Query(ListWorkflowRunsParams {
            workflow_graph_id: Some(graph_a.clone()),
            status: None,
            limit: None,
            offset: None,
        }),
    )
    .await;
    let runs = filtered.0["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    assert!(runs.iter().all(|r| r["workflow_graph_id"] == graph_a));

    // Filter by status.
    let failed = list_workflow_runs(
        State(server.clone()),
        Query(ListWorkflowRunsParams {
            workflow_graph_id: None,
            status: Some("failed".to_string()),
            limit: None,
            offset: None,
        }),
    )
    .await;
    let runs = failed.0["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["run_id"], r2);

    // Limit is applied and clamped.
    let limited = list_workflow_runs(
        State(server.clone()),
        Query(ListWorkflowRunsParams {
            workflow_graph_id: None,
            status: None,
            limit: Some(2),
            offset: None,
        }),
    )
    .await;
    let runs = limited.0["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);

    let _ = std::fs::remove_file(&path);
}
