// ============================================================
// Workflow runtime API — graph CRUD + run lifecycle.
//
// Graph definitions are validated before they are written. Running a graph
// resolves the security subject server-side (never from the client) and
// executes through the Security Execution Gateway. Run state is read back from
// persistence; approval pause/resume reuses the existing approval surface.
// ============================================================

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::agent::verifier::DefaultVerifier;
use crate::db::WorkflowGraphRecord;
use crate::execution::{ExecutionContext, ExecutionId};
use crate::safety::{SecurityExecutionGateway, SecuritySubject};
use crate::server::AppServer;
use crate::workflow::{
    SecurityGatewayNodeExecutor, WorkflowGraphDefinition, WorkflowRun, WorkflowRunId,
    WorkflowRunner,
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

fn run_view(run: &WorkflowRun) -> serde_json::Value {
    serde_json::json!({
        "run_id": run.run_id.as_str(),
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

/// POST /api/workflow-graphs/:id/run — create a run and execute it to
/// completion / pause / failure, returning the final run state.
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
    let mut run = WorkflowRun::new(
        WorkflowRunId::generate(),
        ctx,
        graph.definition.clone(),
        now,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    server
        .db
        .create_workflow_run(&id, &run)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let gateway = build_gateway(&server).await;
    let executor =
        SecurityGatewayNodeExecutor::new(Arc::clone(&gateway), Arc::clone(&server.approval_store));
    let runner = WorkflowRunner::new(executor);
    let db = server.db.clone_connection();
    let graph_id = id.clone();
    runner
        .run(&mut run, &CancellationToken::new(), move |r| {
            db.update_workflow_run(&graph_id, r)
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(run_view(&run)))
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
    Ok(Json(run_view(&stored.run)))
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
    let (graph_id, mut run) = (stored.workflow_graph_id, stored.run);

    // Cancel any pending approval bound to this run.
    for approval in server.approval_store.list_pending() {
        if approval.workflow_run_id.as_deref() == Some(run_id.as_str()) {
            let _ = server
                .approval_store
                .cancel(&approval.approval_id, &approval.conversation_id);
        }
    }

    run.cancel(chrono::Utc::now().timestamp_millis());
    server
        .db
        .update_workflow_run(&graph_id, &run)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(run_view(&run)))
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
        .with_db(Arc::new(server.db.clone_connection())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::ControlSession;
    use crate::server::AppServer;
    use crate::workflow::{
        WorkflowEdgeDefinition, WorkflowNodeConfig, WorkflowNodeDefinition, WorkflowNodeId,
        WorkflowNodeKind, WORKFLOW_GRAPH_SCHEMA_VERSION,
    };

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
}
