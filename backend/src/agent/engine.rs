// ============================================================
// Agent Engine — Core ReAct loop (channel-based for HTTP server)
//
// Flow: prepare → think (LLM with tools) ↔ execute tools → respond
// ============================================================

use serde::Serialize;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use super::state::AgentState;
use super::verifier::replan_message;
use crate::config::types::AppConfig;
use crate::llm::client::LlmClient;
use crate::llm::types::ToolCall;
use crate::safety::approval::ApprovalStore;
use crate::safety::execution_gateway::SecurityExecutionOutcome;
use crate::safety::{SecurityExecutionGateway, SecurityExecutionRequest, SecuritySubject};
use crate::server::LogBuffer;
use crate::tools::registry::ToolRegistry;
use crate::tools::trait_def::RiskLevel;
use crate::utils::text::truncate_chars;

/// Agent streaming event (shared with API layer)
#[derive(Debug, Clone, Serialize)]
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

/// Result of running the agent loop.
pub enum RunOutcome {
    /// The loop produced a final answer (the "done" event was already sent).
    Done { output: String },
    /// The loop paused awaiting user approval (approval_required was sent).
    Paused { approval_id: String },
}

const MAX_ITERATIONS: usize = 20;
const MAX_CONSECUTIVE_SAME_TOOL: usize = 3;

enum ToolDispatchOutcome {
    Continue,
    Replan,
    Paused { approval_id: String },
}

async fn dispatch_tool_call(
    state: &mut AgentState,
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

            if verification.should_replan {
                state.add_tool_result(
                    tool_call.id.clone(),
                    tool_call.function.name.clone(),
                    replan_message(&tool_call.function.name, &verification.reason),
                );
                return Ok(ToolDispatchOutcome::Replan);
            }

            state.add_tool_result(
                tool_call.id.clone(),
                tool_call.function.name.clone(),
                summarize_tool_result(&tool_result.content),
            );
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
    let tools_openai = tool_registry.to_openai_tools();
    let mut iteration = 0;
    let mut last_tool_name = String::new();
    let mut consecutive_same_tool = 0usize;

    loop {
        if cancel_token.is_cancelled() {
            return Err("已取消".to_string());
        }

        iteration += 1;
        if iteration > MAX_ITERATIONS {
            return Err("已达到最大工具调用次数限制".to_string());
        }

        // ── THINK: Call LLM with streaming ──
        let result = client
            .stream_with_callbacks(&state.messages, &tools_openai, |token| {
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
            })
            .await;

        let accumulator = match result {
            Ok(acc) => acc,
            Err(e) => return Err(format!("LLM调用失败: {}", e)),
        };

        // ── Check for tool calls ──
        if accumulator.has_tool_calls() {
            if let Some(tool_calls) = accumulator.to_tool_calls() {
                state.add_assistant_message(
                    if accumulator.content.is_empty() {
                        None
                    } else {
                        Some(accumulator.content.clone())
                    },
                    Some(tool_calls.clone()),
                );

                for (i, tc) in tool_calls.iter().enumerate() {
                    if cancel_token.is_cancelled() {
                        return Err("已取消".to_string());
                    }

                    // Guard against infinite tool loops: bail if same tool called too many times in a row
                    if tc.function.name == last_tool_name {
                        consecutive_same_tool += 1;
                        if consecutive_same_tool >= MAX_CONSECUTIVE_SAME_TOOL {
                            return Err(format!(
                                "工具 {} 被连续调用 {} 次，可能陷入循环，已中止",
                                tc.function.name, consecutive_same_tool
                            ));
                        }
                    } else {
                        consecutive_same_tool = 1;
                        last_tool_name = tc.function.name.clone();
                    }

                    let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(serde_json::Value::Null);

                    match dispatch_tool_call(
                        state,
                        security_gateway,
                        approval_store,
                        conversation_id,
                        tc,
                        &args,
                        tx,
                        log_buffer,
                    )
                    .await?
                    {
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
                    }
                }
                continue;
            }
        }

        // ── RESPOND: Final answer ──
        let output = accumulator.content.clone();
        state.add_assistant_message(Some(output.clone()), None);
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

    use super::{dispatch_tool_call, AgentEvent, ToolDispatchOutcome};
    use crate::agent::state::AgentState;
    use crate::agent::verifier::{VerificationResult, Verifier};
    use crate::config::types::{SandboxConfig, SandboxProfile};
    use crate::llm::types::{ToolCall, ToolCallFunction};
    use crate::safety::{ApprovalStore, SecurityExecutionGateway};
    use crate::server::LogBuffer;
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
