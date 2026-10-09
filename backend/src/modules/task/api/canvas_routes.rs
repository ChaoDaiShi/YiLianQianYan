use super::*;

pub async fn get_canvas_view(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    match server.task_world.get_canvas_view(&graph_id) {
        Ok(view) => (StatusCode::OK, Json(json!({ "view": view }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn put_canvas_view(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
    Json(request): Json<UpdateCanvasViewRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let updated_at = now();
    let view = CanvasView {
        schema_version: request.schema_version,
        graph_id: graph_id.clone(),
        view_revision: request
            .view_revision
            .unwrap_or(request.expected_view_revision),
        graph_revision_seen: request.graph_revision_seen,
        viewport: request.viewport,
        node_layouts: request.node_layouts,
        selection: request.selection,
        groups: request.groups,
        updated_at,
    };
    match server.task_world.save_canvas_view(
        &graph_id,
        view,
        request.expected_view_revision,
        updated_at,
    ) {
        Ok(view) => (StatusCode::OK, Json(json!({ "view": view }))).into_response(),
        Err(error) => runtime_error(error),
    }
}
