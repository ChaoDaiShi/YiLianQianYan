use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::server::AppServer;
use crate::shared::context::{ContextRequest, TaskProjectionProvider};

#[derive(Deserialize)]
pub struct ProjectionQuery {
    pub scope: Option<String>,
    pub query: Option<String>,
    pub max_items: Option<usize>,
}
pub async fn task_projections_handler(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<ProjectionQuery>,
) -> impl IntoResponse {
    let mut request = ContextRequest::new(query.scope.unwrap_or_else(|| "global".to_string()));
    request.query = query.query;
    if let Some(max_items) = query.max_items {
        request.max_items = max_items;
    }
    Json(json!({ "tasks": server.task_world.list(&request) }))
}
