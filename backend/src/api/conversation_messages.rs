use crate::app::state::AppServer;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;

type Error = (StatusCode, Json<Value>);
#[cfg(test)]
#[path = "conversation_messages/tests.rs"]
mod tests;
fn error(status: StatusCode, message: impl Into<String>) -> Error {
    (status, Json(json!({"error":message.into()})))
}
pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route(
            "/api/conversations/:id/messages/:message_id",
            delete(remove),
        )
        .route(
            "/api/conversations/:id/messages/:message_id/rollback",
            post(rollback),
        )
}
async fn remove(
    State(server): State<Arc<AppServer>>,
    Path((id, message)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    mutate(server, id, message, false).await
}
async fn rollback(
    State(server): State<Arc<AppServer>>,
    Path((id, message)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    mutate(server, id, message, true).await
}

async fn mutate(
    server: Arc<AppServer>,
    id: String,
    message: String,
    rollback: bool,
) -> Result<Json<Value>, Error> {
    let gate = server
        .chat_turn_gates
        .lock()
        .entry(id.clone())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone();
    let _guard = gate
        .try_lock_owned()
        .map_err(|_| error(StatusCode::CONFLICT, "对话正在执行，请先停止后再修改消息"))?;
    if server
        .db
        .find_conversation(&id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "无法读取对话"))?
        .is_none()
    {
        return Err(error(StatusCode::NOT_FOUND, "对话不存在"));
    }
    let resources = if rollback {
        server
            .db
            .message_attachments(&id)
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "无法读取消息附件"))?
            .remove(&message)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let result = server.db.mutate_conversation_message(&id,&message,rollback).map_err(|reason|match reason.as_str() {
        reason if reason.starts_with("history_protocol:") => (StatusCode::CONFLICT,Json(json!({"code":"chat_history_protocol_invalid","error":"保留历史仍有未配对工具调用，请回退到受影响的较早用户消息","affected_user_message_id":reason.strip_prefix("history_protocol:").filter(|id|!id.is_empty())}))),
        "conversation_not_found"|"message_not_found"=>error(StatusCode::NOT_FOUND,"对话或目标消息不存在"),
        "conversation_running"=>error(StatusCode::CONFLICT,"对话正在执行，请先停止后再修改消息"),
        "rollback_requires_user_message"=>error(StatusCode::BAD_REQUEST,"只能回退到用户消息"),
        "message_not_visible"=>error(StatusCode::BAD_REQUEST,"请选择一条可见用户或助手消息"),
        _=>error(StatusCode::INTERNAL_SERVER_ERROR,"消息修改未完成，本地存储不可用"),
    })?;
    // Approval resolution uses the same turn gate before consumption, so old
    // approvals cannot race this mutation and execute against edited history.
    for approval in server.approval_store.list_pending_for_conversation(&id) {
        let _ = server.approval_store.cancel(&approval.approval_id, &id);
    }
    let conversation = server.db.get_conversation(&id).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "消息已修改，但对话读取失败",
        )
    })?;
    let conversation = server
        .db
        .present_conversation(json!(conversation))
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "消息已修改，但附件读取失败",
            )
        })?;
    let mut response = json!({"conversation":conversation,"deleted_message_ids":result.deleted_message_ids,"removed_protocol_count":result.removed_protocol_count});
    if let Some(text) = result.draft_text {
        response["draft"] =
            json!({"text":text,"resources":resources,"resource_scope":"conversation"});
    }
    Ok(Json(response))
}
