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
use std::pin::Pin;
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
use crate::workflow::{cancel_workflow_approval, resolve_workflow_approval, WorkflowRunId};

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

/// How to route an approval decision. Workflow-bound approvals resume a
/// [`crate::workflow::WorkflowRun`]; task-agent approvals execute the original
/// call through the gateway and finalize the agent step; everything else
/// resumes a Chat Agent.
enum ApprovalTarget {
    Agent,
    Workflow,
    TaskAgent,
    InvalidBinding,
}

/// Classify an approval from its *trusted internal fields* (never client input).
///
/// A task-agent approval has all three task fields `Some`; a workflow approval
/// has all three workflow fields `Some`; an agent approval has none. Any partial
/// binding is treated as data corruption and fails closed rather than falling
/// back to a different runtime.
fn classify_approval(approval: &PendingApproval) -> ApprovalTarget {
    let has_task = approval.task_id.is_some()
        || approval.task_execution_id.is_some()
        || approval.agent_execution_id.is_some();
    if has_task {
        return match (
            &approval.task_id,
            &approval.task_execution_id,
            &approval.agent_execution_id,
        ) {
            (Some(_), Some(_), Some(_)) => ApprovalTarget::TaskAgent,
            _ => ApprovalTarget::InvalidBinding,
        };
    }

    let has_execution = approval.execution_id.is_some();
    let has_run = approval.workflow_run_id.is_some();
    let has_node = approval.workflow_node_id.is_some();
    match (has_execution, has_run, has_node) {
        (false, false, false) => ApprovalTarget::Agent,
        (true, true, true) => ApprovalTarget::Workflow,
        _ => ApprovalTarget::InvalidBinding,
    }
}

type ApprovalStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

fn box_stream<S>(stream: S) -> ApprovalStream
where
    S: Stream<Item = Result<Event, Infallible>> + Send + 'static,
{
    Box::pin(stream)
}

fn sse_event(event: &str, data: serde_json::Value) -> Event {
    Event::default().event(event).data(data.to_string())
}

/// Build the production security gateway for workflow resume — same components
/// as the normal workflow runtime (MCP-aware registry, verifier, audit, DB).
async fn build_workflow_gateway(server: &AppServer) -> Arc<SecurityExecutionGateway> {
    let tool_registry = server.build_agent_tool_registry().await;
    let config = server.config.read().clone();
    Arc::new(
        SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            config.sandbox.clone(),
            server.workspace_root.clone(),
            Arc::clone(&tool_registry),
            Arc::new(DefaultVerifier::new(&server.workspace_root)),
            Arc::new(server.audit_recorder.clone()),
        )
        .with_db(Arc::new(server.db.clone_connection())),
    )
}

fn read_run_status(server: &AppServer, approval: &PendingApproval) -> String {
    let Some(run_id) = &approval.workflow_run_id else {
        return "failed".to_string();
    };
    let Ok(run_id) = WorkflowRunId::new(run_id.clone()) else {
        return "failed".to_string();
    };
    server
        .db
        .get_workflow_run(&run_id)
        .ok()
        .flatten()
        .map(|stored| stored.run.status.to_string())
        .unwrap_or_else(|| "failed".to_string())
}

