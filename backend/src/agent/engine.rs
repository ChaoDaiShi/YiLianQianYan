// ============================================================
// Agent Engine — Core ReAct loop (channel-based for HTTP server)
//
// Flow: prepare → think (LLM with tools) ↔ execute tools → respond
// ============================================================

use serde::Serialize;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use super::completion_guard::{evaluate_completion, CompletionCheck};
use super::state::AgentState;
use super::tool_loop::ToolLoopGuard;
use super::verifier::replan_message;
use crate::app::state::LogBuffer;
use crate::config::types::AppConfig;
use crate::integrations::llm::client::LlmClient;
use crate::integrations::llm::types::ToolCall;
use crate::safety::approval::ApprovalStore;
use crate::safety::execution_gateway::SecurityExecutionOutcome;
use crate::safety::{SecurityExecutionGateway, SecurityExecutionRequest, SecuritySubject};
use crate::tools::registry::ToolRegistry;
use crate::tools::trait_def::RiskLevel;
use crate::utils::text::truncate_chars;

/// Agent streaming event (shared with API layer)
#[derive(Debug, Clone, Serialize)]
#[serde(into = "serde_json::Value")]
pub struct AgentEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub conversation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub should_replan: Option<bool>,
}

impl From<AgentEvent> for serde_json::Value {
    fn from(event: AgentEvent) -> Self {
        if matches!(event.event_type.as_str(), "model_route" | "workspace_mode") {
            if let Some(mut route) = event.args.clone() {
                route["type"] = serde_json::json!(event.event_type);
                route["conversation_id"] = serde_json::json!(event.conversation_id);
                return route;
            }
        }
        let mut value = serde_json::json!({"type":event.event_type,"conversation_id":event.conversation_id,"token":event.token,"tool_call_id":event.tool_call_id,"tool_name":event.tool_name,"args":event.args,"result":event.result,"status":event.status,"error":event.error,"message_id":event.message_id,"risk_level":event.risk_level,"reason":event.reason,"approval_id":event.approval_id,"verification_success":event.verification_success,"verification_reason":event.verification_reason,"should_replan":event.should_replan});
        value
            .as_object_mut()
            .expect("event object")
            .retain(|_, value| !value.is_null());
        value
    }
}

impl AgentEvent {
    pub(crate) fn error_event(conversation_id: &str, error: String) -> Self {
        Self {
            event_type: "error".into(),
            conversation_id: conversation_id.into(),
            token: None,
            tool_call_id: None,
            tool_name: None,
            args: None,
            result: None,
            status: None,
            error: Some(error),
            message_id: None,
            risk_level: None,
            reason: None,
            approval_id: None,
            verification_success: None,
            verification_reason: None,
            should_replan: None,
        }
    }
}

/// Result of running the agent loop.
pub enum RunOutcome {
    /// The loop produced a final answer (the "done" event was already sent).
    Done { output: String },
    /// The loop paused awaiting user approval (approval_required was sent).
    Paused { approval_id: String },
    WaitingInput {
        tool_call_id: String,
        request: super::user_input::UserInputRequest,
    },
}

const MAX_ITERATIONS: usize = 20;
const MAX_COMPLETION_REPLANS: usize = 2;

