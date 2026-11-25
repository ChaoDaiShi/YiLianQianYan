//! Turn settlement, restart recovery and explicit stop for personal delegations.
use super::{active, attempts, mutate, owner, Automation, AutomationTarget, Occurrence};
use crate::{app::state::AppServer, db::ConversationRunStatus};
use axum::{extract::State, Json};
use std::sync::Arc;

/// Called after the ordinary chat history and run status have committed,
/// including approval and answer continuations.
pub fn finish_turn(
    server: &AppServer,
    conversation: &str,
    status: ConversationRunStatus,
    error: Option<&str>,
) -> Result<(), String> {
    let Some(id) = owner(server, conversation)? else {
        return Ok(());
    };
    let turns = server.db.conversation_turns(conversation)?;
    let turn_id = turns.first().map(|turn| turn.user_message_id.as_str());
    let items = attempts(server, &id)?;
    // A late completion from a stopped turn must never settle a newly queued
    // occurrence for the same persistent conversation.
    let item = items
        .iter()
        .find(|item| {
            item.assistant_turn_id
                .as_deref()
                .is_some_and(|id| Some(id) == turn_id)
        })
        .or_else(|| {
            items
                .iter()
                .find(|item| active(&item.status) && item.assistant_turn_id.is_none())
        });
    let Some(item) = item.filter(|item| active(&item.status)) else {
        return Ok(());
    };
    let state = server.db.get_conversation(conversation)?;
    let current_messages = turn_id
        .and_then(|id| state.messages.iter().position(|m| m.id == id))
        .map(|start| &state.messages[start + 1..])
        .unwrap_or(&[]);
    let current_calls = current_messages
        .iter()
        .filter_map(|m| m.tool_calls.as_deref())
        .filter_map(|calls| serde_json::from_str::<Vec<serde_json::Value>>(calls).ok())
        .flatten()
        .filter_map(|call| call["id"].as_str().map(str::to_owned))
        .collect::<std::collections::HashSet<_>>();
    let uncertain = status == ConversationRunStatus::Completed
        && (item.error.is_some()
            || state.execution_history.iter().any(|record| {
                current_calls.contains(&record.tool_call_id)
                    && (matches!(record.execution_status.as_str(), "failed" | "error")
                        || record.verification_status == "failed")
            }));
    let report = if status == ConversationRunStatus::Completed {
        current_messages
            .iter()
            .rev()
            .find(|m| {
                m.role == "assistant"
                    && !m.content.trim().is_empty()
                    && m.tool_calls
                        .as_deref()
                        .is_none_or(|calls| calls == "[]" || calls == "null")
            })
            .map(|m| crate::utils::text::truncate_chars(&m.content, 4000))
    } else {
        None
    };
    mutate(server, &item.id, |item| {
        if !active(&item.status) {
            return;
        }
        item.assistant_turn_id = turn_id.map(str::to_owned);
        item.status = if uncertain {
            "needs_review"
        } else {
            status.as_str()
        }
        .into();
        item.error = if uncertain {
            Some("本轮有工具失败或核验未通过，请核实执行记录后再显式继续".into())
        } else {
            error.map(str::to_owned).or_else(|| item.error.clone())
        };
        item.report = report;
    })?;
    Ok(())
}

pub(in crate::modules::automation) fn recover(
    server: &AppServer,
    item: &mut Occurrence,
) -> Result<bool, String> {
    // Personal occurrences have a conversation and no workflow/robot run.
    if item.conversation_id.is_none() || item.run_id.is_some() || item.robot_run_id.is_some() {
        return Ok(false);
    }
    if active(&item.status) {
        let conversation = item.conversation_id.as_deref().unwrap_or_default();
        if !server
            .db
            .pending_conversation_inputs(conversation)?
            .is_empty()
        {
            // A persisted question requires a fresh user answer; retaining it
            // does not replay a tool or call the model on startup.
            item.status = "waiting_input".into();
            return server.db.update_automation_occurrence(item).map(|_| true);
        }
        item.status = "interrupted".into();
        item.error =
            Some("应用退出时本轮尚未结束；请先核实文件与执行记录，再手动继续，不自动重放".into());
        server.db.update_automation_occurrence(item)?;
    }
    Ok(true)
}

fn cancel_stored_approvals(server: &AppServer, conversation: &str) -> Result<(), String> {
    let state = server.db.get_conversation(conversation)?;
    for record in state
        .execution_history
        .iter()
        .filter(|record| record.approval_status == "pending")
    {
        let has_call = state
            .messages
            .iter()
            .filter_map(|message| message.tool_calls.as_deref())
            .filter_map(|calls| serde_json::from_str::<Vec<serde_json::Value>>(calls).ok())
            .flatten()
            .any(|call| call["id"].as_str() == Some(record.tool_call_id.as_str()));
        let has_result = state.messages.iter().any(|message| {
            message.role == "tool"
                && message.tool_call_id.as_deref() == Some(record.tool_call_id.as_str())
        });
        if has_call && !has_result {
            server.db.persist_chat_message(
                conversation,
                &crate::integrations::llm::types::ChatMessage {
                    role: "tool".into(),
                    content: Some("用户已停止委托；此待审批操作未执行。".into()),
                    tool_calls: None,
                    tool_call_id: Some(record.tool_call_id.clone()),
                    name: Some(record.name.clone()),
                    reasoning_content: None,
                },
                chrono::Utc::now().timestamp_millis(),
            )?;
        }
        let mut event = crate::agent::engine::AgentEvent::error_event(conversation, String::new());
        event.event_type = "approval_resolved".into();
        event.error = None;
        event.tool_call_id = Some(record.tool_call_id.clone());
        event.tool_name = Some(record.name.clone());
        event.approval_id = record.approval_id.clone();
        event.status = Some("cancelled".into());
        server.db.apply_execution_event(&event)?;
    }
    Ok(())
}

