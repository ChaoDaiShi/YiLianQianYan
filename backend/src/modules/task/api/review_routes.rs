use super::*;

use crate::modules::task::application::review_service;

pub async fn review_graph(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
    Json(request): Json<ReviewGraphRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    match review_service::review_task_graph(&server, &graph_id, request.expected_revision).await {
        Ok((review, reviewed_revision)) => (
            StatusCode::OK,
            Json(json!({
                "review": review,
                "reviewed_revision": reviewed_revision,
            })),
        )
            .into_response(),
        Err(error) => review_graph_error(error),
    }
}
