// ============================================================
// Conversation API handlers
// ============================================================

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::app::state::AppServer;
use crate::db::ConversationSummary;

#[path = "conversation_messages.rs"]
pub mod messages;

pub type StorageApiError = (StatusCode, Json<serde_json::Value>);
#[derive(Deserialize)]
pub struct WorkspaceChoice {
    pub workspace_id: Option<String>,
}
pub async fn workspace_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    server.db.get_conversation(&id).map_err(storage_error)?;
    Ok(Json(
        serde_json::json!({"workspace_id":server.db.conversation_workspace(&id).map_err(storage_error)?}),
    ))
}
pub async fn workspace_update_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<WorkspaceChoice>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    server
        .db
        .set_conversation_workspace(&id, body.workspace_id.as_deref())
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":e})),
            )
        })?;
    Ok(Json(serde_json::json!({"workspace_id":body.workspace_id})))
}

pub async fn turns_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    let turns = server.db.conversation_turns(&id).map_err(storage_error)?;
    Ok(Json(serde_json::json!({"turns":turns})))
}

fn storage_error(_: String) -> StorageApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "code": "local_storage_unavailable",
            "error": "本地数据暂时不可用，请重试"
        })),
    )
}

fn not_found_error(entity: &str) -> StorageApiError {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "code": "not_found",
            "error": format!("{entity}不存在")
        })),
    )
}

// ── List ──

pub async fn list_handler(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<Vec<serde_json::Value>>, StorageApiError> {
    let origins = server.db.conversation_origins().map_err(storage_error)?;
    let rows = server.db.list_conversations().map_err(storage_error)?;
    let rows = rows
        .into_iter()
        .map(|row| {
            let origin = origins
                .get(&row.id)
                .cloned()
                .ok_or_else(|| storage_error("会话来源不存在".into()))?;
            let mut value = serde_json::to_value(row).map_err(|e| storage_error(e.to_string()))?;
            value["origin"] = origin;
            Ok(value)
        })
        .collect::<Result<Vec<_>, StorageApiError>>()?;
    Ok(Json(rows))
}

// ── Create ──

#[derive(Deserialize)]
pub struct CreateRequest {
    pub title: Option<String>,
}
#[derive(Deserialize)]
pub struct RenameRequest {
    pub title: String,
}
pub async fn rename_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(req): Json<RenameRequest>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    if req.title.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"对话标题不能为空"})),
        ));
    }
    if server
        .db
        .find_conversation(&id)
        .map_err(storage_error)?
        .is_none()
    {
        return Err(not_found_error("对话"));
    }
    server
        .db
        .update_conversation_title(&id, req.title.trim())
        .map_err(storage_error)?;
    Ok(Json(serde_json::json!({"id":id,"title":req.title.trim()})))
}

pub async fn create_handler(
    State(server): State<Arc<AppServer>>,
    Json(req): Json<CreateRequest>,
) -> Result<Json<ConversationSummary>, StorageApiError> {
    let title = req.title.unwrap_or_else(|| "新对话".to_string());
    server
        .db
        .create_conversation(&title)
        .map(Json)
        .map_err(storage_error)
}

// ── Load ──

pub async fn load_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    match server.db.find_conversation(&id) {
        Ok(Some(conv)) => {
            let value = serde_json::to_value(conv).map_err(|e| storage_error(e.to_string()))?;
            server
                .db
                .present_conversation(value)
                .map(Json)
                .map_err(storage_error)
        }
        Ok(None) => Err(not_found_error("对话")),
        Err(e) => Err(storage_error(e)),
    }
}

// ── Delete ──

pub async fn delete_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    server
        .db
        .delete_conversation(&id)
        .map(|_| Json(serde_json::json!({"status": "deleted"})))
        .map_err(storage_error)
}
