use super::*;

use crate::modules::task::application::graph_service;

pub async fn list_graphs(State(server): State<Arc<AppServer>>) -> Response {
    let graphs = server.task_world.list_graphs();
    (StatusCode::OK, Json(json!({ "graphs": graphs }))).into_response()
}

pub async fn create_graph(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<CreateGraphRequest>,
) -> Response {
    let graph_id = match parse_graph_id(request.id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    match graph_service::create_task_graph(
        &server,
        graph_id,
        request.nodes,
        request.edges,
        request.goal,
    )
    .await
    {
        Ok(graph) => (StatusCode::CREATED, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => create_graph_error(error),
    }
}

pub async fn get_graph(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    match server.task_world.get_graph(&graph_id) {
        Some(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        None => runtime_error(TaskWorldRuntimeError::GraphNotFound(graph_id.to_string())),
    }
}

pub async fn get_graph_detail(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    if let Err(error) = graph_service::reconcile_active_commands(&server, &graph_id) {
        return runtime_error(error);
    }
    match server.task_world.get_graph_detail(&graph_id) {
        Ok(detail) => (StatusCode::OK, Json(json!({ "detail": detail }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteGraphQuery {
    pub expected_revision: u64,
}
pub async fn delete_graph(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<DeleteGraphQuery>,
) -> Response {
    let graph_id = match parse_graph_id(id) {
        Ok(id) => id,
        Err(e) => return runtime_error(e),
    };
    match server.task_world.delete_graph(
        &graph_id,
        query.expected_revision,
        chrono::Utc::now().timestamp_millis(),
    ) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e @ TaskWorldRuntimeError::StaleRevision { .. }) => {
            planning_error(StatusCode::CONFLICT, "revision_conflict", e.to_string())
        }
        Err(e @ TaskWorldRuntimeError::CommandExecutionActive { .. }) => {
            planning_error(StatusCode::CONFLICT, "graph_running", e.to_string())
        }
        Err(e @ TaskWorldRuntimeError::Harness(TaskHarnessError::ActiveExecution(_))) => {
            planning_error(StatusCode::CONFLICT, "graph_running", e.to_string())
        }
        Err(e) => runtime_error(e),
    }
}