/// Build the workflow resume SSE stream: resolve the approval, resume the
/// workflow run through the gateway, then emit a final run-state event.
///
/// Event ordering is truthful: `approval_resolved` is only emitted AFTER the
/// approval has been genuinely consumed and the resolution audit recorded. A
/// failed resolution emits a safe `workflow_approval_error` event and never a
/// fake `approval_resolved` nor a duplicate audit entry.
fn workflow_approval_stream(
    server: Arc<AppServer>,
    approval: PendingApproval,
    approve: bool,
) -> ApprovalStream {
    let approval_id = approval.approval_id.clone();
    let workflow_run_id = approval.workflow_run_id.clone();
    let workflow_node_id = approval.workflow_node_id.clone();
    let subject_id = approval.subject_id.clone();

    let stream = async_stream::stream! {
        // Resolve FIRST. No event is emitted until the decision truly happened.
        let gateway = build_workflow_gateway(&server).await;
        let result = resolve_workflow_approval(
            &gateway,
            &server.approval_store,
            &server.db,
            &approval_id,
            &subject_id,
            approve,
            &CancellationToken::new(),
        ).await;

        match result {
            Ok(()) => {
                // The approval is now consumed. Verify its status matches the
                // requested decision before recording anything — this prevents
                // re-recording a stale status on a malformed consumption.
                let Some(consumed) = server.approval_store.get(&approval_id) else {
                    yield Ok(workflow_approval_error_event(&approval_id, &workflow_run_id, &workflow_node_id));
                    return;
                };
                let expected = if approve {
                    crate::safety::ApprovalStatus::Approved
                } else {
                    crate::safety::ApprovalStatus::Rejected
                };
                if consumed.status != expected {
                    tracing::warn!(
                        approval_id = %approval_id,
                        approve = approve,
                        actual = %consumed.status,
                        "workflow approval consumed to an unexpected status"
                    );
                    yield Ok(workflow_approval_error_event(&approval_id, &workflow_run_id, &workflow_node_id));
                    return;
                }

                // Record the resolution exactly once.
                record_approval_resolved(&server, &consumed);

                let decision = if approve { "approved" } else { "rejected" };
                yield Ok(sse_event("approval_resolved", serde_json::json!({
                    "type": "approval_resolved",
                    "approval_id": approval_id,
                    "status": decision,
                })));

                // Read the real persisted run status (never fabricated).
                let run_status = read_run_status(&server, &approval);
                yield Ok(sse_event("workflow_run_updated", serde_json::json!({
                    "type": "workflow_run_updated",
                    "workflow_run_id": workflow_run_id,
                    "workflow_node_id": workflow_node_id,
                    "status": run_status,
                })));
            }
            Err(error) => {
                // Failure: no approval_resolved, no new audit. Log internally
                // and emit a safe, non-leaking failure event.
                tracing::warn!(
                    approval_id = %approval_id,
                    approve = approve,
                    error = %error,
                    "workflow approval resolution failed"
                );
                yield Ok(workflow_approval_error_event(&approval_id, &workflow_run_id, &workflow_node_id));
            }
        }
    };
    box_stream(stream)
}

fn workflow_approval_error_event(
    approval_id: &str,
    workflow_run_id: &Option<String>,
    workflow_node_id: &Option<String>,
) -> Event {
    sse_event(
        "workflow_approval_error",
        serde_json::json!({
            "type": "workflow_approval_error",
            "approval_id": approval_id,
            "workflow_run_id": workflow_run_id,
            "workflow_node_id": workflow_node_id,
            "error": "工作流审批恢复失败",
        }),
    )
}

/// Resolve a task-agent approval, then emit a truthful SSE: resolve FIRST, emit
/// `approval_resolved` only on success, then `task_updated` with the real
/// persisted task status. Failures emit only `task_approval_error` — never a
/// fabricated `approved`/`rejected`.
fn task_agent_approval_stream(
    server: Arc<AppServer>,
    approval: PendingApproval,
    approve: bool,
) -> ApprovalStream {
    let approval_id = approval.approval_id.clone();
    let task_id = approval.task_id.clone();
    let task_execution_id = approval.task_execution_id.clone();

    let stream = async_stream::stream! {
        let gateway = build_workflow_gateway(&server).await;
        let result = crate::task::resolve_task_agent_approval(
            &gateway,
            &server.approval_store,
            &server.db,
            &approval_id,
            approve,
        ).await;

        match result {
            Ok(()) => {
                let Some(consumed) = server.approval_store.get(&approval_id) else {
                    yield Ok(sse_event("task_approval_error", serde_json::json!({
                        "type": "task_approval_error",
                        "approval_id": approval_id,
                        "task_id": task_id,
                        "error": "任务审批恢复失败",
                    })));
                    return;
                };
                record_approval_resolved(&server, &consumed);
                let decision = if approve { "approved" } else { "rejected" };
                yield Ok(sse_event("approval_resolved", serde_json::json!({
                    "type": "approval_resolved",
                    "approval_id": approval_id,
                    "status": decision,
                })));

                // Report the real persisted task status.
                let status = task_id.as_ref()
                    .and_then(|id| crate::task::TaskId::new(id.clone()).ok())
                    .and_then(|id| server.db.get_task(&id).ok().flatten())
                    .map(|t| t.status.to_string())
                    .unwrap_or_else(|| "failed".to_string());
                yield Ok(sse_event("task_updated", serde_json::json!({
                    "type": "task_updated",
                    "task_id": task_id,
                    "task_execution_id": task_execution_id,
                    "status": status,
                })));
            }
            Err(_) => {
                yield Ok(sse_event("task_approval_error", serde_json::json!({
                    "type": "task_approval_error",
                    "approval_id": approval_id,
                    "task_id": task_id,
                    "error": "任务审批恢复失败",
                })));
            }
        }
    };
    box_stream(stream)
}

