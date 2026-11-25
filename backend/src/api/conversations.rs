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

pub mod goal;
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
    let workspace_id = match body.workspace_id {
        Some(id) => Some(id),
        None => server.db.default_workspace_id().map_err(storage_error)?,
    };
    server
        .db
        .set_conversation_workspace(&id, workspace_id.as_deref())
        .map_err(|e| {
            if e == "conversation_goal_workspace_conflict" {
                (StatusCode::CONFLICT, Json(serde_json::json!({"code":e,"error":"此对话已关联任务，请在该任务的工作空间中继续"})))
            } else {
                (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":e})))
            }
        })?;
    Ok(Json(serde_json::json!({"workspace_id":workspace_id})))
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
    #[serde(default)]
    pub use_default_workspace: Option<bool>,
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
        .create_conversation_with_workspace_default(
            &title,
            req.use_default_workspace.unwrap_or(true),
        )
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
    let bound = server.db.list_automations().map_err(storage_error)?.into_iter().any(|item| {
        matches!(item.target, Some(crate::modules::automation::model::AutomationTarget::PersonalAssistant { conversation_id, .. }) if conversation_id == id)
    });
    if bound {
        return Err((
            StatusCode::CONFLICT,
            Json(
                serde_json::json!({"error":"此对话仍关联个人委托，包含执行与文件来源；请在时间任务中归档委托并保留记录。需彻底删除时先明确删除委托，再删除对话。"}),
            ),
        ));
    }
    let active_error = || {
        (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "code": "conversation_turn_active",
                "error": "请先停止或完成当前对话轮次后再删除对话"
            })),
        )
    };
    let _turn =
        crate::api::chat::lifecycle::try_acquire_turn(&server, &id).map_err(|_| active_error())?;
    let _lanes_guard =
        crate::api::chat::lifecycle::acquire_turn(&server, &format!("lanes:{id}")).await;
    let lanes = server
        .db
        .conversation_lanes(&id)
        .map_err(|error| storage_error(error.to_string()))?;
    let _lane_turns = lanes
        .iter()
        .map(|lane| {
            crate::api::chat::lifecycle::try_acquire_turn(&server, &lane.id)
                .map_err(|_| active_error())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if lanes.iter().any(|lane| {
        matches!(
            lane.run_status.as_str(),
            "running" | "waiting_approval" | "waiting_input"
        )
    }) {
        return Err(active_error());
    }
    if server.active_tasks.lock().contains_key(&id)
        || matches!(
            server
                .db
                .find_conversation(&id)
                .map_err(storage_error)?
                .map(|conversation| conversation.run_status),
            Some(
                crate::db::ConversationRunStatus::Running
                    | crate::db::ConversationRunStatus::WaitingApproval
                    | crate::db::ConversationRunStatus::WaitingInput
            )
        )
    {
        return Err(active_error());
    }
    server
        .db
        .delete_conversation_family(&id, lanes.iter().map(|lane| lane.id.as_str()))
        .map(|_| Json(serde_json::json!({"status": "deleted"})))
        .map_err(storage_error)
}
