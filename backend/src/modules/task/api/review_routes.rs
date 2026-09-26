use super::*;

pub async fn review_graph(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
    Json(request): Json<ReviewGraphRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let graph = match server.task_world.get_graph(&graph_id) {
        Some(graph) => graph,
        None => return runtime_error(TaskWorldRuntimeError::GraphNotFound(graph_id.to_string())),
    };
    if graph.revision.value() != request.expected_revision {
        return runtime_error(TaskWorldRuntimeError::StaleRevision {
            expected: request.expected_revision,
            actual: graph.revision.value(),
        });
    }
    let model = server.effective_model_config();
    let planner = crate::task::LlmTaskPlanner::new(&model, Arc::clone(&server.secret_resolver));
    match planner.review_graph(&graph).await {
        Ok(review) => (
            StatusCode::OK,
            Json(json!({
                "review": review,
                "reviewed_revision": graph.revision.value(),
            })),
        )
            .into_response(),
        Err(crate::task::TaskPlannerError::Llm(message)) => planning_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "review_unavailable",
            message,
        ),
        Err(error) => planning_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_review",
            error.to_string(),
        ),
    }
}
