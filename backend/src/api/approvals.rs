// ============================================================
// Approval API — approve / reject / cancel pending tool approvals
//
// Approve / reject return an SSE stream that resumes the paused
// agent: the decision is applied, the original tool call is
// executed (approve) exactly once, the tool result is written back
// to the agent context, and the agent loop continues.
// ============================================================

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::agent::engine::{self, AgentEvent, RunOutcome};
use crate::agent::state::AgentState;
use crate::agent::verifier::Verifier;
use crate::config::types::AppConfig;
use crate::db::{Database, MessageRow};
use crate::llm::client::LlmClient;
use crate::llm::types::ChatMessage;
use crate::safety::approval::{ApprovalError, PendingApproval};
use crate::safety::{PermissionDecision, PermissionManager};
use crate::server::{AppServer, LogBuffer};
use crate::tools::registry::ToolRegistry;
use crate::tools::trait_def::RiskLevel;

#[derive(Debug, Deserialize)]
pub struct ApprovalDecisionRequest {
    #[serde(default)]
    pub conversation_id: Option<String>,
}

fn status_for(e: &ApprovalError) -> StatusCode {
    match e {
        ApprovalError::NotFound => StatusCode::NOT_FOUND,
        ApprovalError::AlreadyProcessed
        | ApprovalError::Expired
        | ApprovalError::Cancelled
        | ApprovalError::ConversationMismatch => StatusCode::CONFLICT,
    }
}

/// Resolve the approval and atomically mark it Approved or Rejected.
/// The conversation hint, when provided, must match the approval's.
fn resolve_and_consume(
    server: &AppServer,
    approval_id: &str,
    conv_hint: Option<&str>,
    approve: bool,
) -> Result<PendingApproval, (StatusCode, String)> {
    let lookup = server
        .approval_store
        .get(approval_id)
        .ok_or((StatusCode::NOT_FOUND, "审批不存在".to_string()))?;
    let conv_id = conv_hint
        .map(|s| s.to_string())
        .unwrap_or_else(|| lookup.conversation_id.clone());
    let result = if approve {
        server
            .approval_store
            .consume_for_approval(approval_id, &conv_id)
    } else {
        server
            .approval_store
            .consume_for_rejection(approval_id, &conv_id)
    };
    result.map_err(|e| (status_for(&e), e.to_string()))
}

/// Fail-closed sanity re-check before executing an approved tool.
fn check_safety_sane(
    server: &AppServer,
    approval: &PendingApproval,
) -> Result<(), (StatusCode, String)> {
    let tool = server.tool_registry.get(&approval.tool_name).ok_or((
        StatusCode::CONFLICT,
        format!("工具 {} 不存在，已阻断", approval.tool_name),
    ))?;
    let decision =
        PermissionManager::evaluate(&approval.tool_name, tool.risk_level(), &approval.arguments);
    let current_risk = match decision {
        PermissionDecision::Allow => RiskLevel::Low,
        PermissionDecision::RequireApproval { risk_level, .. } => risk_level,
        PermissionDecision::Deny { .. } => {
            return Err((StatusCode::CONFLICT, "审批已被安全策略拒绝".to_string()))
        }
    };
    if current_risk > approval.risk_level {
        return Err((
            StatusCode::CONFLICT,
            "风险等级已升级，需要重新审批".to_string(),
        ));
    }
    Ok(())
}

/// POST /api/approvals/:id/approve — SSE stream resuming the agent.
pub async fn approve_handler(
    State(server): State<Arc<AppServer>>,
    Path(approval_id): Path<String>,
    Json(body): Json<ApprovalDecisionRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let approval =
        resolve_and_consume(&server, &approval_id, body.conversation_id.as_deref(), true)?;
    check_safety_sane(&server, &approval)?;
    Ok(resume_stream(server, approval, true))
}

