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
use crate::agent::verifier::DefaultVerifier;
use crate::config::types::AppConfig;
use crate::db::{Database, MessageRow};
use crate::llm::client::LlmClient;
use crate::llm::types::ChatMessage;
use crate::safety::approval::{ApprovalError, PendingApproval};
use crate::safety::execution_gateway::{SecurityExecutionOutcome, SecurityGatewayError};
use crate::safety::{
    AuditEventInput, AuditEventType, SecurityExecutionGateway, SecurityExecutionRequest,
    SecuritySubject,
};
use crate::server::{AppServer, LogBuffer};
use crate::tools::registry::ToolRegistry;

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

fn record_approval_resolved(server: &AppServer, approval: &PendingApproval) {
    let resolution = approval.status.to_string();
    let role_key = server
        .db
        .resolve_active_role_binding(&approval.subject_id)
        .unwrap_or_else(|| "restricted".to_string());
    if let Err(error) = server.audit_recorder.record(AuditEventInput {
        event_type: AuditEventType::ApprovalResolved,
        correlation_id: approval.tool_call_id.clone(),
        request_id: approval.tool_call_id.clone(),
        subject_id: approval.subject_id.clone(),
        role_key,
        conversation_id: Some(approval.conversation_id.clone()),
        tool_call_id: Some(approval.tool_call_id.clone()),
        tool_name: Some(approval.tool_name.clone()),
        risk_level: Some(approval.risk_level.to_string()),
        decision_status: Some(resolution.clone()),
        request: None,
        result: None,
        details: serde_json::json!({
            "phase": AuditEventType::ApprovalResolved.as_str(),
            "approval_id": approval.approval_id.clone(),
            "resolution": resolution,
        }),
        ..Default::default()
    }) {
        tracing::error!(
            approval_id = %approval.approval_id,
            resolution = %approval.status,
            error = %error,
            "failed to persist approval resolution audit"
        );
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
    let approval = result.map_err(|e| (status_for(&e), e.to_string()))?;
    record_approval_resolved(server, &approval);
    Ok(approval)
}

async fn execute_approved_tool(
    server: &AppServer,
    config: &AppConfig,
    approval: &PendingApproval,
    tool_registry: &Arc<ToolRegistry>,
) -> Result<SecurityExecutionOutcome, SecurityGatewayError> {
    let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
        config.sandbox.clone(),
        server.workspace_root.clone(),
        Arc::clone(tool_registry),
        Arc::new(DefaultVerifier::new(&server.workspace_root)),
        Arc::new(server.audit_recorder.clone()),
    )
    .with_db(Arc::new(server.db.clone_connection()));
    let request = SecurityExecutionRequest {
        conversation_id: approval.conversation_id.clone(),
        tool_call_id: approval.tool_call_id.clone(),
        tool_name: approval.tool_name.clone(),
        arguments: approval.arguments.clone(),
        subject: SecuritySubject::local_user(),
    };

    gateway
        .execute_approved(&request, approval.risk_level)
        .await
}

