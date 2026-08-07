// ============================================================
// Settings API handlers
// ============================================================

use axum::{extract::State, Json};
use std::sync::Arc;

use crate::config::types::AppConfig;
use crate::server::AppServer;

pub async fn get_handler(State(server): State<Arc<AppServer>>) -> Json<AppConfig> {
    Json(server.config.read().clone())
}

pub async fn update_handler(
    State(server): State<Arc<AppServer>>,
    Json(config): Json<AppConfig>,
) -> Json<serde_json::Value> {
    match server.db.save_settings(&config) {
        Ok(_) => {
            *server.config.write() = config;
            Json(serde_json::json!({"status": "saved"}))
        }
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}