/// POST /api/approvals/:id/reject — SSE stream resuming the agent.
pub async fn reject_handler(
    State(server): State<Arc<AppServer>>,
    Path(approval_id): Path<String>,
    Json(body): Json<ApprovalDecisionRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let approval = resolve_and_consume(
        &server,
        &approval_id,
        body.conversation_id.as_deref(),
        false,
    )?;
    Ok(resume_stream(server, approval, false))
}

/// POST /api/approvals/:id/cancel — cancel a pending approval (no resume).
pub async fn cancel_handler(
    State(server): State<Arc<AppServer>>,
    Path(approval_id): Path<String>,
    Json(body): Json<ApprovalDecisionRequest>,
) -> Json<serde_json::Value> {
    let lookup = match server.approval_store.get(&approval_id) {
        Some(a) => a,
        None => {
            return Json(serde_json::json!({"ok": false, "error": "审批不存在"}));
        }
    };
    let conv_id = body
        .conversation_id
        .clone()
        .unwrap_or_else(|| lookup.conversation_id.clone());
    match server.approval_store.cancel(&approval_id, &conv_id) {
        Ok(a) => {
            // Keep the conversation chain valid: record that the operation never ran.
            let db = server.db.clone_connection();
            let now = chrono::Utc::now().timestamp_millis();
            let _ = db.add_message(&MessageRow {
                id: uuid::Uuid::new_v4().to_string(),
                conversation_id: a.conversation_id.clone(),
                role: "tool".to_string(),
                content: "任务已取消，操作未执行。".to_string(),
                tool_calls: None,
                tool_call_id: Some(a.tool_call_id.clone()),
                tool_name: Some(a.tool_name.clone()),
                tool_result: None,
                created_at: now,
            });
            Json(serde_json::json!({
                "ok": true,
                "approval_id": a.approval_id,
                "status": "cancelled"
            }))
        }
        Err(e) => Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    }
}

/// GET /api/approvals/:id — fetch a single approval.
pub async fn get_handler(
    State(server): State<Arc<AppServer>>,
    Path(approval_id): Path<String>,
) -> Json<serde_json::Value> {
    match server.approval_store.get(&approval_id) {
        Some(a) => Json(serde_json::json!({"ok": true, "approval": a})),
        None => Json(serde_json::json!({"ok": false, "error": "审批不存在"})),
    }
}

