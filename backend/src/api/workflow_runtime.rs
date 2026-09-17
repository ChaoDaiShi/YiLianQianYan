// ============================================================
// Workflow runtime API — graph CRUD + run lifecycle.
//
// Graph definitions are validated before they are written. Running a graph
// resolves the security subject server-side (never from the client) and
// executes through the Security Execution Gateway. Run state is read back from
// persistence; approval pause/resume reuses the existing approval surface.
// ============================================================

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::agent::verifier::DefaultVerifier;
use crate::db::{WorkflowGraphRecord, WorkflowRunQuery};
use crate::execution::{ExecutionContext, ExecutionId};
use crate::safety::{SecurityExecutionGateway, SecuritySubject};
use crate::server::AppServer;
use crate::workflow::{
    LlmWorkflowAgentExecutor, SecurityGatewayNodeExecutor, WorkflowAgentExecutor,
    WorkflowGraphDefinition, WorkflowRun, WorkflowRunId, WorkflowRunner,
};
use tokio_util::sync::CancellationToken;

#[derive(serde::Deserialize)]
pub struct CreateWorkflowGraphRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub definition: WorkflowGraphDefinition,
}

#[derive(serde::Deserialize)]
pub struct UpdateWorkflowGraphRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub definition: Option<WorkflowGraphDefinition>,
}

fn run_view(graph_id: &str, run: &WorkflowRun) -> serde_json::Value {
    serde_json::json!({
        "run_id": run.run_id.as_str(),
        "workflow_graph_id": graph_id,
        "status": run.status.to_string(),
        "created_at": run.created_at,
        "updated_at": run.updated_at,
        "execution_id": run.execution_context.execution_id.as_str(),
        "subject_id": run.execution_context.subject_id,
        "nodes": run.node_states.iter().map(|s| serde_json::json!({
            "node_id": s.node_id.as_str(),
            "status": s.status.to_string(),
            "started_at": s.started_at,
            "finished_at": s.finished_at,
            "error": s.error,
            "result": s.result.as_ref().map(|r| serde_json::json!({ "summary": r.summary })),
        })).collect::<Vec<_>>(),
    })
}

fn graph_view(record: &WorkflowGraphRecord) -> serde_json::Value {
    serde_json::json!({
        "id": record.id,
        "name": record.name,
        "description": record.description,
        "definition": record.definition,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    })
}

pub async fn list_workflow_graphs(State(server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    let graphs = server.db.list_workflow_graphs().unwrap_or_default();
    Json(serde_json::json!({
        "graphs": graphs.iter().map(graph_view).collect::<Vec<_>>(),
    }))
}

pub async fn create_workflow_graph(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<CreateWorkflowGraphRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    body.definition
        .validate()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let now = chrono::Utc::now().timestamp_millis();
    let record = WorkflowGraphRecord {
        id: uuid::Uuid::new_v4().to_string(),
        name: body.name.unwrap_or_else(|| "未命名工作流图".to_string()),
        description: body.description.unwrap_or_default(),
        definition: body.definition,
        created_at: now,
        updated_at: now,
    };
    server
        .db
        .create_workflow_graph(&record)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(graph_view(&record)))
}

pub async fn get_workflow_graph(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let record = server
        .db
        .get_workflow_graph(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "工作流图不存在".to_string()))?;
    Ok(Json(graph_view(&record)))
}

pub async fn update_workflow_graph(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateWorkflowGraphRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let existing = server
        .db
        .get_workflow_graph(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "工作流图不存在".to_string()))?;

    let now = chrono::Utc::now().timestamp_millis();
    let record = WorkflowGraphRecord {
        id: existing.id.clone(),
        name: body.name.unwrap_or(existing.name),
        description: body.description.unwrap_or(existing.description),
        definition: body.definition.unwrap_or(existing.definition),
        created_at: existing.created_at,
        updated_at: now,
    };
    record
        .definition
        .validate()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    server
        .db
        .update_workflow_graph(&id, &record)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(graph_view(&record)))
}

pub async fn delete_workflow_graph(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    server
        .db
        .delete_workflow_graph(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(serde_json::json!({"status": "deleted"})))
}

/// Query params for `GET /api/workflow-runs`.
#[derive(Debug, Deserialize, Default)]
pub struct ListWorkflowRunsParams {
    pub workflow_graph_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// POST /api/workflow-graphs/:id/run — create a run, persist it, register it as
/// active, spawn the runner, and return immediately with the run id.
///
/// The HTTP request never waits for the workflow to finish; the frontend polls
/// `GET /api/workflow-runs/:run_id` for progress.
pub async fn run_workflow_graph(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let graph = server
        .db
        .get_workflow_graph(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "工作流图不存在".to_string()))?;