/// POST /api/approvals/:id/approve — SSE stream resuming the agent.
pub async fn approve_handler(
    State(server): State<Arc<AppServer>>,
    Path(approval_id): Path<String>,
    Json(body): Json<ApprovalDecisionRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let approval =
        resolve_and_consume(&server, &approval_id, body.conversation_id.as_deref(), true)?;
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
            record_approval_resolved(&server, &a);
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
        // Build ONE MCP-aware runtime registry snapshot for the whole resume.
        // The same registry is used to (1) execute the approved original tool
        // and (2) resume the ReAct loop, so evaluation and execution agree and
        // stale binding approvals fail closed.
        let tool_registry = server.build_agent_tool_registry().await;
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

            let gateway_outcome =
                execute_approved_tool(&server, &config, &approval, &tool_registry).await;
            match gateway_outcome {
                Ok(SecurityExecutionOutcome::Executed {
                    tool_result,
                    verification,
                }) => {
                    let _ = tx
                        .send(AgentEvent {
                            event_type: "tool_end".into(),
                            conversation_id: approval.conversation_id.clone(),
                            token: None,
                            tool_call_id: Some(approval.tool_call_id.clone()),
                            tool_name: Some(approval.tool_name.clone()),
                            args: None,
                            result: Some(tool_result.content.clone()),
                            status: Some(
                                (if tool_result.ok { "success" } else { "error" }).to_string(),
                            ),
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

                    let llm_result = engine::summarize_tool_result(&tool_result.content);
                    let decision_msg = if verification.should_replan {
                        crate::agent::verifier::replan_message(
                            &approval.tool_name,
                            &verification.reason,
                        )
                    } else {
                        llm_result
                    };
                    add_tool_message(
                        &db,
                        &approval,
                        decision_msg,
                        chrono::Utc::now().timestamp_millis(),
                    );
                }
                Ok(SecurityExecutionOutcome::Denied { reason }) => {
                    let reason = crate::utils::text::truncate_chars(&reason, 500);
                    let _ = tx
                        .send(AgentEvent {
                            event_type: "tool_end".into(),
                            conversation_id: approval.conversation_id.clone(),
                            tool_call_id: Some(approval.tool_call_id.clone()),
                            tool_name: Some(approval.tool_name.clone()),
                            result: Some(reason.clone()),
                            status: Some("error".to_string()),
                            approval_id: Some(approval.approval_id.clone()),
                            token: None,
                            args: None,
                            error: None,
                            message_id: None,
                            risk_level: Some(approval.risk_level.to_string()),
                            reason: Some(reason.clone()),
                            verification_success: None,
                            verification_reason: None,
                            should_replan: Some(true),
                        })
                        .await;
                    add_tool_message(
                        &db,
                        &approval,
                        format!(
                            "Approved tool execution denied by current security policy: {reason}"
                        ),
                        chrono::Utc::now().timestamp_millis(),
                    );
                }
                Ok(SecurityExecutionOutcome::RequiresApproval { .. }) => {
                    let reason =
                        "approved execution unexpectedly requested another approval".to_string();
                    let _ = tx
                        .send(AgentEvent {
                            event_type: "tool_end".into(),
                            conversation_id: approval.conversation_id.clone(),
                            tool_call_id: Some(approval.tool_call_id.clone()),
                            tool_name: Some(approval.tool_name.clone()),
                            result: Some(reason.clone()),
                            status: Some("error".to_string()),
                            approval_id: Some(approval.approval_id.clone()),
                            token: None,
                            args: None,
                            error: None,
                            message_id: None,
                            risk_level: Some(approval.risk_level.to_string()),
                            reason: Some(reason.clone()),
                            verification_success: None,
                            verification_reason: None,
                            should_replan: Some(true),
                        })
                        .await;
                    add_tool_message(
                        &db,
                        &approval,
                        reason,
                        chrono::Utc::now().timestamp_millis(),
                    );
                }
                Err(error) => {
                    let reason = crate::utils::text::truncate_chars(&error.to_string(), 500);
                    let _ = tx
                        .send(AgentEvent {
                            event_type: "tool_end".into(),
                            conversation_id: approval.conversation_id.clone(),
                            tool_call_id: Some(approval.tool_call_id.clone()),
                            tool_name: Some(approval.tool_name.clone()),
                            result: Some(reason.clone()),
                            status: Some("error".to_string()),
                            approval_id: Some(approval.approval_id.clone()),
                            token: None,
                            args: None,
                            error: None,
                            message_id: None,
                            risk_level: Some(approval.risk_level.to_string()),
                            reason: Some(reason.clone()),
                            verification_success: None,
                            verification_reason: None,
                            should_replan: Some(true),
                        })
                        .await;
                    add_tool_message(
                        &db,
                        &approval,
                        format!("Approved tool execution failed closed: {reason}"),
                        chrono::Utc::now().timestamp_millis(),
                    );
                }
            }
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
    let security_gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
        config.sandbox.clone(),
        server.workspace_root.clone(),
        Arc::clone(tool_registry),
        Arc::new(DefaultVerifier::new(&server.workspace_root)),
        Arc::new(server.audit_recorder.clone()),
    )
    .with_db(Arc::new(server.db.clone_connection()));

    let result = engine::run_react_loop_with_channel(
        &mut agent_state,
        &llm_client,
        tool_registry,
        &server.approval_store,
        &security_gateway,
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

#[cfg(test)]
mod tests {
    use super::{
        cancel_handler, execute_approved_tool, resolve_and_consume, ApprovalDecisionRequest,
    };
    use async_trait::async_trait;
    use axum::{
        extract::{Path, State},
        Json,
    };
    use parking_lot::Mutex;
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

    use crate::{
        db::{SecurityAuditEvent, SecurityAuditQuery},
        safety::execution_gateway::SecurityExecutionOutcome,
        safety::ControlSession,
        server::AppServer,
        tools::{trait_def::RiskLevel, Tool, ToolRegistry, ToolResult},
    };

    struct TempDatabase(PathBuf);

    struct CountingTool {
        name: &'static str,
        executions: Arc<AtomicUsize>,
        arguments: Arc<Mutex<Vec<serde_json::Value>>>,
    }

    #[async_trait]
    impl Tool for CountingTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "approval gateway test tool"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        async fn execute(&self, args: serde_json::Value) -> ToolResult {
            self.executions.fetch_add(1, Ordering::SeqCst);
            self.arguments.lock().push(args);
            ToolResult::success("executed")
        }
    }

    impl TempDatabase {
        fn new(label: &str) -> Self {
            Self(std::env::temp_dir().join(format!(
                "yilian-approval-audit-{label}-{}.db",
                uuid::Uuid::new_v4()
            )))
        }
    }

    impl Drop for TempDatabase {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn test_server(label: &str) -> (TempDatabase, Arc<AppServer>) {
        let temp = TempDatabase::new(label);
        let server =
            AppServer::new_with_control_session(&temp.0, ".", ControlSession::generate()).unwrap();
        (temp, Arc::new(server))
    }

    fn test_server_with_tool(
        label: &str,
        tool_name: &'static str,
    ) -> (
        TempDatabase,
        Arc<AppServer>,
        Arc<ToolRegistry>,
        Arc<AtomicUsize>,
        Arc<Mutex<Vec<serde_json::Value>>>,
    ) {
        let temp = TempDatabase::new(label);
        let executions = Arc::new(AtomicUsize::new(0));
        let arguments = Arc::new(Mutex::new(Vec::new()));
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(CountingTool {
            name: tool_name,
            executions: Arc::clone(&executions),
            arguments: Arc::clone(&arguments),
        }));
        let registry = Arc::new(registry);
        let mut server =
            AppServer::new_with_control_session(&temp.0, ".", ControlSession::generate()).unwrap();
        server.tool_registry = Arc::clone(&registry);
        (temp, Arc::new(server), registry, executions, arguments)
    }

    #[tokio::test]
    async fn approve_adapter_executes_exact_original_call_once_through_gateway() {
        let (_temp, server, registry, executions, arguments) =
            test_server_with_tool("gateway", "bash");
        server.config.write().sandbox.profile = crate::config::types::SandboxProfile::Open;
        let pending = server.approval_store.create(
            "conversation-1".to_string(),
            "tool-call-1".to_string(),
            "bash".to_string(),
            serde_json::json!({"command": "echo exact-original"}),
            RiskLevel::High,
            "approval required".to_string(),
            "local-user".to_string(),
        );
        let consumed = resolve_and_consume(
            &server,
            &pending.approval_id,
            Some(&pending.conversation_id),
            true,
        )
        .unwrap();
        let config = server.config.read().clone();

        let outcome = execute_approved_tool(&server, &config, &consumed, &registry)
            .await
            .unwrap();

        assert!(matches!(outcome, SecurityExecutionOutcome::Executed { .. }));
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert_eq!(
            arguments.lock().as_slice(),
            &[serde_json::json!({"command": "echo exact-original"})]
        );
        assert!(resolve_and_consume(
            &server,
            &pending.approval_id,
            Some(&pending.conversation_id),
            true,
        )
        .is_err());
        assert_eq!(executions.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn approve_adapter_sandbox_deny_does_not_execute_tool() {
        let (_temp, server, registry, executions, _arguments) =
            test_server_with_tool("sandbox-deny", "write_file");
        server.config.write().sandbox.profile = crate::config::types::SandboxProfile::ReadOnly;
        let pending = server.approval_store.create(
            "conversation-1".to_string(),
            "tool-call-1".to_string(),
            "write_file".to_string(),
            serde_json::json!({"path": "notes.txt", "content": "blocked"}),
            RiskLevel::Medium,
            "approval required".to_string(),
            "local-user".to_string(),
        );
        let consumed = resolve_and_consume(
            &server,
            &pending.approval_id,
            Some(&pending.conversation_id),
            true,
        )
        .unwrap();
        let config = server.config.read().clone();

        let outcome = execute_approved_tool(&server, &config, &consumed, &registry)
            .await
            .unwrap();

        assert!(matches!(outcome, SecurityExecutionOutcome::Denied { .. }));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
    }

    fn create_pending(server: &AppServer) -> crate::safety::approval::PendingApproval {
        server.approval_store.create(
            "conversation-1".to_string(),
            "tool-call-1".to_string(),
            "bash".to_string(),
            serde_json::json!({"command": "echo safe", "token": "must-not-be-audited"}),
            RiskLevel::High,
            "high-risk tool".to_string(),
            "local-user".to_string(),
        )
    }

    fn resolution_events(server: &AppServer) -> Vec<SecurityAuditEvent> {
        server
            .audit_recorder
            .query(&SecurityAuditQuery {
                correlation_id: Some("tool-call-1".to_string()),
                event_type: Some("approval_resolved".to_string()),
                ..Default::default()
            })
            .unwrap()
    }

    fn assert_resolution(event: &SecurityAuditEvent, approval_id: &str, resolution: &str) {
        assert_eq!(event.conversation_id.as_deref(), Some("conversation-1"));
        assert_eq!(event.tool_call_id.as_deref(), Some("tool-call-1"));
        assert_eq!(event.tool_name.as_deref(), Some("bash"));
        assert_eq!(event.risk_level.as_deref(), Some("high"));
        assert_eq!(event.decision_status.as_deref(), Some(resolution));
        assert_eq!(event.details["context"]["approval_id"], approval_id);
        assert_eq!(event.details["context"]["resolution"], resolution);
        assert!(!serde_json::to_string(event)
            .unwrap()
            .contains("must-not-be-audited"));
    }

    #[test]
    fn approve_records_approved_resolution_once() {
        let (_temp, server) = test_server("approve");
        let pending = create_pending(&server);

        let resolved = resolve_and_consume(
            &server,
            &pending.approval_id,
            Some(&pending.conversation_id),
            true,
        )
        .unwrap();

        assert_eq!(resolved.status.to_string(), "approved");
        let events = resolution_events(&server);
        assert_eq!(events.len(), 1);
        assert_resolution(&events[0], &pending.approval_id, "approved");
    }

    #[test]
    fn reject_records_rejected_resolution_once() {
        let (_temp, server) = test_server("reject");
        let pending = create_pending(&server);

        let resolved = resolve_and_consume(
            &server,
            &pending.approval_id,
            Some(&pending.conversation_id),
            false,
        )
        .unwrap();

        assert_eq!(resolved.status.to_string(), "rejected");
        let events = resolution_events(&server);
        assert_eq!(events.len(), 1);
        assert_resolution(&events[0], &pending.approval_id, "rejected");
    }

    #[tokio::test]
    async fn cancel_records_cancelled_resolution_once() {
        let (_temp, server) = test_server("cancel");
        let pending = create_pending(&server);

        let Json(response) = cancel_handler(
            State(Arc::clone(&server)),
            Path(pending.approval_id.clone()),
            Json(ApprovalDecisionRequest {
                conversation_id: Some(pending.conversation_id.clone()),
            }),
        )
        .await;

        assert_eq!(response["ok"], true);
        assert_eq!(response["status"], "cancelled");
        let events = resolution_events(&server);
        assert_eq!(events.len(), 1);
        assert_resolution(&events[0], &pending.approval_id, "cancelled");
    }

    #[test]
    fn repeated_consumption_does_not_record_second_resolution() {
        let (_temp, server) = test_server("duplicate");
        let pending = create_pending(&server);

        resolve_and_consume(
            &server,
            &pending.approval_id,
            Some(&pending.conversation_id),
            true,
        )
        .unwrap();
        assert!(resolve_and_consume(
            &server,
            &pending.approval_id,
            Some(&pending.conversation_id),
            false,
        )
        .is_err());

        assert_eq!(resolution_events(&server).len(), 1);
    }

    // ── MCP runtime integration ──

    use crate::db::McpServer;
    use crate::mcp::McpTool;
    use crate::tools::McpToolAdapter;

    fn mcp_server(id: &str, command: Option<&str>) -> McpServer {
        McpServer {
            id: id.to_string(),
            name: "filesystem".to_string(),
            transport: "stdio".to_string(),
            command: command.map(str::to_string),
            args: None,
            url: None,
            env: None,
            enabled: true,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn mcp_tool(name: &str) -> McpTool {
        McpTool {
            name: name.to_string(),
            description: Some("MCP test tool".to_string()),
            input_schema: serde_json::json!({"type": "object"}),
        }
    }

    #[tokio::test]
    async fn approved_mcp_tool_reaches_adapter_executor_and_fails_closed() {
        // command = None: the adapter's execute() goes through call_stdio_tool,
        // which rejects config before spawning. This proves the approved MCP
        // tool really reaches the adapter executor (no spawn needed).
        let (_temp, server, _server_registry, _executions, _arguments) =
            test_server_with_tool("mcp-gateway", "x");
        server.config.write().sandbox.profile = crate::config::types::SandboxProfile::Open;

        let mcp = mcp_server("550e8400-e29b-41d4-a716-446655440000", None);
        let adapter = McpToolAdapter::new(&mcp, &mcp_tool("search")).unwrap();
        let exposed = adapter.name().to_string();
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(adapter));
        let registry = Arc::new(registry);

        let pending = server.approval_store.create(
            "conversation-1".to_string(),
            "mcp-call-1".to_string(),
            exposed.clone(),
            serde_json::json!({"q": "x"}),
            RiskLevel::High,
            "mcp approval required".to_string(),
            "local-user".to_string(),
        );
        let consumed = resolve_and_consume(
            &server,
            &pending.approval_id,
            Some(&pending.conversation_id),
            true,
        )
        .unwrap();
        let config = server.config.read().clone();

        let outcome = execute_approved_tool(&server, &config, &consumed, &registry)
            .await
            .unwrap();

        // Reached the adapter executor (not ToolNotFound / RequiresApproval),
        // and the underlying call_stdio_tool failed closed on missing command.
        match outcome {
            SecurityExecutionOutcome::Executed { tool_result, .. } => {
                assert!(!tool_result.ok);
                assert!(tool_result.content.contains("MCP tool execution failed"));
            }
            other => panic!("expected executed outcome, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn stale_mcp_approval_fails_closed() {
        // Old binding: command = "old-command" → approved_tool_name.
        let old_server = mcp_server("550e8400-e29b-41d4-a716-446655440000", Some("old-command"));
        let old_adapter = McpToolAdapter::new(&old_server, &mcp_tool("search")).unwrap();
        let approved_tool_name = old_adapter.name().to_string();

        // Config changed while waiting for approval → new binding identity.
        let mut new_server =
            mcp_server("550e8400-e29b-41d4-a716-446655440000", Some("new-command"));
        new_server.updated_at += 1;
        let new_adapter = McpToolAdapter::new(&new_server, &mcp_tool("search")).unwrap();
        assert_ne!(approved_tool_name, new_adapter.name());

        // Runtime registry only contains the NEW adapter.
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(new_adapter));
        let registry = Arc::new(registry);

        let (_temp, server, _server_registry, _executions, _arguments) =
            test_server_with_tool("stale-mcp", "x");
        let pending = server.approval_store.create(
            "conversation-1".to_string(),
            "stale-call-1".to_string(),
            approved_tool_name.clone(),
            serde_json::json!({"q": "x"}),
            RiskLevel::High,
            "mcp approval required".to_string(),
            "local-user".to_string(),
        );
        let consumed = resolve_and_consume(
            &server,
            &pending.approval_id,
            Some(&pending.conversation_id),
            true,
        )
        .unwrap();
        let config = server.config.read().clone();

        // Old approval's tool_name cannot resolve in the new registry → Err.
        let outcome = execute_approved_tool(&server, &config, &consumed, &registry).await;
        assert!(outcome.is_err(), "stale approval must fail closed");
    }
}