async fn cancel_task_agent_approval(
    server: &AppServer,
    approval_id: &str,
    approval: &PendingApproval,
) -> Result<(), String> {
    let consumed = server
        .approval_store
        .cancel(approval_id, &approval.conversation_id)
        .map_err(|error| error.to_string())?;

    let task_id = consumed
        .task_id
        .as_ref()
        .ok_or_else(|| "审批未绑定任务".to_string())?;
    let execution_id = consumed
        .task_execution_id
        .as_ref()
        .ok_or_else(|| "审批未绑定执行".to_string())?;
    let agent_execution_id = consumed
        .agent_execution_id
        .as_ref()
        .ok_or_else(|| "审批未绑定 Agent 执行".to_string())?;

    let task_id = crate::task::TaskId::new(task_id.clone()).map_err(|e| e.to_string())?;
    let execution_id =
        crate::task::TaskExecutionId::new(execution_id.clone()).map_err(|e| e.to_string())?;
    let agent_execution_id = crate::task::AgentExecutionId::new(agent_execution_id.clone())
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().timestamp_millis();
    if let Some(mut agent_execution) = server.db.get_agent_execution(&agent_execution_id)? {
        agent_execution.status = crate::task::AgentExecutionStatus::Cancelled;
        agent_execution.finished_at = Some(now);
        agent_execution.updated_at = now;
        server.db.update_agent_execution(&agent_execution)?;
    }
    if let Some(mut execution) = server.db.get_task_execution(&execution_id)? {
        execution.status = crate::task::TaskExecutionStatus::Cancelled;
        execution.finished_at = Some(now);
        execution.updated_at = now;
        server.db.update_task_execution(&execution)?;
    }
    if let Some(mut task) = server.db.get_task(&task_id)? {
        task.status = crate::task::TaskStatus::Cancelled;
        task.updated_at = now;
        server.db.update_task(&task)?;
    }
    Ok(())
}

/// POST /api/approvals/:id/approve — SSE stream resuming the agent or a
/// workflow run, depending on the approval's binding.
pub async fn approve_handler(
    State(server): State<Arc<AppServer>>,
    Path(approval_id): Path<String>,
    Json(body): Json<ApprovalDecisionRequest>,
) -> Result<Sse<ApprovalStream>, (StatusCode, String)> {
    let lookup = server
        .approval_store
        .get(&approval_id)
        .ok_or((StatusCode::NOT_FOUND, "审批不存在".to_string()))?;
    match classify_approval(&lookup) {
        ApprovalTarget::Agent => {
            let approval =
                resolve_and_consume(&server, &approval_id, body.conversation_id.as_deref(), true)?;
            Ok(Sse::new(resume_stream(server, approval, true)))
        }
        ApprovalTarget::Workflow => {
            // Fail fast on a consumed approval: a surface-normal SSE that only
            // fails inside the stream would mislead the client. Concurrency is
            // still ultimately bounded by the atomic consume-once.
            if lookup.status != crate::safety::ApprovalStatus::Pending {
                return Err((
                    StatusCode::CONFLICT,
                    "审批已被处理，不能重复操作".to_string(),
                ));
            }
            Ok(Sse::new(workflow_approval_stream(server, lookup, true)))
        }
        ApprovalTarget::TaskAgent => {
            if lookup.status != crate::safety::ApprovalStatus::Pending {
                return Err((
                    StatusCode::CONFLICT,
                    "审批已被处理，不能重复操作".to_string(),
                ));
            }
            Ok(Sse::new(task_agent_approval_stream(server, lookup, true)))
        }
        ApprovalTarget::InvalidBinding => Err((StatusCode::CONFLICT, "审批绑定不完整".to_string())),
    }
}

