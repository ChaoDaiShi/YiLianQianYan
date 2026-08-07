// ============================================================
// Agent Engine — Core ReAct loop (channel-based for HTTP server)
//
// Flow: prepare → think (LLM with tools) ↔ execute tools → respond
// ============================================================

use serde::Serialize;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use super::state::AgentState;
use crate::config::types::AppConfig;
use crate::llm::client::LlmClient;
use crate::safety::{PermissionDecision, PermissionManager};
use crate::server::LogBuffer;
use crate::tools::registry::ToolRegistry;
use crate::tools::trait_def::RiskLevel;

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
}

const MAX_ITERATIONS: usize = 20;
const MAX_CONSECUTIVE_SAME_TOOL: usize = 3;

/// Trim large binary payloads from tool results before sending to the LLM.
/// The full result is still delivered to the frontend via SSE.
fn summarize_tool_result(content: &str) -> String {
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
    if content.len() > 8000 {
        let mut end = 8000;
        while end > 0 && !content.is_char_boundary(end) {
            end -= 1;
        }
        let truncated = &content[..end];
        return format!("{}...\n(输出已截断，完整内容已展示在界面中)", truncated);
    }

    content.to_string()
}

/// Run the ReAct agent loop, sending streaming events through a channel.
pub async fn run_react_loop_with_channel(
    state: &mut AgentState,
    client: &LlmClient,
    tool_registry: &ToolRegistry,
    _config: &AppConfig,
    conversation_id: &str,
    cancel_token: &CancellationToken,
    tx: &Sender<AgentEvent>,
    log_buffer: &LogBuffer,
) -> Result<String, String> {
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

                for tc in &tool_calls {
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

                    // ── Safety gate: assess risk and check permission before executing ──
                    let tool = tool_registry.get(&tc.function.name);
                    let default_risk = tool.map(|t| t.risk_level()).unwrap_or(RiskLevel::Low);

                    let decision =
                        PermissionManager::evaluate(&tc.function.name, default_risk, &args);

                    match decision {
                        PermissionDecision::Allow => {
                            let _ = tx.try_send(AgentEvent {
                                event_type: "tool_start".into(),
                                conversation_id: conversation_id.to_string(),
                                token: None,
                                tool_call_id: Some(tc.id.clone()),
                                tool_name: Some(tc.function.name.clone()),
                                args: Some(args.clone()),
                                result: None,
                                status: None,
                                error: None,
                                message_id: None,
                                risk_level: None,
                                reason: None,
                            });

                            // Log tool start
                            log_buffer.push("tool", "tool", &format!("▶ {}", tc.function.name));

                            let tool_result =
                                match tool_registry.execute(&tc.function.name, args).await {
                                    Some(r) => r,
                                    None => crate::tools::trait_def::ToolResult::error(format!(
                                        "未知工具: {}",
                                        tc.function.name
                                    )),
                                };

                            let status = if tool_result.ok { "success" } else { "error" };

                            // Log tool result
                            let log_level = if tool_result.ok { "tool" } else { "error" };
                            let result_preview = if tool_result.content.len() > 80 {
                                format!("{}…", &tool_result.content[..80])
                            } else {
                                tool_result.content.clone()
                            };
                            log_buffer.push(
                                log_level,
                                "tool",
                                &format!(
                                    "{} {} – {}",
                                    if tool_result.ok { "✓" } else { "✗" },
                                    tc.function.name,
                                    result_preview
                                ),
                            );

                            // Send full result to frontend via SSE (includes images, etc.)
                            let _ = tx.try_send(AgentEvent {
                                event_type: "tool_end".into(),
                                conversation_id: conversation_id.to_string(),
                                token: None,
                                tool_call_id: Some(tc.id.clone()),
                                tool_name: Some(tc.function.name.clone()),
                                args: None,
                                result: Some(tool_result.content.clone()),
                                status: Some(status.to_string()),
                                error: None,
                                message_id: None,
                                risk_level: None,
                                reason: None,
                            });

                            // Trim data URIs before sending to LLM to avoid context pollution
                            // LLMs can't interpret base64, so we replace with a human-readable summary
                            let llm_result = summarize_tool_result(&tool_result.content);

                            state.add_tool_result(
                                tc.id.clone(),
                                tc.function.name.clone(),
                                llm_result,
                            );
                        }

                        PermissionDecision::RequireApproval { risk_level, reason } => {
                            let _ = tx.try_send(AgentEvent {
                                event_type: "approval_required".into(),
                                conversation_id: conversation_id.to_string(),
                                token: None,
                                tool_call_id: Some(tc.id.clone()),
                                tool_name: Some(tc.function.name.clone()),
                                args: Some(args.clone()),
                                result: None,
                                status: None,
                                error: None,
                                message_id: None,
                                risk_level: Some(risk_level.to_string()),
                                reason: Some(reason.clone()),
                            });

                            log_buffer.push(
                                "warn",
                                "safety",
                                &format!("⚠ {} 已阻止 — {}", tc.function.name, reason),
                            );

                            let blocked = format!(
                                "工具 {} 需要用户批准，当前操作尚未执行。原因：{}",
                                tc.function.name, reason
                            );
                            state.add_tool_result(tc.id.clone(), tc.function.name.clone(), blocked);
                        }

                        PermissionDecision::Deny { reason } => {
                            let denied =
                                format!("工具 {} 已被安全策略拒绝：{}", tc.function.name, reason);
                            state.add_tool_result(tc.id.clone(), tc.function.name.clone(), denied);
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
        });

        return Ok(output);
    }
}