    // Subject is resolved server-side (the local desktop user), never client-supplied.
    let now = chrono::Utc::now().timestamp_millis();
    let ctx = ExecutionContext::new(
        ExecutionId::generate(),
        SecuritySubject::local_user().subject_id,
        "workflow-runner",
        None,
        now,
    );
    let run = WorkflowRun::new(
        WorkflowRunId::generate(),
        ctx,
        graph.definition.clone(),
        now,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let run_id = run.run_id.clone();
    let execution_id = run.execution_context.execution_id.clone();
    let initial_status = run.status.to_string();
    let run_id_str = run_id.to_string();

    // Persist immediately so the run is visible even before the runner starts.
    server
        .db
        .create_workflow_run(&id, &run)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // Register an active-run cancel token.
    let token = CancellationToken::new();
    server
        .active_workflow_runs
        .lock()
        .insert(run_id_str.clone(), token.clone());

    // Build runtime components before spawning.
    let gateway = build_gateway(&server).await;
    let agent_executor = build_agent_executor(&server);
    let executor =
        SecurityGatewayNodeExecutor::new(Arc::clone(&gateway), Arc::clone(&server.approval_store))
            .with_agent_executor(agent_executor);
    let runner = WorkflowRunner::new(executor);

    let db = server.db.clone_connection();
    let graph_id = id.clone();
    let active_runs = server.active_workflow_runs.clone();
    let mut run = run;
    tokio::spawn(async move {
        let result = runner
            .run(&mut run, &token, |r| db.update_workflow_run(&graph_id, r))
            .await;
        // Terminal cleanup: always remove the active-run entry.
        active_runs.lock().remove(&run_id_str);
        if let Err(error) = result {
            tracing::error!(run_id = %run_id_str, error = %error, "workflow run ended with an execution error");
        }
    });

    Ok(Json(serde_json::json!({
        "run_id": run_id.as_str(),
        "execution_id": execution_id.as_str(),
        "status": initial_status,
    })))
}

/// Execute an existing WorkflowGraph for Task Harness without introducing a
/// second workflow state machine. The same persisted WorkflowRun,
/// SecurityExecutionGateway and WorkflowRunner used by the HTTP runtime are
/// retained; Task Harness only observes the bounded terminal result.
pub(crate) async fn execute_for_task_harness(
    server: Arc<AppServer>,
    workflow_graph_id: &str,
) -> Result<Option<serde_json::Value>, String> {
    let graph = server
        .db
        .get_workflow_graph(workflow_graph_id)?
        .ok_or_else(|| format!("workflow graph not found: {workflow_graph_id}"))?;
    let now = chrono::Utc::now().timestamp_millis();
    let context = ExecutionContext::new(
        ExecutionId::generate(),
        SecuritySubject::local_user().subject_id,
        "task-harness-workflow",
        None,
        now,
    );
    let mut run = WorkflowRun::new(
        WorkflowRunId::generate(),
        context,
        graph.definition.clone(),
        now,
    )
    .map_err(|error| error.to_string())?;
    let run_id = run.run_id.clone();
    server.db.create_workflow_run(workflow_graph_id, &run)?;
    let cancel = CancellationToken::new();
    server
        .active_workflow_runs
        .lock()
        .insert(run_id.to_string(), cancel.clone());

    let gateway = build_gateway(&server).await;
    let executor =
        SecurityGatewayNodeExecutor::new(Arc::clone(&gateway), Arc::clone(&server.approval_store))
            .with_agent_executor(build_agent_executor(&server));
    let runner = WorkflowRunner::new(executor);
    let db = server.db.clone_connection();
    let graph_id = workflow_graph_id.to_string();
    let result = runner
        .run(&mut run, &cancel, move |state| {
            db.update_workflow_run(&graph_id, state)
        })
        .await;
    server.active_workflow_runs.lock().remove(run_id.as_str());
    result.map_err(|error| error.to_string())?;

    match run.status {
        crate::workflow::WorkflowRunStatus::Completed => Ok(Some(serde_json::json!({
            "status": "completed",
            "completed": true,
            "workflow_graph_id": workflow_graph_id,
            "workflow_run_id": run_id.as_str(),
        }))),
        crate::workflow::WorkflowRunStatus::WaitingApproval => {
            Err("workflow paused for approval".to_string())
        }
        crate::workflow::WorkflowRunStatus::Cancelled => Err("workflow cancelled".to_string()),
        crate::workflow::WorkflowRunStatus::Failed => Err("workflow failed".to_string()),
        status => Err(format!("workflow ended in non-terminal status {status}")),
    }
}

pub async fn get_workflow_run(
    State(server): State<Arc<AppServer>>,
    Path(run_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let run_id =
        WorkflowRunId::new(run_id).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let stored = server
        .db
        .get_workflow_run(&run_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "工作流运行不存在".to_string()))?;
    let graph_id = stored.workflow_graph_id.clone();
    Ok(Json(run_view(&graph_id, &stored.run)))
}

/// GET /api/workflow-runs — list persisted runs (recent history), newest first.
pub async fn list_workflow_runs(
    State(server): State<Arc<AppServer>>,
    Query(params): Query<ListWorkflowRunsParams>,
) -> Json<serde_json::Value> {
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let query = WorkflowRunQuery {
        workflow_graph_id: params.workflow_graph_id,
        status: params.status,
        limit: Some(limit),
        offset: params.offset,
    };
    match server.db.list_workflow_runs(&query) {
        Ok(stored) => Json(serde_json::json!({
            "runs": stored.iter().map(|s| run_view(&s.workflow_graph_id, &s.run)).collect::<Vec<_>>(),
        })),
        Err(error) => {
            tracing::error!(error = %error, "failed to list workflow runs");
            Json(serde_json::json!({ "runs": [] }))
        }
    }
}

pub async fn cancel_workflow_run(
    State(server): State<Arc<AppServer>>,
    Path(run_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let run_id =
        WorkflowRunId::new(run_id).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let stored = server
        .db
        .get_workflow_run(&run_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "工作流运行不存在".to_string()))?;
    let (graph_id, mut run) = (stored.workflow_graph_id.clone(), stored.run);

    // Terminal runs: cancel is a stable no-op that returns the current state.
    if matches!(
        run.status,
        crate::workflow::WorkflowRunStatus::Completed
            | crate::workflow::WorkflowRunStatus::Failed
            | crate::workflow::WorkflowRunStatus::Cancelled
    ) {
        return Ok(Json(run_view(&graph_id, &run)));
    }

    // Signal the active runner (if any) to stop scheduling further nodes.
    if let Some(token) = server.active_workflow_runs.lock().get(run_id.as_str()) {
        token.cancel();
    }
    // Remove from the active registry so a later re-run isn't blocked.
    server.active_workflow_runs.lock().remove(run_id.as_str());

    // Cancel any pending approval bound to this run.
    for approval in server.approval_store.list_pending() {
        if approval.workflow_run_id.as_deref() == Some(run_id.as_str()) {
            let _ = server
                .approval_store
                .cancel(&approval.approval_id, &approval.conversation_id);
        }
    }

    // Persist the terminal Cancelled state (the spawned runner's persist closure
    // will also see the cancelled token and persist — idempotent overwrite).
    run.cancel(chrono::Utc::now().timestamp_millis());
    server
        .db
        .update_workflow_run(&graph_id, &run)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(run_view(&graph_id, &run)))
}

