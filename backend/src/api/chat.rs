// ============================================================
// Chat API — POST /api/chat (SSE streaming)
// ============================================================

use axum::{
    extract::State,
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::agent::engine::{self, AgentEvent};
use crate::agent::state::AgentState;
use crate::db::MessageRow;
use crate::llm::client::LlmClient;
use crate::server::AppServer;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub conversation_id: Option<String>,
    pub message: String,
    pub workflow_id: Option<String>,
}

/// POST /api/chat — SSE streaming agent response
pub async fn chat_handler(
    State(server): State<Arc<AppServer>>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(256);

    let config = server.config.read().clone();
    let db = server.db.clone_connection();
    let tool_registry = server.tool_registry.clone();
    let mut system_prompt = server.build_system_prompt();

    // Merge workflow context if workflow_id is provided
    let workflow_prompt = if let Some(ref wf_id) = req.workflow_id {
        if let Ok(Some(wf)) = db.get_workflow(wf_id) {
            let mut extra = String::new();
            extra.push_str("\n\n## 当前工作流\n\n");
            extra.push_str(&format!("**名称**: {}\n", wf.name));
            if !wf.description.is_empty() {
                extra.push_str(&format!("**描述**: {}\n", wf.description));
            }
            if !wf.nodes.is_empty() {
                extra.push_str(&format!("**流程**: {}\n", wf.nodes.join(" → ")));
            }
            if !wf.system_prompt_extra.is_empty() {
                extra.push_str(&format!("\n{}\n", wf.system_prompt_extra));
            }
            extra.push_str("\n请按照以上工作流结构执行任务。");
            Some(extra)
        } else {
            None
        }
    } else {
        // Use active workflow if no explicit workflow_id
        let active_wf = db
            .get_active_workflow_id()
            .ok()
            .flatten()
            .and_then(|aid| db.get_workflow(&aid).ok().flatten());
        active_wf.map(|wf| {
            let mut extra = String::new();
            extra.push_str("\n\n## 当前激活工作流\n\n");
            extra.push_str(&format!("**名称**: {}\n", wf.name));
            if !wf.description.is_empty() {
                extra.push_str(&format!("**描述**: {}\n", wf.description));
            }
            if !wf.nodes.is_empty() {
                extra.push_str(&format!("**流程**: {}\n", wf.nodes.join(" → ")));
            }
            if !wf.system_prompt_extra.is_empty() {
                extra.push_str(&format!("\n{}\n", wf.system_prompt_extra));
            }
            extra.push_str("\n请按照以上工作流结构执行任务。");
            extra
        })
    };

    if let Some(extra) = workflow_prompt {
        system_prompt.push_str(&extra);
    }

    // Get or create conversation
    let conv_id = match req.conversation_id {
        Some(ref id) if !id.is_empty() => id.clone(),
        _ => {
            let summary = db.create_conversation("新对话").unwrap_or_else(|_| {
                crate::db::ConversationSummary {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: "新对话".to_string(),
                    created_at: chrono::Utc::now().timestamp_millis(),
                    updated_at: chrono::Utc::now().timestamp_millis(),
                }
            });
            summary.id
        }
    };

    // Create cancel token
    let cancel_token = CancellationToken::new();
    server
        .active_tasks
        .lock()
        .insert(conv_id.clone(), cancel_token.clone());

    // Save user message & track existing message count for later
    let now = chrono::Utc::now().timestamp_millis();
    let _ = db.add_message(&MessageRow {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: conv_id.clone(),
        role: "user".to_string(),
        content: req.message.clone(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_result: None,
        created_at: now,
    });
    let prev_msg_count = if let Ok(conv) = db.get_conversation(&conv_id) {
        conv.messages.len()
    } else {
        0
    };

    // Auto-title: use first user message (trim to 40 chars)
    let title = if req.message.len() > 40 {
        format!("{}…", &req.message[..40])
    } else {
        req.message.clone()
    };
    let _ = db.update_conversation_title(&conv_id, &title);

    // Build agent state with history
    let mut agent_state = AgentState::new(system_prompt);
    if let Ok(conv) = db.get_conversation(&conv_id) {
        let msgs: Vec<crate::llm::types::ChatMessage> = conv
            .messages
            .iter()
            .map(|m| crate::llm::types::ChatMessage {
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
    agent_state.add_user_message(req.message.clone());

    // Log chat request
    let msg_preview = if req.message.len() > 60 {
        format!("{}…", &req.message[..60])
    } else {
        req.message.clone()
    };
    server
        .log_buffer
        .push("chat", "api", &format!("收到消息: {}", msg_preview));

    // Spawn agent loop
    let llm_client = LlmClient::new(&config.model);
    let conv_clone = conv_id.clone();
    let config_clone = config.clone();
    let log_buffer = server.log_buffer.clone();

    let db_clone = db.clone();

    tokio::spawn(async move {
        log_buffer.push("info", "agent", "Agent 循环开始");
        let verifier = crate::agent::verifier::DefaultVerifier::new(&server.workspace_root);

        let result = engine::run_react_loop_with_channel(
            &mut agent_state,
            &llm_client,
            &tool_registry,
            &server.approval_store,
            &verifier,
            &config_clone,
            &conv_clone,
            &cancel_token,
            &tx,
            &log_buffer,
        )
        .await;

        // Save new messages (only assistant + tool — those after the initial history + user msg)
        let total_msgs = agent_state.messages.len();
        let new_start = prev_msg_count; // skip already-saved system + history + user
        let now = chrono::Utc::now().timestamp_millis();
        for msg in &agent_state.messages[new_start.min(total_msgs)..] {
            if msg.role == "user" || msg.role == "system" {
                continue;
            }
            let _ = db_clone.add_message(&MessageRow {
                id: uuid::Uuid::new_v4().to_string(),
                conversation_id: conv_clone.clone(),
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

        match result {
            Err(ref e) => {
                log_buffer.push("error", "agent", &format!("Agent 错误: {}", e));
                let _ = tx
                    .send(AgentEvent {
                        event_type: "error".into(),
                        conversation_id: conv_clone.clone(),
                        error: Some(e.clone()),
                        token: None,
                        tool_call_id: None,
                        tool_name: None,
                        args: None,
                        result: None,
                        status: None,
                        message_id: None,
                        risk_level: None,
                        reason: None,
                        approval_id: None,
                        verification_success: None,
                        verification_reason: None,
                        should_replan: None,
                    })
                    .await;
            }
            Ok(engine::RunOutcome::Done { .. }) => {
                log_buffer.push("info", "agent", "Agent 完成");
            }
            Ok(engine::RunOutcome::Paused { approval_id }) => {
                log_buffer.push(
                    "warn",
                    "agent",
                    &format!("Agent 暂停，等待审批 {}", approval_id),
                );
                // Do NOT send "done": the frontend already received approval_required
                // and will show the approval card. The stream simply ends here.
            }
        }
        server.active_tasks.lock().remove(&conv_clone);
    });

    // Return SSE stream
    let conv_stream = conv_id.clone();
    let stream = async_stream::stream! {
        yield Ok(Event::default()
            .event("connected")
            .data(serde_json::json!({"conversation_id": conv_stream}).to_string()));

        while let Some(event) = rx.recv().await {
            let json = serde_json::to_string(&event).unwrap_or_default();
            yield Ok(Event::default().event("agent-event").data(json));
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}

/// POST /api/chat/stop — cancel a running generation
pub async fn stop_handler(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let conv_id = body["conversation_id"].as_str().unwrap_or("");
    if !conv_id.is_empty() {
        if let Some(token) = server.active_tasks.lock().get(conv_id) {
            token.cancel();
            return Json(serde_json::json!({"status": "cancelled"}));
        }
    }
    Json(serde_json::json!({"status": "not_found"}))
}
