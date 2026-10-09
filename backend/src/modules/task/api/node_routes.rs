use super::*;

pub async fn add_node(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
    Json(request): Json<AddNodeRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    if let Err(error) = crate::modules::task::application::capability_binding::validate_node_input(
        &server,
        &request.node.input,
    )
    .await
    {
        return runtime_error(error);
    }
    match server
        .task_world
        .add_node(&graph_id, request.node, request.expected_revision, now())
    {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn update_node(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, node_id)): Path<(String, String)>,
    Json(request): Json<UpdateNodeRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let node_id = match TaskNodeId::new(node_id) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    let node = match TaskNode::new(node_id, request.kind, request.title, request.input) {
        Ok(node) => node.with_retry_policy(request.retry_policy),
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    if let Err(error) = crate::modules::task::application::capability_binding::validate_node_input(
        &server,
        &node.input,
    )
    .await
    {
        return runtime_error(error);
    }
    match server
        .task_world
        .update_node(&graph_id, node, request.expected_revision, now())
    {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn delete_node(
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
    match server
        .task_world
        .delete_node(&graph_id, &node_id, request.expected_revision, now())
    {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn add_edge(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
    Json(request): Json<AddEdgeRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let from = match TaskNodeId::new(request.from) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    let to = match TaskNodeId::new(request.to) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server.task_world.add_edge(
        &graph_id,
        TaskEdge::new(from, to),
        request.expected_revision,
        now(),
    ) {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn delete_edge(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, from, to)): Path<(String, String, String)>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let from = match TaskNodeId::new(from) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    let to = match TaskNodeId::new(to) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server.task_world.delete_edge(
        &graph_id,
        &TaskEdge::new(from, to),
        request.expected_revision,
        now(),
    ) {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn start_node(
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
    match server
        .task_world
        .start_node(&graph_id, &node_id, request.expected_revision, now())
    {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}