/// Called only after an explicit stop has cancelled and joined the turn. A
/// missing result is uncertainty, never proof that an operation did not run.
fn close_interrupted_tools(server: &AppServer, conversation: &str) -> Result<(), String> {
    let state = server.db.get_conversation(conversation)?;
    let finished = state
        .messages
        .iter()
        .filter(|m| m.role == "tool")
        .filter_map(|m| m.tool_call_id.as_deref())
        .collect::<std::collections::HashSet<_>>();
    let mut repaired = std::collections::HashSet::new();
    let uncertain = "用户已停止此轮。该工具调用在中断时缺少完整结果，实际结果不确定；没有自动重试，请人工核实文件与执行记录。";
    for call in state
        .messages
        .iter()
        .filter_map(|m| m.tool_calls.as_deref())
        .filter_map(|raw| serde_json::from_str::<Vec<serde_json::Value>>(raw).ok())
        .flatten()
    {
        let Some(id) = call["id"].as_str() else {
            continue;
        };
        if finished.contains(id) || !repaired.insert(id.to_owned()) {
            continue;
        }
        let name = call["function"]["name"].as_str().unwrap_or("unknown");
        server.db.persist_chat_message(
            conversation,
            &crate::integrations::llm::types::ChatMessage {
                role: "tool".into(),
                content: Some(uncertain.into()),
                tool_calls: None,
                tool_call_id: Some(id.into()),
                name: Some(name.into()),
                reasoning_content: None,
            },
            chrono::Utc::now().timestamp_millis(),
        )?;
        let mut event = crate::agent::engine::AgentEvent::error_event(conversation, String::new());
        event.event_type = "tool_end".into();
        event.error = None;
        event.tool_call_id = Some(id.into());
        event.tool_name = Some(name.into());
        event.status = Some("error".into());
        event.result = Some(uncertain.into());
        server.db.apply_execution_event(&event)?;
        event.event_type = "verification".into();
        event.verification_success = Some(false);
        event.verification_reason = Some(uncertain.into());
        server.db.apply_execution_event(&event)?;
    }
    // A hard process exit can persist a start event before its assistant message.
    // Settle those audit records too; no synthetic tool invocation is performed.
    for record in state.execution_history.iter().filter(|r| {
        matches!(
            r.execution_status.as_str(),
            "running" | "queued" | "awaiting_approval"
        )
    }) {
        if repaired.contains(&record.tool_call_id) {
            continue;
        }
        let mut event = crate::agent::engine::AgentEvent::error_event(conversation, String::new());
        event.event_type = "tool_end".into();
        event.error = None;
        event.tool_call_id = Some(record.tool_call_id.clone());
        event.tool_name = Some(record.name.clone());
        event.status = Some("error".into());
        event.result = Some(uncertain.into());
        server.db.apply_execution_event(&event)?;
        event.event_type = "verification".into();
        event.verification_success = Some(false);
        event.verification_reason = Some(uncertain.into());
        server.db.apply_execution_event(&event)?;
    }
    if server
        .db
        .conversation_protocol_issue(conversation)?
        .is_some()
    {
        return Err(
            "委托已停止，但历史仍有无法安全修复的协议问题；请保留记录并新建委托，不会自动重试"
                .into(),
        );
    }
    Ok(())
}

pub async fn stop(server: &Arc<AppServer>, id: &str) -> Result<Automation, String> {
    let _dispatch_guard =
        crate::api::chat::lifecycle::acquire_turn(server, &format!("personal-delegation:{id}"))
            .await;
    let automation = server.db.get_automation(id)?.ok_or("个人委托不存在")?;
    let Some(AutomationTarget::PersonalAssistant {
        conversation_id, ..
    }) = &automation.target
    else {
        return Err("此停止入口仅用于个人委托".into());
    };
    let paused = super::super::set_enabled(server, id, false, automation.revision)?;
    for item in attempts(server, id)?
        .into_iter()
        .filter(|v| active(&v.status))
    {
        mutate(server, &item.id, |item| {
            item.status = "cancelled".into();
            item.error = Some("用户已停止委托；已完成的文件操作不会回滚，请核实后再继续".into());
        })?;
    }
    if let Some(cancel) = server.active_tasks.lock().get(conversation_id) {
        cancel.cancel();
    }
    let turn_guard = crate::api::chat::lifecycle::acquire_turn(server, conversation_id).await;
    drop(turn_guard);
    for approval in server
        .approval_store
        .list_pending_for_conversation(conversation_id)
    {
        let Json(result) = crate::api::approvals::cancel_handler(
            State(server.clone()),
            axum::extract::Path(approval.approval_id),
            Json(crate::api::approvals::ApprovalDecisionRequest {
                conversation_id: Some(conversation_id.clone()),
                attestation_id: None,
            }),
        )
        .await;
        if result["ok"] != true {
            return Err(format!("停止后的审批清理失败：{result}"));
        }
    }
    server.db.cancel_conversation_inputs(conversation_id)?;
    cancel_stored_approvals(server, conversation_id)?;
    if !server.active_tasks.lock().contains_key(conversation_id) {
        server.db.set_conversation_run_status(
            conversation_id,
            ConversationRunStatus::Cancelled,
            None,
        )?;
    }
    close_interrupted_tools(server, conversation_id)?;
    Ok(paused)
}