/// GET /api/approvals/pending — list all pending approvals.
pub async fn pending_handler(State(server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    server.approval_store.purge_expired();
    let list = server.approval_store.list_pending();
    Json(serde_json::json!({"ok": true, "approvals": list}))
}

/// Build the resume SSE stream: apply the decision, then re-run the agent.
fn resume_stream(
    server: Arc<AppServer>,
    approval: PendingApproval,
    approve: bool,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(256);

    tokio::spawn(async move {
        let db = server.db.clone_connection();
        let tool_registry = server.tool_registry.clone();
        let config = server.config.read().clone();
        let log_buffer = server.log_buffer.clone();
        let system_prompt = server.build_system_prompt();

        // 1. Notify the frontend the decision was applied.
        let status = if approve { "approved" } else { "rejected" };
        let _ = tx
            .send(AgentEvent {
                event_type: "approval_resolved".into(),
                conversation_id: approval.conversation_id.clone(),
                token: None,
                tool_call_id: Some(approval.tool_call_id.clone()),
                tool_name: Some(approval.tool_name.clone()),
                args: None,
                result: None,
                status: Some(status.to_string()),
                error: None,
                message_id: None,
                risk_level: Some(approval.risk_level.to_string()),
                reason: None,
                approval_id: Some(approval.approval_id.clone()),
                verification_success: None,
                verification_reason: None,
                should_replan: None,
            })
            .await;

        // 2. Approve: execute the ORIGINAL tool call, exactly once.
        if approve {
            let _ = tx
                .send(AgentEvent {
                    event_type: "tool_start".into(),
                    conversation_id: approval.conversation_id.clone(),
                    token: None,
                    tool_call_id: Some(approval.tool_call_id.clone()),
                    tool_name: Some(approval.tool_name.clone()),
                    args: Some(approval.arguments.clone()),
                    result: None,
                    status: None,
                    error: None,
                    message_id: None,
                    risk_level: Some(approval.risk_level.to_string()),
                    reason: None,
                    approval_id: Some(approval.approval_id.clone()),
                    verification_success: None,
                    verification_reason: None,
                    should_replan: None,
                })
                .await;

            log_buffer.push(
                "tool",
                "tool",
                &format!("▶ {}（审批后执行）", approval.tool_name),
            );

            let tool_result = match tool_registry
                .execute(&approval.tool_name, approval.arguments.clone())
                .await
            {
                Some(r) => r,
                None => crate::tools::trait_def::ToolResult::error(format!(
                    "未知工具: {}",
                    approval.tool_name
                )),
            };

            let _ = tx
                .send(AgentEvent {
                    event_type: "tool_end".into(),
                    conversation_id: approval.conversation_id.clone(),
                    token: None,
                    tool_call_id: Some(approval.tool_call_id.clone()),
                    tool_name: Some(approval.tool_name.clone()),
                    args: None,
                    result: Some(tool_result.content.clone()),
                    status: Some((if tool_result.ok { "success" } else { "error" }).to_string()),
                    error: None,
                    message_id: None,
                    risk_level: None,
                    reason: None,
                    approval_id: Some(approval.approval_id.clone()),
                    verification_success: None,
                    verification_reason: None,
                    should_replan: None,
                })
                .await;

            // ── Verify the executed tool's real outcome (approved path) ──
            let verifier = crate::agent::verifier::DefaultVerifier::new(&server.workspace_root);
            let verification = verifier
                .verify(&approval.tool_name, &approval.arguments, &tool_result)
                .await;

            let _ = tx
                .send(AgentEvent {
                    event_type: "verification".into(),
                    conversation_id: approval.conversation_id.clone(),
                    token: None,
                    tool_call_id: Some(approval.tool_call_id.clone()),
                    tool_name: Some(approval.tool_name.clone()),
                    args: None,
                    result: None,
                    status: None,
                    error: None,
                    message_id: None,
                    risk_level: None,
                    reason: None,
                    approval_id: Some(approval.approval_id.clone()),
                    verification_success: Some(verification.success),
                    verification_reason: Some(verification.reason.clone()),
                    should_replan: Some(verification.should_replan),
                })
                .await;

            // Persist the result back to the conversation — or the replan message
            // when the outcome failed verification.
            let llm_result = engine::summarize_tool_result(&tool_result.content);
            let decision_msg = if verification.should_replan {
                crate::agent::verifier::replan_message(&approval.tool_name, &verification.reason)
            } else {
                llm_result
            };
            add_tool_message(
                &db,
                &approval,
                decision_msg,
                chrono::Utc::now().timestamp_millis(),
            );
        } else {
            // Reject: never execute; record the refusal so the agent can replan.
            let rejected = format!(
                "用户拒绝了工具 {} 的执行请求。原始操作未执行。",
                approval.tool_name
            );
            add_tool_message(
                &db,
                &approval,
                rejected,
                chrono::Utc::now().timestamp_millis(),
            );
        }

        // 3. Resume the agent loop with the updated context.
        resume_agent(
            &server,
            &db,
            &tool_registry,
            &config,
            &log_buffer,
            &system_prompt,
            &approval,
            &tx,
        )
        .await;
    });

    let stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            let json = serde_json::to_string(&event).unwrap_or_default();
            yield Ok(Event::default().event("agent-event").data(json));
        }
    };

    Sse::new(stream)
}

