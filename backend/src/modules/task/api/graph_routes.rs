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
