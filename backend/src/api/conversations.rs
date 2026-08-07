// ============================================================
// Conversation API handlers
// ============================================================

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::db::ConversationSummary;
use crate::server::AppServer;

// ── List ──

pub async fn list_handler(State(server): State<Arc<AppServer>>) -> Json<Vec<ConversationSummary>> {
    Json(server.db.list_conversations().unwrap_or_default())
}

// ── Create ──

#[derive(Deserialize)]
pub struct CreateRequest {
    pub title: Option<String>,
}

pub async fn create_handler(
    State(server): State<Arc<AppServer>>,
    Json(req): Json<CreateRequest>,
) -> Json<ConversationSummary> {
    let title = req.title.unwrap_or_else(|| "新对话".to_string());
    Json(
        server
            .db
            .create_conversation(&title)
            .unwrap_or_else(|_| ConversationSummary {
                id: uuid::Uuid::new_v4().to_string(),
                title,
                created_at: chrono::Utc::now().timestamp_millis(),
                updated_at: chrono::Utc::now().timestamp_millis(),
            }),
    )
}

// ── Load ──

pub async fn load_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match server.db.get_conversation(&id) {
        Ok(conv) => Json(serde_json::to_value(conv).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

// ── Delete ──

pub async fn delete_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match server.db.delete_conversation(&id) {
        Ok(_) => Json(serde_json::json!({"status": "deleted"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}