/// POST /api/approvals/:id/reject — SSE stream rejecting the agent or a
/// workflow run.
pub async fn reject_handler(
    State(server): State<Arc<AppServer>>,
    Path(approval_id): Path<String>,
    Json(body): Json<ApprovalDecisionRequest>,
) -> Result<Sse<ApprovalStream>, (StatusCode, String)> {
    let lookup = server
        .approval_store
        .get(&approval_id)
        .ok_or((StatusCode::NOT_FOUND, "审批不存在".to_string()))?;
    match classify_approval(&lookup) {
        ApprovalTarget::Agent => {
            let approval = resolve_and_consume(
                &server,
                &approval_id,
                body.conversation_id.as_deref(),
                false,
            )?;
            Ok(Sse::new(resume_stream(server, approval, false)))
        }
        ApprovalTarget::Workflow => {
            if lookup.status != crate::safety::ApprovalStatus::Pending {
                return Err((
                    StatusCode::CONFLICT,
                    "审批已被处理，不能重复操作".to_string(),
                ));
            }
            Ok(Sse::new(workflow_approval_stream(server, lookup, false)))
        }
        ApprovalTarget::TaskAgent => {
            if lookup.status != crate::safety::ApprovalStatus::Pending {
                return Err((
                    StatusCode::CONFLICT,
                    "审批已被处理，不能重复操作".to_string(),
                ));
            }
            Ok(Sse::new(task_agent_approval_stream(server, lookup, false)))
        }
        ApprovalTarget::InvalidBinding => Err((StatusCode::CONFLICT, "审批绑定不完整".to_string())),
    }
}

