use super::*;

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
