use super::*;

use crate::modules::workflow::application::graph_service;

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
    let record =
        graph_service::create_workflow_graph(&server, body.name, body.description, body.definition)
            .map_err(graph_service_error)?;
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
    let record = graph_service::update_workflow_graph(
        &server,
        &id,
        body.name,
        body.description,
        body.definition,
    )
    .map_err(graph_service_error)?;
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