/// POST /api/approvals/:id/cancel — cancel a pending approval (no resume).
pub async fn cancel_handler(
    State(server): State<Arc<AppServer>>,
    Path(approval_id): Path<String>,
    Json(body): Json<ApprovalDecisionRequest>,
) -> Json<serde_json::Value> {
    let Some(lookup) = server.approval_store.get(&approval_id) else {
        return Json(serde_json::json!({"ok": false, "error": "审批不存在"}));
    };
    match classify_approval(&lookup) {
        ApprovalTarget::Workflow => match cancel_workflow_approval(
            &server.approval_store,
            &server.db,
            &approval_id,
            &lookup.subject_id,
        )
        .await
        {
            Ok(()) => {
                if let Some(consumed) = server.approval_store.get(&approval_id) {
                    record_approval_resolved(&server, &consumed);
                }
                Json(serde_json::json!({
                    "ok": true,
                    "approval_id": approval_id,
                    "status": "cancelled"
                }))
            }
            Err(error) => Json(serde_json::json!({"ok": false, "error": error})),
        },
        ApprovalTarget::InvalidBinding => {
            Json(serde_json::json!({"ok": false, "error": "审批绑定不完整"}))
        }
        ApprovalTarget::TaskAgent => {
            match cancel_task_agent_approval(&server, &approval_id, &lookup).await {
                Ok(()) => {
                    if let Some(consumed) = server.approval_store.get(&approval_id) {
                        record_approval_resolved(&server, &consumed);
                    }
                    Json(serde_json::json!({
                        "ok": true,
                        "approval_id": approval_id,
                        "status": "cancelled"
                    }))
                }
                Err(error) => Json(serde_json::json!({"ok": false, "error": error})),
            }
        }
        ApprovalTarget::Agent => {
            let conv_id = body
                .conversation_id
                .clone()
                .unwrap_or_else(|| lookup.conversation_id.clone());
            match server.approval_store.cancel(&approval_id, &conv_id) {
                Ok(a) => {
                    record_approval_resolved(&server, &a);
                    // Keep the conversation chain valid: record the op never ran.
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
) -> ApprovalStream {
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

    box_stream(stream)
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
        approve_handler, cancel_handler, classify_approval, execute_approved_tool, reject_handler,
        resolve_and_consume, workflow_approval_stream, ApprovalDecisionRequest, ApprovalTarget,
    };
    use async_trait::async_trait;
    use axum::{
        body::to_bytes,
        extract::{Path, State},
        http::StatusCode,
        response::IntoResponse,
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
        execution::{ExecutionContext, ExecutionId},
        safety::execution_gateway::SecurityExecutionOutcome,
        safety::ControlSession,
        server::AppServer,
        tools::{trait_def::RiskLevel, Tool, ToolRegistry, ToolResult},
        workflow::{
            NodeRunStatus, WorkflowGraphDefinition, WorkflowNodeConfig, WorkflowNodeDefinition,
            WorkflowNodeId, WorkflowNodeKind, WorkflowRun, WorkflowRunId, WorkflowRunStatus,
            WORKFLOW_GRAPH_SCHEMA_VERSION,
        },
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

    // ── Workflow approval HTTP bridge tests ──

    fn plain_approval() -> crate::safety::PendingApproval {
        crate::safety::PendingApproval {
            approval_id: "a".to_string(),
            conversation_id: "c".to_string(),
            tool_call_id: "t".to_string(),
            tool_name: "bash".to_string(),
            arguments: serde_json::json!({}),
            risk_level: RiskLevel::High,
            reason: "r".to_string(),
            status: crate::safety::ApprovalStatus::Pending,
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now(),
            subject_id: "local-user".to_string(),
            execution_id: None,
            workflow_run_id: None,
            workflow_node_id: None,
            task_id: None,
            task_execution_id: None,
            agent_execution_id: None,
        }
    }

    #[test]
    fn classify_plain_approval_as_agent() {
        assert!(matches!(
            classify_approval(&plain_approval()),
            ApprovalTarget::Agent
        ));
    }

    #[test]
    fn classify_full_binding_as_workflow() {
        let approval = crate::safety::PendingApproval {
            execution_id: Some("e".into()),
            workflow_run_id: Some("r".into()),
            workflow_node_id: Some("n".into()),
            ..plain_approval()
        };
        assert!(matches!(
            classify_approval(&approval),
            ApprovalTarget::Workflow
        ));
    }

    #[test]
    fn classify_partial_binding_as_invalid() {
        let approval = crate::safety::PendingApproval {
            execution_id: Some("e".into()),
            workflow_run_id: Some("r".into()),
            workflow_node_id: None,
            ..plain_approval()
        };
        assert!(matches!(
            classify_approval(&approval),
            ApprovalTarget::InvalidBinding
        ));
    }

    fn bash_graph() -> WorkflowGraphDefinition {
        WorkflowGraphDefinition {
            schema_version: WORKFLOW_GRAPH_SCHEMA_VERSION,
            entry_node_id: WorkflowNodeId::new("bash").unwrap(),
            nodes: vec![WorkflowNodeDefinition {
                id: WorkflowNodeId::new("bash").unwrap(),
                kind: WorkflowNodeKind::Tool,
                config: WorkflowNodeConfig::Tool {
                    tool_name: "bash".to_string(),
                    arguments: serde_json::json!({"command": "echo test"}),
                },
            }],
            edges: vec![],
        }
    }

    /// Create a persisted run paused in WaitingApproval, plus a workflow-bound
    /// approval for it. Returns (run_id, approval_id).
    fn paused_workflow(server: &AppServer) -> (String, String) {
        let now = chrono::Utc::now().timestamp_millis();
        let ctx = ExecutionContext::new(
            ExecutionId::generate(),
            "local-user",
            "workflow-runner",
            None,
            now,
        );
        let mut run = WorkflowRun::new(WorkflowRunId::generate(), ctx, bash_graph(), now).unwrap();
        let bash = WorkflowNodeId::new("bash").unwrap();
        run.transition_node(&bash, NodeRunStatus::Running, now)
            .unwrap();
        run.transition_node(&bash, NodeRunStatus::WaitingApproval, now)
            .unwrap();
        let run_id = run.run_id.to_string();
        server.db.create_workflow_run("g1", &run).unwrap();
        server.db.update_workflow_run("g1", &run).unwrap();

        let approval = server.approval_store.create_workflow(
            run.execution_context.execution_id.to_string(),
            run.run_id.to_string(),
            "bash".to_string(),
            "tool-call-1".to_string(),
            "bash".to_string(),
            serde_json::json!({"command": "echo test"}),
            RiskLevel::High,
            "high-risk".to_string(),
            "local-user".to_string(),
        );
        (run_id, approval.approval_id)
    }

    /// Consume a real handler `Sse` response to completion and return its body
    /// text. This drives the exact stream the handler returns.
    async fn consume_sse(sse: axum::response::Sse<super::ApprovalStream>) -> String {
        let response = sse.into_response();
        let body = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        String::from_utf8(body.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn workflow_approve_handler_resumes_run() {
        let (_temp, server) = test_server("wf-approve");
        let (run_id, approval_id) = paused_workflow(&server);

        // Consume the REAL SSE response the handler returns.
        let sse = approve_handler(
            State(server.clone()),
            Path(approval_id.clone()),
            Json(ApprovalDecisionRequest {
                conversation_id: None,
            }),
        )
        .await
        .unwrap();
        let text = consume_sse(sse).await;

        let stored = server
            .db
            .get_workflow_run(&WorkflowRunId::new(run_id).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(stored.run.status, WorkflowRunStatus::Completed);
        assert_eq!(
            server
                .approval_store
                .get(&approval_id)
                .unwrap()
                .status
                .to_string(),
            "approved"
        );
        assert!(text.contains("approval_resolved"));
        assert!(text.contains("workflow_run_updated"));
    }

    #[tokio::test]
    async fn workflow_reject_handler_fails_run() {
        let (_temp, server) = test_server("wf-reject");
        let (run_id, approval_id) = paused_workflow(&server);

        let sse = reject_handler(
            State(server.clone()),
            Path(approval_id.clone()),
            Json(ApprovalDecisionRequest {
                conversation_id: None,
            }),
        )
        .await
        .unwrap();
        let text = consume_sse(sse).await;

        let stored = server
            .db
            .get_workflow_run(&WorkflowRunId::new(run_id).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(stored.run.status, WorkflowRunStatus::Failed);
        assert_eq!(
            server
                .approval_store
                .get(&approval_id)
                .unwrap()
                .status
                .to_string(),
            "rejected"
        );
        assert!(text.contains("workflow_run_updated"));
    }

    #[tokio::test]
    async fn workflow_cancel_handler_cancels_run() {
        let (_temp, server) = test_server("wf-cancel");
        let (run_id, approval_id) = paused_workflow(&server);

        let Json(response) = cancel_handler(
            State(server.clone()),
            Path(approval_id.clone()),
            Json(ApprovalDecisionRequest {
                conversation_id: None,
            }),
        )
        .await;
        assert_eq!(response["ok"], true);

        let stored = server
            .db
            .get_workflow_run(&WorkflowRunId::new(run_id).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(stored.run.status, WorkflowRunStatus::Cancelled);
        assert_eq!(
            server
                .approval_store
                .get(&approval_id)
                .unwrap()
                .status
                .to_string(),
            "cancelled"
        );
    }

    #[tokio::test]
    async fn workflow_approval_replay_is_rejected() {
        let (_temp, server) = test_server("wf-replay");
        let (run_id, approval_id) = paused_workflow(&server);

        // First real approve succeeds.
        let sse = approve_handler(
            State(server.clone()),
            Path(approval_id.clone()),
            Json(ApprovalDecisionRequest {
                conversation_id: None,
            }),
        )
        .await
        .unwrap();
        consume_sse(sse).await;

        // Second real approve fails fast at the handler with CONFLICT — never a
        // surface-normal SSE. No re-execution; the run stays completed.
        let replay = approve_handler(
            State(server.clone()),
            Path(approval_id.clone()),
            Json(ApprovalDecisionRequest {
                conversation_id: None,
            }),
        )
        .await;
        assert!(replay.is_err());
        assert_eq!(replay.unwrap_err().0, StatusCode::CONFLICT);

        let stored = server
            .db
            .get_workflow_run(&WorkflowRunId::new(run_id).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(stored.run.status, WorkflowRunStatus::Completed);
        assert_eq!(
            server
                .approval_store
                .get(&approval_id)
                .unwrap()
                .status
                .to_string(),
            "approved"
        );
    }

    #[tokio::test]
    async fn workflow_partial_binding_fails_closed() {
        let (_temp, server) = test_server("wf-partial");
        let mut approval = plain_approval();
        approval.approval_id = "partial-1".to_string();
        approval.execution_id = Some("e".to_string());
        approval.workflow_run_id = Some("r".to_string());
        approval.workflow_node_id = None;
        server.approval_store.insert_for_test(approval);

        let result = approve_handler(
            State(server.clone()),
            Path("partial-1".to_string()),
            Json(ApprovalDecisionRequest {
                conversation_id: None,
            }),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn workflow_approval_resolved_event_only_after_success() {
        let (_temp, server) = test_server("wf-order");
        let (_run_id, approval_id) = paused_workflow(&server);

        let sse = approve_handler(
            State(server.clone()),
            Path(approval_id.clone()),
            Json(ApprovalDecisionRequest {
                conversation_id: None,
            }),
        )
        .await
        .unwrap();
        let text = consume_sse(sse).await;

        let resolved_at = text
            .find("approval_resolved")
            .expect("approval_resolved emitted");
        let updated_at = text
            .find("workflow_run_updated")
            .expect("workflow_run_updated emitted");
        assert!(
            resolved_at < updated_at,
            "approval_resolved must precede workflow_run_updated"
        );
        assert_eq!(
            server
                .approval_store
                .get(&approval_id)
                .unwrap()
                .status
                .to_string(),
            "approved"
        );
    }

    #[tokio::test]
    async fn workflow_failed_resolution_does_not_emit_resolved_or_duplicate_audit() {
        let (_temp, server) = test_server("wf-failres");
        let (_run_id, approval_id) = paused_workflow(&server);

        // Complete one real approve: exactly one ApprovalResolved audit.
        let sse = approve_handler(
            State(server.clone()),
            Path(approval_id.clone()),
            Json(ApprovalDecisionRequest {
                conversation_id: None,
            }),
        )
        .await
        .unwrap();
        consume_sse(sse).await;
        assert_eq!(resolution_events(&server).len(), 1);

        // Drive the stream again on the already-consumed approval → resolve
        // fails (AlreadyProcessed). No approval_resolved event, no new audit.
        let approval = server.approval_store.get(&approval_id).unwrap();
        let stream = workflow_approval_stream(server.clone(), approval, true);
        let text = consume_sse(axum::response::Sse::new(stream)).await;

        assert!(
            !text.contains("approval_resolved"),
            "failed resolution must not emit approval_resolved"
        );
        assert_eq!(
            resolution_events(&server).len(),
            1,
            "failed resolution must not add a duplicate approval_resolved audit"
        );
    }
}