/// Insert a tool message for the paused tool call into the conversation.
fn add_tool_message(db: &Database, approval: &PendingApproval, content: String, now: i64) {
    let _ = db.add_message(&MessageRow {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: approval.conversation_id.clone(),
        role: "tool".to_string(),
        content,
        tool_calls: None,
        tool_call_id: Some(approval.tool_call_id.clone()),
        tool_name: Some(approval.tool_name.clone()),
        tool_result: None,
        created_at: now,
    });
}

/// Rebuild the agent state from persisted history and continue the loop.
async fn resume_agent(
    server: &AppServer,
    db: &Database,
    tool_registry: &Arc<ToolRegistry>,
    config: &AppConfig,
    log_buffer: &LogBuffer,
    system_prompt: &str,
    approval: &PendingApproval,
    tx: &Sender<AgentEvent>,
) {
    let mut agent_state = AgentState::new(system_prompt.to_string());
    if let Ok(conv) = db.get_conversation(&approval.conversation_id) {
        let msgs: Vec<ChatMessage> = conv
            .messages
            .iter()
            .map(|m| ChatMessage {
                role: m.role.clone(),
                content: if m.content.is_empty() {
                    None
                } else {
                    Some(m.content.clone())
                },
                tool_calls: m
                    .tool_calls
                    .as_ref()
                    .and_then(|tc| serde_json::from_str(tc).ok()),
                tool_call_id: m.tool_call_id.clone(),
                name: m.tool_name.clone(),
            })
            .collect();
        agent_state.load_history(msgs);
    }
    let prev_count = agent_state.messages.len();

    let cancel_token = CancellationToken::new();
    server
        .active_tasks
        .lock()
        .insert(approval.conversation_id.clone(), cancel_token.clone());

    let llm_client = LlmClient::new(&config.model);
    let verifier = crate::agent::verifier::DefaultVerifier::new(&server.workspace_root);

    let result = engine::run_react_loop_with_channel(
        &mut agent_state,
        &llm_client,
        tool_registry,
        &server.approval_store,
        &verifier,
        config,
        &approval.conversation_id,
        &cancel_token,
        tx,
        log_buffer,
    )
    .await;

    // Persist any new messages produced during the resumed run.
    let now = chrono::Utc::now().timestamp_millis();
    for msg in &agent_state.messages[prev_count.min(agent_state.messages.len())..] {
        if msg.role == "user" || msg.role == "system" {
            continue;
        }
        let _ = db.add_message(&MessageRow {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: approval.conversation_id.clone(),
            role: msg.role.clone(),
            content: msg.content.clone().unwrap_or_default(),
            tool_calls: msg
                .tool_calls
                .as_ref()
                .map(|tc| serde_json::to_string(tc).unwrap_or_default()),
            tool_call_id: msg.tool_call_id.clone(),
            tool_name: msg.name.clone(),
            tool_result: None,
            created_at: now,
        });
    }

    server.active_tasks.lock().remove(&approval.conversation_id);

    match result {
        Ok(RunOutcome::Done { .. }) => {
            log_buffer.push("info", "agent", "审批后 Agent 完成");
        }
        Ok(RunOutcome::Paused { approval_id }) => {
            log_buffer.push(
                "warn",
                "agent",
                &format!("审批后 Agent 再次暂停，等待审批 {}", approval_id),
            );
        }
        Err(e) => {
            log_buffer.push("error", "agent", &format!("审批后 Agent 错误: {}", e));
            let _ = tx
                .send(AgentEvent {
                    event_type: "error".into(),
                    conversation_id: approval.conversation_id.clone(),
                    error: Some(e),
                    token: None,
                    tool_call_id: None,
                    tool_name: None,
                    args: None,
                    result: None,
                    status: None,
                    message_id: None,
                    risk_level: None,
                    reason: None,
                    approval_id: Some(approval.approval_id.clone()),
                    verification_success: None,
                    verification_reason: None,
                    should_replan: None,
                })
                .await;
        }
    }
}