async fn build_gateway(server: &AppServer) -> Arc<SecurityExecutionGateway> {
    let tool_registry = server.build_agent_tool_registry().await;
    let config = server.config.read().clone();
    Arc::new(
        SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            config.sandbox.clone(),
            server.workspace_root.clone(),
            Arc::clone(&tool_registry),
            Arc::new(DefaultVerifier::new(&server.workspace_root)),
            Arc::new(server.audit_recorder.clone()),
        )
        .with_db(Arc::new(server.db.clone_connection()))
        .with_grant_enforcement(),
    )
}

/// Build the LLM-only agent executor from the configured model. Node config can
/// never supply secrets — model/provider/base_url all come from app config.
fn build_agent_executor(server: &AppServer) -> Arc<dyn WorkflowAgentExecutor> {
    let config = server.config.read().clone();
    Arc::new(LlmWorkflowAgentExecutor::new(
        &config.model,
        Arc::clone(&server.secret_resolver),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::{ExecutionContext, ExecutionId};
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

    fn paused_run_in_db(server: &Arc<AppServer>, graph_id: &str) -> (String, String) {
        let now = chrono::Utc::now().timestamp_millis();
        let ctx = ExecutionContext::new(
            ExecutionId::generate(),
            "local-user",
            "workflow-runner",
            None,
            now,
        );
        let mut run =
            WorkflowRun::new(WorkflowRunId::generate(), ctx, output_graph(), now).unwrap();
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
}
