use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    app::state::AppServer,
    db::ConversationRunStatus,
    interaction::{text_task_command, TaskNarrator},
    modules::task::{TaskCommandService, TaskExecutionControlState, TaskGraphId},
};

#[derive(Deserialize)]
pub struct Input {
    conversation_id: String,
    message: String,
}

#[derive(Serialize)]
pub struct Reply {
    message: Option<String>,
}

pub async fn handle(
    State(server): State<Arc<AppServer>>,
    Json(input): Json<Input>,
) -> Result<Json<Reply>, super::ChatApiError> {
    let Some(command) = text_task_command(&input.message) else {
        return Ok(Json(Reply { message: None }));
    };
    let id = input.conversation_id.as_str();
    let db = &server.db;
    let error = |reason: String| {
        super::chat_error(StatusCode::CONFLICT, "chat_task_command_failed", reason)
    };
    let run_status = db
        .conversation_run_status(id)
        .map_err(&error)?
        .ok_or_else(|| {
            super::chat_error(
                StatusCode::NOT_FOUND,
                "conversation_not_found",
                "会话已删除或不存在",
            )
        })?;
    if !super::runtime::ordinary_conversation(&server, id).map_err(&error)? {
        return Ok(Json(Reply { message: None }));
    }
    let status = matches!(command.as_str(), "task.status" | "task.current");
    if command == "task.resume"
        && db.conversation_mode(id).map_err(&error)?
            == crate::db::conversation_modes::WorkspaceMode::Chat
    {
        return Ok(Json(Reply {
            message: Some("当前是聊天模式。请切换到自动或工作模式，再发送任务指令。".into()),
        }));
    }
    let graph = db
        .load_conversation_goal_binding(id)
        .map_err(&error)?
        .map(|binding| db.confirmed_proof_draft_for_goal(&binding.goal_id, binding.goal_version))
        .transpose()
        .map_err(|failure| error(failure.to_string()))?
        .flatten();
    if let Some(draft) = graph {
        let graph = TaskGraphId::new(draft.graph_id.clone())
            .map_err(|failure| error(failure.to_string()))?;
        let service = TaskCommandService::new(&server.task_world);
        let now = chrono::Utc::now().timestamp_millis();
        if !status
            && (!db
                .confirmed_proof_draft_for_graph(&draft.graph_id)
                .map_err(|failure| error(failure.to_string()))?
                .is_some_and(|current| {
                    current.contract.goal_id == draft.contract.goal_id
                        && current.contract.version == draft.contract.version
                })
                || !server.task_world.get_graph(&graph).is_some_and(|current| {
                    Some(current.revision.value()) == draft.confirmed_graph_revision
                }))
        {
            return Err(error(
                "会话绑定的目标版本已变化，请在当前目标中操作。".into(),
            ));
        }
        let projection = match command.as_str() {
            "task.pause" => service.pause(&graph, now),
            "task.resume" => {
                if service
                    .status(&graph, now)
                    .map_err(|failure| error(failure.to_string()))?
                    .control_state
                    != TaskExecutionControlState::Paused
                {
                    return Ok(Json(Reply { message: None }));
                }
                service.resume(&graph, now)
            }
            "task.cancel" => service.cancel(&graph, now),
            "task.status" | "task.current" => service.status(&graph, now),
            _ => return Ok(Json(Reply { message: None })),
        }
        .map_err(|failure| error(failure.to_string()))?;
        return Ok(Json(Reply {
            message: Some(TaskNarrator::narrate(&projection).text),
        }));
    }

    let running = server.active_tasks.lock().contains_key(id);
    let message = match command.as_str() {
        "task.status" | "task.current" => Some(
            match run_status {
                ConversationRunStatus::WaitingApproval => "当前对话正在等待操作审批。",
                ConversationRunStatus::WaitingInput => "当前对话正在等待你的补充。",
                _ if running => "当前对话仍在处理本轮请求。",
                ConversationRunStatus::Running => "本轮仍在结算执行状态。",
                ConversationRunStatus::Completed => "上一轮已结束。请查看对话中的回复和执行记录。",
                ConversationRunStatus::Failed => "上一轮遇到错误。请查看对话中的执行记录。",
                ConversationRunStatus::Cancelled => "上一轮已取消。",
                ConversationRunStatus::Interrupted => "上一轮已中断。",
                ConversationRunStatus::Idle => "当前对话没有运行中的任务。",
            }
            .into(),
        ),
        "task.cancel" => Some(
            if super::super::chat_stop::stop(server.clone(), id)
                .await
                .map_err(error)?
            {
                "已向当前轮次发出取消请求。".into()
            } else {
                "当前没有可取消的轮次。".into()
            },
        ),
        "task.pause" => {
            Some("当前是对话轮次。可发送「取消当前任务」停止，绑定的目标支持暂停和继续。".into())
        }
        "task.resume" if running => {
            Some("当前轮次仍在处理。可发送「补充给当前任务：…」调整后续步骤。".into())
        }
        "task.resume" if run_status == ConversationRunStatus::WaitingInput => {
            Some("当前轮次等待回答。请用问题卡片，或发送「补充回答：…」。".into())
        }
        "task.resume" if run_status == ConversationRunStatus::WaitingApproval => {
            Some("当前轮次等待审批，请先处理审批。".into())
        }
        _ => None,
    };
    Ok(Json(Reply { message }))
}