fn record_reasoning_delta(
    tx: &Sender<AgentEvent>,
    conversation: &str,
    journal: &parking_lot::Mutex<String>,
    reasoning: &str,
) {
    journal.lock().push_str(reasoning);
    let mut event = AgentEvent::error_event(conversation, String::new());
    event.event_type = "reasoning".into();
    event.error = None;
    event.token = Some(reasoning.into());
    let _ = tx.try_send(event);
}
async fn flush_reasoning_snapshot(
    tx: &Sender<AgentEvent>,
    conversation: &str,
    journal: &parking_lot::Mutex<String>,
) {
    let complete = journal.lock().clone();
    if complete.is_empty() {
        return;
    }
    let mut event = AgentEvent::error_event(conversation, String::new());
    event.event_type = "reasoning_snapshot".into();
    event.error = None;
    event.token = Some(complete);
    let _ = tx.send(event).await;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FinalResponseGate {
    Complete,
    Replan,
}

fn gate_final_response(
    state: &mut AgentState,
    completion_replans: &mut usize,
) -> Result<FinalResponseGate, String> {
    match evaluate_completion(&state.messages) {
        CompletionCheck::Ready => Ok(FinalResponseGate::Complete),
        CompletionCheck::MissingDesktopTextInputEvidence => {
            if *completion_replans >= MAX_COMPLETION_REPLANS {
                return Err("无法确认文本已写入目标窗口，任务未完成".to_string());
            }
            *completion_replans += 1;
            state.add_system_message(
                "当前任务尚未执行已验证的目标窗口文本输入。不要声称任务完成；请调用 keyboard，使用 action=type、text 和 target_application 完成输入，或在无法执行时明确失败。"
                    .to_string(),
            );
            Ok(FinalResponseGate::Replan)
        }
    }
}

fn should_stream_response_tokens(state: &AgentState) -> bool {
    matches!(evaluate_completion(&state.messages), CompletionCheck::Ready)
}

fn finish_with_local_result(
    state: &mut AgentState,
    conversation_id: &str,
    tx: &Sender<AgentEvent>,
    output: String,
) -> RunOutcome {
    state.add_assistant_message(Some(output.clone()), None);
    state.output = output.clone();
    let _ = tx.try_send(AgentEvent {
        event_type: "done".into(),
        conversation_id: conversation_id.to_string(),
        token: None,
        tool_call_id: None,
        tool_name: None,
        args: None,
        result: None,
        status: None,
        error: None,
        message_id: Some(uuid::Uuid::new_v4().to_string()),
        risk_level: None,
        reason: None,
        approval_id: None,
        verification_success: None,
        verification_reason: None,
        should_replan: None,
    });
    RunOutcome::Done { output }
}

enum ToolDispatchOutcome {
    Continue,
    Replan,
    Paused { approval_id: String },
    Stopped { output: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExternalToolAccounting {
    NotExternal,
    Known,
    BillingUnknown,
    ResultUnknown,
}

/// Account external ACP tool results without treating context-window usage as
/// billed tokens. Approval resumption can call this same helper after dispatch.
pub(crate) fn record_external_tool_accounting(
    scope: Option<&crate::agent::delegation::ExecutionScope>,
    tool_name: &str,
    structured: Option<&serde_json::Value>,
) -> Result<ExternalToolAccounting, String> {
    if !tool_name.starts_with("subagent_acp_") || !tool_name.ends_with("_run") {
        return Ok(ExternalToolAccounting::NotExternal);
    }

    let result_unknown = structured
        .and_then(|value| value.get("state"))
        .and_then(serde_json::Value::as_str)
        == Some("result_unknown");
    let billing_known = structured
        .and_then(|value| value.get("model_calls"))
        .and_then(serde_json::Value::as_u64)
        .is_some()
        && structured
            .and_then(|value| value.get("total_tokens"))
            .and_then(serde_json::Value::as_u64)
            .is_some();
    let disposition = if result_unknown {
        ExternalToolAccounting::ResultUnknown
    } else if billing_known {
        ExternalToolAccounting::Known
    } else {
        ExternalToolAccounting::BillingUnknown
    };

    let strict = scope.is_some_and(|scope| scope.goal_id.is_some() || scope.handoff_id.is_some());
    if strict && disposition != ExternalToolAccounting::Known {
        let scope = scope.ok_or_else(|| "proofloop_usage_unavailable".to_string())?;
        let budget = scope
            .budget
            .as_ref()
            .ok_or_else(|| "proofloop_usage_unavailable".to_string())?;
        budget.mark_usage_unavailable(scope.handoff_id.as_deref())?;
    }
    Ok(disposition)
}

async fn dispatch_tool_call(
    state: &mut AgentState,
    loop_guard: &mut ToolLoopGuard,
    security_gateway: &SecurityExecutionGateway,
    approval_store: &ApprovalStore,
    conversation_id: &str,
    tool_call: &ToolCall,
    args: &serde_json::Value,
    tx: &Sender<AgentEvent>,
    log_buffer: &LogBuffer,
) -> Result<ToolDispatchOutcome, String> {
    let request = SecurityExecutionRequest {
        conversation_id: conversation_id.to_string(),
        tool_call_id: tool_call.id.clone(),
        tool_name: tool_call.function.name.clone(),
        arguments: args.clone(),
        subject: SecuritySubject::local_user(),
    };
    let execution_started = std::sync::atomic::AtomicBool::new(false);

    let outcome = security_gateway
        .execute_with_on_start(&request, RiskLevel::Low, || {
            execution_started.store(true, std::sync::atomic::Ordering::SeqCst);
            state.mark_tool_started(&tool_call.id);
            let _ = tx.try_send(AgentEvent {
                event_type: "tool_start".into(),
                conversation_id: conversation_id.to_string(),
                token: None,
                tool_call_id: Some(tool_call.id.clone()),
                tool_name: Some(tool_call.function.name.clone()),
                args: Some(args.clone()),
                result: None,
                status: None,
                error: None,
                message_id: None,
                risk_level: None,
                reason: None,
                approval_id: None,
                verification_success: None,
                verification_reason: None,
                should_replan: None,
            });
            log_buffer.push("tool", "tool", &format!("▶ {}", tool_call.function.name));
        })
        .await;

    match outcome {
        Ok(SecurityExecutionOutcome::Executed {
            tool_result,
            verification,
        }) => {
            let status = if tool_result.ok { "success" } else { "error" };
            let log_level = if tool_result.ok { "tool" } else { "error" };
            let result_preview = truncate_chars(&tool_result.content, 80);
            log_buffer.push(
                log_level,
                "tool",
                &format!(
                    "{} {} — {}",
                    if tool_result.ok { "✓" } else { "✗" },
                    tool_call.function.name,
                    result_preview
                ),
            );

            let _ = tx.try_send(AgentEvent {
                event_type: "tool_end".into(),
                conversation_id: conversation_id.to_string(),
                token: None,
                tool_call_id: Some(tool_call.id.clone()),
                tool_name: Some(tool_call.function.name.clone()),
                args: None,
                result: Some(tool_result.content.clone()),
                status: Some(status.to_string()),
                error: None,
                message_id: None,
                risk_level: None,
                reason: None,
                approval_id: None,
                verification_success: None,
                verification_reason: None,
                should_replan: None,
            });

            let scope = super::delegation::current_scope();
            let strict_goal_scope = scope
                .as_ref()
                .is_some_and(|scope| scope.goal_id.is_some() || scope.handoff_id.is_some());
            let external_accounting = match record_external_tool_accounting(
                scope.as_ref(),
                &tool_call.function.name,
                tool_result.structured.as_ref(),
            ) {
                Ok(value) => value,
                Err(error) => {
                    state.add_tool_result(
                        tool_call.id.clone(),
                        tool_call.function.name.clone(),
                        format!(
                            "{}\n\n{}",
                            tool_result.content,
                            "ACP 计费 token 用量不可核实，目标预算已停止后续自动执行。"
                        ),
                    );
                    return Err(error);
                }
            };
            let result_unknown = external_accounting == ExternalToolAccounting::ResultUnknown;
            let strict_billing_unknown =
                strict_goal_scope && external_accounting == ExternalToolAccounting::BillingUnknown;
            if result_unknown || strict_billing_unknown {
                let output = if result_unknown {
                    format!(
                        "{}\n\n外部 ACP 执行结果未知。为避免重复提交，已停止自动重试；请先核实外部会话结果。",
                        tool_result.content
                    )
                } else {
                    format!(
                        "{}\n\n外部 ACP 没有提供可计费 token 用量。目标预算现为不可用，已停止后续自动模型和工具调用。",
                        tool_result.content
                    )
                };
                state.add_tool_result(
                    tool_call.id.clone(),
                    tool_call.function.name.clone(),
                    output.clone(),
                );
                let reason = if result_unknown {
                    "ACP结果未知，自动重试已停止".to_string()
                } else {
                    format!(
                        "{}；ACP计费用量未知，目标预算不可用，自动后续调用已停止",
                        verification.reason
                    )
                };
                let _ = tx.try_send(AgentEvent {
                    event_type: "verification".into(),
                    conversation_id: conversation_id.to_string(),
                    token: None,
                    tool_call_id: Some(tool_call.id.clone()),
                    tool_name: Some(tool_call.function.name.clone()),
                    args: None,
                    result: None,
                    status: None,
                    error: None,
                    message_id: None,
                    risk_level: None,
                    reason: None,
                    approval_id: None,
                    verification_success: Some(if result_unknown {
                        false
                    } else {
                        verification.success
                    }),
                    verification_reason: Some(reason),
                    should_replan: Some(false),
                });
                return Ok(ToolDispatchOutcome::Stopped { output });
            }

            let _ = tx.try_send(AgentEvent {
                event_type: "verification".into(),
                conversation_id: conversation_id.to_string(),
                token: None,
                tool_call_id: Some(tool_call.id.clone()),
                tool_name: Some(tool_call.function.name.clone()),
                args: None,
                result: None,
                status: None,
                error: None,
                message_id: None,
                risk_level: None,
                reason: None,
                approval_id: None,
                verification_success: Some(verification.success),
                verification_reason: Some(verification.reason.clone()),
                should_replan: Some(verification.should_replan),
            });

            log_buffer.push(
                if verification.success { "tool" } else { "warn" },
                "verify",
                &format!(
                    "[VERIFY] {} tool={} reason={}",
                    if verification.success {
                        "success"
                    } else {
                        "failed"
                    },
                    tool_call.function.name,
                    verification.reason
                ),
            );

            let failure = (!tool_result.ok || !verification.success).then(|| {
                serde_json::json!({
                    "tool_ok": tool_result.ok,
                    "output": tool_result.content,
                    "verification_success": verification.success,
                    "verification_reason": verification.reason,
                    "observation": verification.observation,
                })
            });
            if verification.should_replan {
                state.add_tool_result(
                    tool_call.id.clone(),
                    tool_call.function.name.clone(),
                    replan_message(&tool_call.function.name, &verification.reason),
                );
                loop_guard.observe(&tool_call.function.name, args, failure)?;
                return Ok(ToolDispatchOutcome::Replan);
            }

            state.add_tool_result(
                tool_call.id.clone(),
                tool_call.function.name.clone(),
                summarize_tool_result(&if external_accounting
                    == ExternalToolAccounting::BillingUnknown
                {
                    format!(
                        "{}\n\nACP v1 未提供累计计费 token 用量，外部模型成本未知。",
                        tool_result.content
                    )
                } else {
                    tool_result.content.clone()
                }),
            );
            loop_guard.observe(&tool_call.function.name, args, failure)?;
            Ok(ToolDispatchOutcome::Continue)
        }
        Ok(SecurityExecutionOutcome::RequiresApproval { risk_level, reason }) => {
            let (approval, created) = approval_store.create_or_get_pending(
                conversation_id.to_string(),
                tool_call.id.clone(),
                tool_call.function.name.clone(),
                args.clone(),
                risk_level,
                reason,
                SecuritySubject::local_user().subject_id,
            );
            if !created {
                state.add_tool_result(
                    tool_call.id.clone(),
                    tool_call.function.name.clone(),
                    format!(
                        "Tool {} was skipped because another approval is already pending.",
                        tool_call.function.name
                    ),
                );
            }

            let _ = tx.try_send(AgentEvent {
                event_type: "approval_required".into(),
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
                reason: Some(approval.reason.clone()),
                approval_id: Some(approval.approval_id.clone()),
                verification_success: None,
                verification_reason: None,
                should_replan: None,
            });
            log_buffer.push(
                "warn",
                "safety",
                &format!(
                    "⚠ {} waiting for approval — {}",
                    approval.tool_name, approval.reason
                ),
            );

            Ok(ToolDispatchOutcome::Paused {
                approval_id: approval.approval_id,
            })
        }
        Ok(SecurityExecutionOutcome::Denied { reason }) => {
            state.add_tool_result(
                tool_call.id.clone(),
                tool_call.function.name.clone(),
                format!(
                    "Tool {} was denied by the security gateway: {}",
                    tool_call.function.name, reason
                ),
            );
            loop_guard.observe(
                &tool_call.function.name,
                args,
                Some(serde_json::json!({"denied": reason})),
            )?;
            Ok(ToolDispatchOutcome::Continue)
        }
        Err(error) => {
            if execution_started.load(std::sync::atomic::Ordering::SeqCst) {
                let _ = tx.try_send(AgentEvent {
                    event_type: "tool_end".into(),
                    conversation_id: conversation_id.to_string(),
                    token: None,
                    tool_call_id: Some(tool_call.id.clone()),
                    tool_name: Some(tool_call.function.name.clone()),
                    args: None,
                    result: Some(error.to_string()),
                    status: Some("error".to_string()),
                    error: None,
                    message_id: None,
                    risk_level: None,
                    reason: None,
                    approval_id: None,
                    verification_success: None,
                    verification_reason: None,
                    should_replan: None,
                });
            }
            log_buffer.push(
                "error",
                "safety",
                &format!(
                    "Security gateway failed closed for {}: {error}",
                    tool_call.function.name
                ),
            );
            state.add_tool_result(
                tool_call.id.clone(),
                tool_call.function.name.clone(),
                format!(
                    "Tool {} was not executed because the security gateway failed closed: {}",
                    tool_call.function.name, error
                ),
            );
            loop_guard.observe(
                &tool_call.function.name,
                args,
                Some(serde_json::json!({"gateway_error": error.to_string()})),
            )?;
            Ok(ToolDispatchOutcome::Continue)
        }
    }
}

/// Trim large binary payloads from tool results before sending to the LLM.
/// The full result is still delivered to the frontend via SSE.
pub(crate) fn summarize_tool_result(content: &str) -> String {
    // Data URIs (e.g. screenshots) — keep only the metadata line after the URI
    if content.starts_with("data:image/") {
        let summary: Vec<&str> = content.split('\n').skip(1).collect();
        if summary.is_empty() {
            return "截图已完成，图片已展示在界面中。请直接描述你看到的截图内容回复用户。"
                .to_string();
        }
        return format!(
            "截图已完成，图片已展示在界面中。截图信息: {}。请直接回复用户，不要再次调用截图工具。",
            summary.join("\n")
        );
    }

    // General: truncate extremely long results to avoid flooding LLM context.
    // Use char boundary to avoid UTF-8 panic.
    if content.chars().count() > 8000 {
        let truncated = truncate_chars(content, 8000);
        return format!("{}\n(输出已截断，完整内容已展示在界面中)", truncated);
    }

    content.to_string()
}

/// Run the ReAct agent loop, sending streaming events through a channel.
///
/// Returns `RunOutcome::Paused` when a high-risk tool call requires user
/// approval; the caller must persist state and wait for a decision before
/// resuming.
pub async fn run_react_loop_with_channel(
    state: &mut AgentState,
    client: &LlmClient,
    tool_registry: &ToolRegistry,
    approval_store: &ApprovalStore,
    security_gateway: &SecurityExecutionGateway,
    _config: &AppConfig,
    conversation_id: &str,
    cancel_token: &CancellationToken,
    tx: &Sender<AgentEvent>,
    log_buffer: &LogBuffer,
) -> Result<RunOutcome, String> {
    super::state::tool_protocol::validate(&state.messages)?;
    let inherited = super::delegation::current_scope();
    let scope = super::delegation::ExecutionScope {
        cancel: cancel_token.clone(),
        budget: inherited
            .as_ref()
            .and_then(|v| v.budget.clone())
            .or_else(|| {
                Some(crate::modules::task::resource_budget::SharedExecutionBudget::new(6, 24_000))
            }),
        handoff_id: inherited.as_ref().and_then(|v| v.handoff_id.clone()),
        goal_id: inherited.as_ref().and_then(|v| v.goal_id.clone()),
        goal_version: inherited.as_ref().and_then(|v| v.goal_version),
        evidence_ids: inherited
            .as_ref()
            .map(|v| v.evidence_ids.clone())
            .unwrap_or_default(),
        deadline: inherited.and_then(|v| v.deadline),
    };
    let result = super::delegation::EXECUTION_SCOPE
        .scope(
            scope,
            run_react_loop_inner(
                state,
                client,
                tool_registry,
                approval_store,
                security_gateway,
                _config,
                conversation_id,
                cancel_token,
                tx,
                log_buffer,
            ),
        )
        .await;
    if result.is_err() {
        state.close_interrupted_tool_batch();
    }
    result
}

async fn run_react_loop_inner(
    state: &mut AgentState,
    client: &LlmClient,
    tool_registry: &ToolRegistry,
    approval_store: &ApprovalStore,
    security_gateway: &SecurityExecutionGateway,
    _config: &AppConfig,
    conversation_id: &str,
    cancel_token: &CancellationToken,
    tx: &Sender<AgentEvent>,
    log_buffer: &LogBuffer,
) -> Result<RunOutcome, String> {
    let tools_openai = tool_registry.to_openai_tools();
    let mut iteration = 0;
    let mut loop_guard = ToolLoopGuard::default();
    let mut completion_replans = 0usize;
    let reasoning_journal = parking_lot::Mutex::new(String::new());
    let mut compacted_context = state.compaction_checkpoint.clone();

    loop {
        if cancel_token.is_cancelled() {
            return Err("已取消".to_string());
        }
        if super::delegation::current_scope()
            .and_then(|scope| scope.deadline)
            .is_some_and(|deadline| tokio::time::Instant::now() >= deadline)
        {
            cancel_token.cancel();
            return Err("执行超时".into());
        }

        iteration += 1;
        if iteration > MAX_ITERATIONS {
            return Err(format!(
                "已达到本轮 {MAX_ITERATIONS} 轮执行上限，任务尚未完成"
            ));
        }

        // ── THINK: Call LLM with streaming ──
        let model_messages = super::context_compaction::project(
            &state.messages,
            &tools_openai,
            &_config.compaction,
            client,
            cancel_token,
            &mut compacted_context,
        )
        .await?;
        state.compaction_checkpoint = compacted_context.clone();
        let stream_response_tokens = should_stream_response_tokens(state);
        let execution_scope = super::delegation::current_scope();
        let strict_budget = execution_scope
            .as_ref()
            .is_some_and(|scope| scope.handoff_id.is_some() || scope.goal_id.is_some());
        let strict_output_limit = if strict_budget {
            execution_scope
                .as_ref()
                .and_then(|scope| scope.budget.as_ref().map(|budget| (scope, budget)))
                .map(|(scope, budget)| {
                    let input_upper_bound =
                        crate::integrations::llm::client::conservative_input_token_upper_bound(
                            &model_messages,
                            &tools_openai,
                        )
                        .ok_or_else(|| "proofloop_usage_unavailable".to_string())?;
                    budget.output_token_limit(
                        scope.handoff_id.as_deref(),
                        input_upper_bound,
                        client.max_output_tokens(),
                    )
                })
                .transpose()?
        } else {
            None
        };
        if let Some(scope) = &execution_scope {
            if strict_budget {
                if let Some(budget) = &scope.budget {
                    budget.begin_call(scope.handoff_id.as_deref())?;
                }
            }
        }
        let on_token = |token: &str| {
            if !stream_response_tokens {
                return;
            }
            let _ = tx.try_send(AgentEvent {
                event_type: "token".into(),
                conversation_id: conversation_id.to_string(),
                token: Some(token.to_string()),
                tool_call_id: None,
                tool_name: None,
                args: None,
                result: None,
                status: None,
                error: None,
                message_id: None,
                risk_level: None,
                reason: None,
                approval_id: None,
                verification_success: None,
                verification_reason: None,
                should_replan: None,
            });
        };
        let on_reasoning = |reasoning: &str| {
            record_reasoning_delta(tx, conversation_id, &reasoning_journal, reasoning);
        };
        let invocation = client.stream_with_reasoning_max_tokens(
            &model_messages,
            &tools_openai,
            strict_output_limit.unwrap_or_else(|| client.max_output_tokens()),
            on_token,
            on_reasoning,
        );
        let result = if let Some(deadline) =
            execution_scope.as_ref().and_then(|scope| scope.deadline)
        {
            tokio::select! {
                biased;
                _=cancel_token.cancelled()=>{
                    flush_reasoning_snapshot(tx, conversation_id, &reasoning_journal).await;
                    if let Some(scope)=&execution_scope {if let Some(budget)=&scope.budget{if strict_budget{let _=budget.settle_tokens(scope.handoff_id.as_deref(),None);}else{let _=budget.observe_parent_usage(None);}}}
                    return Err("已取消".into());
                },
                _=tokio::time::sleep_until(deadline)=>{
                    flush_reasoning_snapshot(tx, conversation_id, &reasoning_journal).await;
                    if let Some(scope)=&execution_scope {if let Some(budget)=&scope.budget{if strict_budget{let _=budget.settle_tokens(scope.handoff_id.as_deref(),None);}else{let _=budget.observe_parent_usage(None);}}}
                    cancel_token.cancel();
                    return Err("执行超时".into());
                },
                response=invocation=>response,
            }
        } else {
            tokio::select! {
                biased;
                _=cancel_token.cancelled()=>{
                    flush_reasoning_snapshot(tx, conversation_id, &reasoning_journal).await;
                    if let Some(scope)=&execution_scope {if let Some(budget)=&scope.budget{if strict_budget{let _=budget.settle_tokens(scope.handoff_id.as_deref(),None);}else{let _=budget.observe_parent_usage(None);}}}
                    return Err("已取消".into());
                },
                response=invocation=>response,
            }
        };
        flush_reasoning_snapshot(tx, conversation_id, &reasoning_journal).await;
        if let Some(scope) = &execution_scope {
            if let Some(budget) = &scope.budget {
                let tokens = result
                    .as_ref()
                    .ok()
                    .and_then(|v| v.usage.as_ref())
                    .map(|v| u64::from(v.total_tokens));
                if strict_budget {
                    budget.settle_tokens(scope.handoff_id.as_deref(), tokens)?;
                } else {
                    budget.observe_parent_usage(tokens)?;
                }
            }
        }

        let accumulator = match result {
            Ok(acc) => acc,
            Err(e) => return Err(format!("LLM调用失败: {}", e)),
        };

        // ── Check for tool calls ──
        if accumulator.has_tool_calls() {
            if let Some(tool_calls) = accumulator.to_tool_calls() {
                let ids = tool_calls
                    .iter()
                    .map(|call| call.id.as_str())
                    .collect::<std::collections::HashSet<_>>();
                if ids.len() != tool_calls.len() || ids.contains("") {
                    return Err("模型返回重复或空工具调用ID，本轮工具未执行".into());
                }
                state.started_tool_calls.clear();
                state.add_assistant_message(
                    if accumulator.content.is_empty() {
                        None
                    } else {
                        Some(accumulator.content.clone())
                    },
                    Some(tool_calls.clone()),
                );
                if let Some(message) = state.messages.last_mut() {
                    message.reasoning_content = Some(accumulator.reasoning_content.clone());
                }

                // A necessary-input question owns the entire round, even when listed last.
                if let Some(question) = tool_calls
                    .iter()
                    .find(|call| call.function.name == "ask_user")
                {
                    let parsed = serde_json::from_str::<super::user_input::UserInputRequest>(
                        &question.function.arguments,
                    )
                    .map_err(|e| e.to_string())
                    .and_then(|request| {
                        request.validate()?;
                        Ok(request)
                    });
                    for call in tool_calls.iter().filter(|call| call.id != question.id) {
                        state.add_tool_result(call.id.clone(),call.function.name.clone(),serde_json::json!({"status":"not_executed","reason":"waiting_for_user_input"}).to_string());
                        let mut event = AgentEvent::error_event(conversation_id, String::new());
                        event.event_type = "tool_end".into();
                        event.error = None;
                        event.tool_call_id = Some(call.id.clone());
                        event.tool_name = Some(call.function.name.clone());
                        event.status = Some("not_executed".into());
                        event.result = Some("等待必要输入，本轮未执行".into());
                        let _ = tx.send(event).await;
                    }
                    match parsed {
                        Ok(request) => {
                            return Ok(RunOutcome::WaitingInput {
                                tool_call_id: question.id.clone(),
                                request,
                            })
                        }
                        Err(error) => {
                            state.add_tool_result(
                                question.id.clone(),
                                "ask_user".into(),
                                format!("Invalid question: {error}"),
                            );
                            continue;
                        }
                    }
                }

                for (i, tc) in tool_calls.iter().enumerate() {
                    if cancel_token.is_cancelled() {
                        return Err("已取消".to_string());
                    }

                    let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(serde_json::Value::Null);

                    let dispatch = dispatch_tool_call(
                        state,
                        &mut loop_guard,
                        security_gateway,
                        approval_store,
                        conversation_id,
                        tc,
                        &args,
                        tx,
                        log_buffer,
                    );
                    let dispatch_outcome = if let Some(deadline) =
                        execution_scope.as_ref().and_then(|scope| scope.deadline)
                    {
                        tokio::select! {
                            biased;
                            _=cancel_token.cancelled()=>return Err("已取消".into()),
                            _=tokio::time::sleep_until(deadline)=>{cancel_token.cancel();return Err("执行超时".into());},
                            result=dispatch=>result?,
                        }
                    } else {
                        tokio::select! {
                            biased;
                            _=cancel_token.cancelled()=>return Err("已取消".into()),
                            result=dispatch=>result?,
                        }
                    };
                    match dispatch_outcome {
                        ToolDispatchOutcome::Continue => {}
                        ToolDispatchOutcome::Replan => {
                            for later in tool_calls.iter().skip(i + 1) {
                                let skipped = format!(
                                    "工具 {} 的调用因验证失败被跳过，未执行。",
                                    later.function.name
                                );
                                state.add_tool_result(
                                    later.id.clone(),
                                    later.function.name.clone(),
                                    skipped,
                                );
                            }
                            break;
                        }
                        ToolDispatchOutcome::Paused { approval_id } => {
                            for later in tool_calls.iter().skip(i + 1) {
                                let skipped = format!(
                                    "工具 {} 的调用因等待审批被跳过，未执行。",
                                    later.function.name
                                );
                                state.add_tool_result(
                                    later.id.clone(),
                                    later.function.name.clone(),
                                    skipped,
                                );
                            }
                            return Ok(RunOutcome::Paused { approval_id });
                        }
                        ToolDispatchOutcome::Stopped { output } => {
                            for later in tool_calls.iter().skip(i + 1) {
                                state.add_tool_result(
                                    later.id.clone(),
                                    later.function.name.clone(),
                                    format!(
                                        "工具 {} 因外部 ACP 结果或计费用量不确定而被跳过，未执行。",
                                        later.function.name
                                    ),
                                );
                            }
                            return Ok(finish_with_local_result(
                                state,
                                conversation_id,
                                tx,
                                output,
                            ));
                        }
                    }
                }
                continue;
            }
        }

        // ── RESPOND: Final answer ──
        if gate_final_response(state, &mut completion_replans)? == FinalResponseGate::Replan {
            continue;
        }
        let output = accumulator.content.clone();
        state.add_assistant_message(Some(output.clone()), None);
        if let Some(message) = state.messages.last_mut() {
            message.reasoning_content = Some(accumulator.reasoning_content.clone());
        }
        state.output = output.clone();

        let msg_id = uuid::Uuid::new_v4().to_string();
        let _ = tx.try_send(AgentEvent {
            event_type: "done".into(),
            conversation_id: conversation_id.to_string(),
            token: None,
            tool_call_id: None,
            tool_name: None,
            args: None,
            result: None,
            status: None,
            error: None,
            message_id: Some(msg_id),
            risk_level: None,
            reason: None,
            approval_id: None,
            verification_success: None,
            verification_reason: None,
            should_replan: None,
        });

        return Ok(RunOutcome::Done { output });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use async_trait::async_trait;
    use serde_json::json;
    use tokio::sync::mpsc;

    use super::{
        dispatch_tool_call, gate_final_response, record_external_tool_accounting,
        should_stream_response_tokens, AgentEvent, ExternalToolAccounting, FinalResponseGate,
        ToolDispatchOutcome, MAX_COMPLETION_REPLANS,
    };
    use crate::agent::state::AgentState;
    use crate::agent::verifier::{VerificationResult, Verifier};
    use crate::app::state::LogBuffer;
    use crate::config::types::{SandboxConfig, SandboxProfile};
    use crate::integrations::llm::types::{ToolCall, ToolCallFunction};
    use crate::safety::{ApprovalStore, SecurityExecutionGateway};
    use crate::tools::{Tool, ToolRegistry, ToolResult};

    struct CountingTool {
        name: &'static str,
        executions: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Tool for CountingTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "engine gateway test tool"
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type": "object"})
        }

        async fn execute(&self, _args: serde_json::Value) -> ToolResult {
            self.executions.fetch_add(1, Ordering::SeqCst);
            ToolResult::success("tool output")
        }
    }

    struct FixedVerifier {
        result: VerificationResult,
    }

    #[async_trait]
    impl Verifier for FixedVerifier {
        async fn verify(
            &self,
            _tool_name: &str,
            _args: &serde_json::Value,
            _tool_result: &ToolResult,
        ) -> VerificationResult {
            self.result.clone()
        }
    }

    fn gateway(
        tool_name: &'static str,
        profile: SandboxProfile,
        verification: VerificationResult,
    ) -> (
        SecurityExecutionGateway,
        Arc<AtomicUsize>,
        crate::db::Database,
    ) {
        let executions = Arc::new(AtomicUsize::new(0));
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(CountingTool {
            name: tool_name,
            executions: Arc::clone(&executions),
        }));
        let db_path =
            std::env::temp_dir().join(format!("yilian-engine-gateway-{}.db", uuid::Uuid::new_v4()));
        let db = crate::db::Database::new(&db_path).unwrap();
        let gateway = SecurityExecutionGateway::with_sandbox_registry_and_verifier(
            SandboxConfig {
                profile,
                writable_paths: Vec::new(),
                denied_write_paths: Vec::new(),
            },
            ".",
            Arc::new(registry),
            Arc::new(FixedVerifier {
                result: verification,
            }),
        )
        .with_db(std::sync::Arc::new(db.clone_connection()));
        (gateway, executions, db)
    }

    fn tool_call(name: &str, arguments: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "call-1".to_string(),
            call_type: "function".to_string(),
            function: ToolCallFunction {
                name: name.to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    fn events(receiver: &mut mpsc::Receiver<AgentEvent>) -> Vec<AgentEvent> {
        let mut events = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            events.push(event);
        }
        events
    }

    #[test]
    fn strict_acp_accounting_unknown_blocks_goal_budget_without_fabricated_usage() {
        let budget = crate::modules::task::resource_budget::SharedExecutionBudget::new(6, 24_000);
        let scope = crate::agent::delegation::ExecutionScope {
            cancel: tokio_util::sync::CancellationToken::new(),
            budget: Some(Arc::clone(&budget)),
            handoff_id: None,
            goal_id: Some("goal-1".into()),
            goal_version: Some(1),
            evidence_ids: Vec::new(),
            deadline: None,
        };
        let completed = json!({
            "provider_id": "external",
            "state": "completed",
            "model_calls": null,
            "total_tokens": null,
        });
        assert_eq!(
            record_external_tool_accounting(
                Some(&scope),
                "subagent_acp_external_run",
                Some(&completed),
            )
            .unwrap(),
            ExternalToolAccounting::BillingUnknown,
        );
        assert_eq!(
            budget.available().unwrap_err(),
            "proofloop_usage_unavailable"
        );

        let unknown_result =
            json!({"state":"result_unknown","model_calls":null,"total_tokens":null});
        assert_eq!(
            record_external_tool_accounting(
                Some(&scope),
                "subagent_acp_external_run",
                Some(&unknown_result),
            )
            .unwrap(),
            ExternalToolAccounting::ResultUnknown,
        );
        assert_eq!(
            record_external_tool_accounting(None, "write_file", Some(&completed)).unwrap(),
            ExternalToolAccounting::NotExternal,
        );
    }

    fn desktop_text_state() -> AgentState {
        let mut state = AgentState::new("system".to_string());
        state.add_user_message("打开记事本，然后在新的页面写入你好世界".to_string());
        state.add_assistant_message(
            None,
            Some(vec![ToolCall {
                id: "call-open".to_string(),
                call_type: "function".to_string(),
                function: ToolCallFunction {
                    name: "open_application".to_string(),
                    arguments: json!({"application": "Notepad"}).to_string(),
                },
            }]),
        );
        state.add_tool_result(
            "call-open".to_string(),
            "open_application".to_string(),
            "记事本窗口已显示在桌面前台".to_string(),
        );
        state
    }

    fn verified_desktop_text_state() -> AgentState {
        let mut state = desktop_text_state();
        state.add_assistant_message(
            None,
            Some(vec![ToolCall {
                id: "call-type".to_string(),
                call_type: "function".to_string(),
                function: ToolCallFunction {
                    name: "keyboard".to_string(),
                    arguments: json!({
                        "action": "type",
                        "text": "你好世界",
                        "target_application": "Notepad"
                    })
                    .to_string(),
                },
            }]),
        );
        state.add_tool_result(
            "call-type".to_string(),
            "keyboard".to_string(),
            format!(
                "文本已输入目标应用 Notepad；{}",
                crate::tools::input::VERIFIED_DESKTOP_TEXT_INPUT_MARKER
            ),
        );
        state
    }

    #[test]
    fn unverified_desktop_completion_is_replanned_then_fails_closed() {
        let mut state = desktop_text_state();
        let mut completion_replans = 0;

        for expected_replans in 1..=MAX_COMPLETION_REPLANS {
            assert_eq!(
                gate_final_response(&mut state, &mut completion_replans).unwrap(),
                FinalResponseGate::Replan
            );
            assert_eq!(completion_replans, expected_replans);
            assert_eq!(state.messages.last().unwrap().role, "system");
            assert!(state
                .messages
                .last()
                .unwrap()
                .content
                .as_deref()
                .is_some_and(|content| content.contains("尚未执行已验证的目标窗口文本输入")));
        }

        let error = gate_final_response(&mut state, &mut completion_replans).unwrap_err();
        assert_eq!(error, "无法确认文本已写入目标窗口，任务未完成");
    }

    #[test]
    fn unverified_desktop_completion_does_not_stream_a_false_success_answer() {
        assert!(!should_stream_response_tokens(&desktop_text_state()));
    }

    #[test]
    fn verified_desktop_completion_can_stream_and_finish() {
        let mut state = verified_desktop_text_state();
        let mut completion_replans = 0;

        assert!(should_stream_response_tokens(&state));
        assert_eq!(
            gate_final_response(&mut state, &mut completion_replans).unwrap(),
            FinalResponseGate::Complete
        );
        assert_eq!(completion_replans, 0);
    }

    #[tokio::test]
    async fn dispatch_low_risk_tool_executes_once_through_gateway() {
        let (gateway, executions, _db) = gateway(
            "read_file",
            SandboxProfile::ReadOnly,
            VerificationResult::success("verified", None),
        );
        let call = tool_call("read_file", json!({"path": "README.md"}));
        let args = json!({"path": "README.md"});
        let mut state = AgentState::new("system".to_string());
        let approvals = ApprovalStore::new();
        let (sender, mut receiver) = mpsc::channel(16);

        let outcome = dispatch_tool_call(
            &mut state,
            &mut super::ToolLoopGuard::default(),
            &gateway,
            &approvals,
            "conversation-1",
            &call,
            &args,
            &sender,
            &LogBuffer::new(16),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, ToolDispatchOutcome::Continue));
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert_eq!(
            state.messages.last().unwrap().content.as_deref(),
            Some("tool output")
        );
        let event_types = events(&mut receiver)
            .into_iter()
            .map(|event| event.event_type)
            .collect::<Vec<_>>();
        assert_eq!(event_types, vec!["tool_start", "tool_end", "verification"]);
    }

    #[tokio::test]
    async fn dispatch_high_risk_tool_pauses_without_execution() {
        let (gateway, executions, _db) = gateway(
            "bash",
            SandboxProfile::Open,
            VerificationResult::success("unused", None),
        );
        let call = tool_call("bash", json!({"command": "git push origin develop"}));
        let args = json!({"command": "git push origin develop"});
        let mut state = AgentState::new("system".to_string());
        let approvals = ApprovalStore::new();
        let (sender, mut receiver) = mpsc::channel(16);

        let outcome = dispatch_tool_call(
            &mut state,
            &mut super::ToolLoopGuard::default(),
            &gateway,
            &approvals,
            "conversation-1",
            &call,
            &args,
            &sender,
            &LogBuffer::new(16),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, ToolDispatchOutcome::Paused { .. }));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        let approval = approvals.pending_for("conversation-1").unwrap();
        assert_eq!(approval.tool_name, "bash");
        let emitted = events(&mut receiver);
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].event_type, "approval_required");
        assert_eq!(emitted[0].risk_level.as_deref(), Some("high"));
    }

    #[tokio::test]
    async fn dispatch_sandbox_deny_writes_reason_without_execution() {
        let (gateway, executions, _db) = gateway(
            "write_file",
            SandboxProfile::ReadOnly,
            VerificationResult::success("unused", None),
        );
        let call = tool_call(
            "write_file",
            json!({"path": "notes.txt", "content": "blocked"}),
        );
        let args = json!({"path": "notes.txt", "content": "blocked"});
        let mut state = AgentState::new("system".to_string());
        let approvals = ApprovalStore::new();
        let (sender, mut receiver) = mpsc::channel(16);

        let outcome = dispatch_tool_call(
            &mut state,
            &mut super::ToolLoopGuard::default(),
            &gateway,
            &approvals,
            "conversation-1",
            &call,
            &args,
            &sender,
            &LogBuffer::new(16),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, ToolDispatchOutcome::Continue));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        assert!(state
            .messages
            .last()
            .unwrap()
            .content
            .as_deref()
            .unwrap()
            .contains("sandbox denied"));
        assert!(events(&mut receiver).is_empty());
    }

    #[tokio::test]
    async fn dispatch_verification_failure_returns_replan() {
        let (gateway, executions, _db) = gateway(
            "read_file",
            SandboxProfile::ReadOnly,
            VerificationResult::failure("not observed", None),
        );
        let call = tool_call("read_file", json!({"path": "README.md"}));
        let args = json!({"path": "README.md"});
        let mut state = AgentState::new("system".to_string());
        let approvals = ApprovalStore::new();
        let (sender, mut receiver) = mpsc::channel(16);

        let outcome = dispatch_tool_call(
            &mut state,
            &mut super::ToolLoopGuard::default(),
            &gateway,
            &approvals,
            "conversation-1",
            &call,
            &args,
            &sender,
            &LogBuffer::new(16),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, ToolDispatchOutcome::Replan));
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert!(state
            .messages
            .last()
            .unwrap()
            .content
            .as_deref()
            .unwrap()
            .contains("not observed"));
        let verification = events(&mut receiver)
            .into_iter()
            .find(|event| event.event_type == "verification")
            .unwrap();
        assert_eq!(verification.verification_success, Some(false));
        assert_eq!(verification.should_replan, Some(true));
    }

    #[tokio::test]
    async fn dispatch_registry_miss_pairs_tool_start_with_tool_end() {
        let db_path = std::env::temp_dir().join(format!(
            "yilian-engine-registry-miss-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = crate::db::Database::new(&db_path).unwrap();
        let gateway = SecurityExecutionGateway::with_sandbox_registry_and_verifier(
            SandboxConfig {
                profile: SandboxProfile::ReadOnly,
                writable_paths: Vec::new(),
                denied_write_paths: Vec::new(),
            },
            ".",
            Arc::new(ToolRegistry::new()),
            Arc::new(FixedVerifier {
                result: VerificationResult::success("unused", None),
            }),
        )
        .with_db(Arc::new(db.clone_connection()));
        let call = tool_call("read_file", json!({"path": "README.md"}));
        let args = json!({"path": "README.md"});
        let mut state = AgentState::new("system".to_string());
        let approvals = ApprovalStore::new();
        let (sender, mut receiver) = mpsc::channel(16);

        let outcome = dispatch_tool_call(
            &mut state,
            &mut super::ToolLoopGuard::default(),
            &gateway,
            &approvals,
            "conversation-1",
            &call,
            &args,
            &sender,
            &LogBuffer::new(16),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, ToolDispatchOutcome::Continue));
        let emitted = events(&mut receiver);
        assert_eq!(
            emitted
                .iter()
                .map(|event| event.event_type.as_str())
                .collect::<Vec<_>>(),
            vec!["tool_start", "tool_end"]
        );
        assert_eq!(emitted[1].status.as_deref(), Some("error"));
        assert!(emitted[1]
            .result
            .as_deref()
            .unwrap()
            .contains("tool not found"));
    }

    #[tokio::test]
    async fn dispatch_reuses_existing_approval_identity_in_sse() {
        let (gateway, executions, _db) = gateway(
            "bash",
            SandboxProfile::Open,
            VerificationResult::success("unused", None),
        );
        let call = tool_call("bash", json!({"command": "git push origin develop"}));
        let args = json!({"command": "git push origin develop"});
        let mut state = AgentState::new("system".to_string());
        let approvals = ApprovalStore::new();
        let existing = approvals.create(
            "conversation-1".to_string(),
            "existing-call".to_string(),
            "process".to_string(),
            json!({"action": "kill", "pid": 42}),
            crate::tools::RiskLevel::High,
            "approve process kill".to_string(),
            "local-user".to_string(),
        );
        let (sender, mut receiver) = mpsc::channel(16);

        let outcome = dispatch_tool_call(
            &mut state,
            &mut super::ToolLoopGuard::default(),
            &gateway,
            &approvals,
            "conversation-1",
            &call,
            &args,
            &sender,
            &LogBuffer::new(16),
        )
        .await
        .unwrap();

        assert!(matches!(
            outcome,
            ToolDispatchOutcome::Paused { approval_id } if approval_id == existing.approval_id
        ));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        let emitted = events(&mut receiver);
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].tool_call_id.as_deref(), Some("existing-call"));
        assert_eq!(emitted[0].tool_name.as_deref(), Some("process"));
        assert_eq!(
            emitted[0].args.as_ref(),
            Some(&json!({"action": "kill", "pid": 42}))
        );
        assert_eq!(emitted[0].reason.as_deref(), Some("approve process kill"));
        assert_eq!(emitted[0].risk_level.as_deref(), Some("high"));
        assert_eq!(
            emitted[0].approval_id.as_deref(),
            Some(existing.approval_id.as_str())
        );
        assert!(state
            .messages
            .last()
            .unwrap()
            .content
            .as_deref()
            .unwrap()
            .contains("skipped"));
    }
}

