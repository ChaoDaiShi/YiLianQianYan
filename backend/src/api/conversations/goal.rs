//! Read and establish a conversation's existing ProofLoop goal identity.
use super::{not_found_error, storage_error, StorageApiError};
use crate::{
    app::state::AppServer,
    db::ConversationGoalBinding,
    modules::task::TaskGraphId,
    workspace::{WorkspaceId, WorkspaceStatus},
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

fn conflict(code: &str, message: &str) -> StorageApiError {
    (
        StatusCode::CONFLICT,
        Json(json!({"code":code,"error":message})),
    )
}

#[derive(Deserialize)]
pub struct BindGoalRequest {
    graph_id: String,
}

pub async fn get_handler(
    State(server): State<Arc<AppServer>>,
    Path(conversation_id): Path<String>,
) -> Result<Json<Value>, StorageApiError> {
    if server
        .db
        .find_conversation(&conversation_id)
        .map_err(storage_error)?
        .is_none()
    {
        return Err(not_found_error("对话"));
    }
    if crate::modules::automation::personal::is_delegated(&server, &conversation_id)
        .map_err(storage_error)?
    {
        return Ok(Json(json!({"binding":null,"restricted":true})));
    }
    let Some(binding) = server
        .db
        .load_conversation_goal_binding(&conversation_id)
        .map_err(storage_error)?
    else {
        return Ok(Json(json!({"binding":null})));
    };
    let draft = server
        .db
        .confirmed_proof_draft_for_goal(&binding.goal_id, binding.goal_version)
        .map_err(|error| storage_error(error.to_string()))?
        .ok_or_else(|| conflict("proofloop_goal_not_confirmed", "对话绑定的目标记录不可用"))?;
    Ok(Json(json!({"binding":{
        "goal_id": binding.goal_id,
        "goal_version": binding.goal_version,
        "graph_id": draft.graph_id,
        "workspace_id": draft.contract.workspace_id,
    }})))
}

pub async fn put_handler(
    State(server): State<Arc<AppServer>>,
    Path(conversation_id): Path<String>,
    Json(request): Json<BindGoalRequest>,
) -> Result<Json<Value>, StorageApiError> {
    if crate::modules::automation::personal::is_delegated(&server, &conversation_id)
        .map_err(storage_error)?
    {
        return Err(conflict(
            "personal_delegation_scope",
            "个人委托已有独立操作范围和预算，不能关联其他目标；请从委托详情管理",
        ));
    }
    let _turn =
        crate::api::chat::lifecycle::try_acquire_turn(&server, &conversation_id).map_err(|_| {
            conflict(
                "conversation_turn_active",
                "请等待当前对话轮次结束后再关联任务",
            )
        })?;
    let conversation = server
        .db
        .find_conversation(&conversation_id)
        .map_err(storage_error)?
        .ok_or_else(|| not_found_error("对话"))?;
    if matches!(
        conversation.run_status,
        crate::db::ConversationRunStatus::Running
            | crate::db::ConversationRunStatus::WaitingApproval
            | crate::db::ConversationRunStatus::WaitingInput
    ) {
        return Err(conflict(
            "conversation_turn_active",
            "请先完成或取消当前对话轮次后再关联任务",
        ));
    }
    let graph_id = TaskGraphId::new(&request.graph_id).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"code":"invalid_graph_id","error":error.to_string()})),
        )
    })?;
    if server.task_world.get_graph(&graph_id).is_none() {
        return Err(not_found_error("任务图"));
    }
    let draft = server
        .db
        .confirmed_proof_draft_for_graph(&request.graph_id)
        .map_err(|error| storage_error(error.to_string()))?
        .ok_or_else(|| conflict("proofloop_goal_not_confirmed", "任务图尚未确认目标"))?;
    let workspace_id = WorkspaceId::new(&draft.contract.workspace_id)
        .map_err(|error| storage_error(error.to_string()))?;
    let workspace = server
        .db
        .get_workspace(&workspace_id)
        .map_err(storage_error)?
        .ok_or_else(|| not_found_error("工作空间"))?;
    if workspace.status != WorkspaceStatus::Active {
        return Err(conflict("workspace_archived", "工作空间已归档"));
    }
    let binding = ConversationGoalBinding {
        conversation_id: conversation_id.clone(),
        goal_id: draft.contract.goal_id,
        goal_version: draft.contract.version,
    };
    let bound = server
        .db
        .bind_conversation_goal_with_workspace(&binding, Some(workspace_id.as_str()))
        .map_err(|error| {
            if error == "conversation_goal_workspace_conflict" {
                conflict(
                    "conversation_goal_workspace_conflict",
                    "对话与目标属于不同工作空间，请在目标工作空间中新建对话",
                )
            } else {
                storage_error(error)
            }
        })?;
    if !bound {
        return Err(conflict(
            "proofloop_goal_rebind_rejected",
            "此对话已绑定到其他目标版本，不能重新绑定",
        ));
    }
    get_handler(State(server), Path(conversation_id)).await
}
