use super::*;

pub async fn execute_command(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, node_id)): Path<(String, String)>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let node_id = match TaskNodeId::new(node_id) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server.task_world.execute_node(
        &graph_id,
        &node_id,
        request.expected_revision,
        &server.command_router,
        now(),
    ) {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn cancel_command(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, node_id)): Path<(String, String)>,
    Json(request): Json<CancelCommandRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let node_id = match TaskNodeId::new(node_id) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server.task_world.cancel_command(
        &graph_id,
        &node_id,
        request.expected_revision,
        &request.request_id,
        &server.command_router,
        now(),
    ) {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn checkpoint(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    match server
        .task_world
        .checkpoint(&graph_id, request.expected_revision, now())
    {
        Ok(checkpoint) => (
            StatusCode::CREATED,
            Json(json!({ "checkpoint": checkpoint })),
        )
            .into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn restore(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
    Json(request): Json<RestoreRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    match server.task_world.restore(
        &graph_id,
        &request.checkpoint_id,
        request.expected_revision,
        now(),
    ) {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}