#[cfg(test)]
mod route_wire_tests {
    use super::*;
    #[tokio::test]
    async fn reasoning_snapshot_recovers_deltas_dropped_by_a_full_live_channel() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let journal = parking_lot::Mutex::new(String::new());
        record_reasoning_delta(&tx, "c", &journal, "first ");
        record_reasoning_delta(&tx, "c", &journal, "second ");
        record_reasoning_delta(&tx, "c", &journal, "third");
        assert_eq!(rx.recv().await.unwrap().token.as_deref(), Some("first "));
        assert!(
            rx.try_recv().is_err(),
            "live channel really dropped the later deltas"
        );
        flush_reasoning_snapshot(&tx, "c", &journal).await;
        let complete = rx.recv().await.unwrap();
        assert_eq!(complete.event_type, "reasoning_snapshot");
        assert_eq!(complete.token.as_deref(), Some("first second third"));
    }
    #[test]
    fn model_route_serializes_actual_payload_at_top_level() {
        let mut event = AgentEvent::error_event("conversation-a", String::new());
        event.event_type = "model_route".into();
        event.error = None;
        event.args = Some(
            serde_json::json!({"phase":"switched","request_id":"request-a","attempt_id":"attempt-b","replace_content":true,"interrupted_content":"unfinished text","to_profile_id":"second"}),
        );
        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["type"], "model_route");
        assert_eq!(value["conversation_id"], "conversation-a");
        assert_eq!(value["replace_content"], true);
        assert_eq!(value["interrupted_content"], "unfinished text");
        assert!(value.get("args").is_none());
    }
}

#[cfg(test)]
#[path = "engine/loop_policy_tests.rs"]
mod loop_policy_tests;
