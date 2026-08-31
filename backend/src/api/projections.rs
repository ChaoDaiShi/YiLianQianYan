use axum::{extract::Query, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;

use crate::shared::context::{
    ContextRequest, DesktopContextProvider, MockDesktopContextProvider, MockTaskProjectionProvider,
    TaskProjectionProvider,
};

#[derive(Deserialize)]
pub struct ProjectionQuery {
    scope: Option<String>,
}

pub async fn task_projections_handler(Query(query): Query<ProjectionQuery>) -> impl IntoResponse {
    let request = ContextRequest::new(query.scope.unwrap_or_else(|| "global".to_string()));
    let provider = MockTaskProjectionProvider;
    Json(json!({ "tasks": provider.list(&request) }))
}

pub async fn desktop_context_handler(Query(query): Query<ProjectionQuery>) -> impl IntoResponse {
    let request = ContextRequest::new(query.scope.unwrap_or_else(|| "global".to_string()));
    let provider = MockDesktopContextProvider;
    Json(provider.provide(&request))
}
