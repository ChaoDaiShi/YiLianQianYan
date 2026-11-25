//! Settings HTTP handlers.
//!
//! Thin by design: each handler adapts transport (extractors, response shape)
//! and delegates to the application layer. The secret state machine lives in
//! `application::update`, not here.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::app::state::AppServer;
use crate::config::types::AppConfig;

use crate::modules::settings::application::{provider_test, service, update};

pub async fn get_handler(State(server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    Json(service::build_redacted_config(&server).await)
}

#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    #[serde(flatten)]
    pub(super) config: AppConfig,
    pub(super) expected_revision: Option<u64>,
}

pub async fn update_handler(
    State(server): State<Arc<AppServer>>,
    Json(incoming): Json<UpdateSettingsRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    update::apply(&server, incoming.config, incoming.expected_revision).await
}

pub async fn verify_provider_handler(
    State(server): State<Arc<AppServer>>,
    Path(kind): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    provider_test::verify_provider(&server, &kind).await
}
