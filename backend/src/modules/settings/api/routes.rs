//! Settings HTTP handlers.
//!
//! Thin by design: each handler adapts transport (extractors, response shape)
//! and delegates to the application layer. The secret state machine lives in
//! `application::update`, not here.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use crate::config::types::AppConfig;
use crate::server::AppServer;

use crate::modules::settings::application::{provider_test, service, update};

pub async fn get_handler(State(server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    Json(service::build_redacted_config(&server).await)
}

pub async fn update_handler(
    State(server): State<Arc<AppServer>>,
    Json(incoming): Json<AppConfig>,
) -> Json<serde_json::Value> {
    update::apply(&server, incoming).await
}

pub async fn verify_provider_handler(
    State(server): State<Arc<AppServer>>,
    Path(kind): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    provider_test::verify_provider(&server, &kind).await
}
