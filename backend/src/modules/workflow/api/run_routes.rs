use super::*;

use crate::modules::workflow::application::run_service;

/// POST /api/workflow-graphs/:id/run — create a run, persist it, register it as
/// active, spawn the runner, and return immediately with the run id.
///
/// The HTTP request never waits for the workflow to finish; the frontend polls
/// `GET /api/workflow-runs/:run_id` for progress.
pub async fn run_workflow_graph(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let started = run_service::start_workflow_run(&server, &id)
        .await
        .map_err(run_service_error)?;
    Ok(Json(serde_json::json!({
        "run_id": started.run_id,
        "execution_id": started.execution_id,
        "status": started.status,
    })))
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
    let (graph_id, run) = run_service::cancel_workflow_run(&server, &run_id)
        .await
        .map_err(run_service_error)?;
    Ok(Json(run_view(&graph_id, &run)))
}
