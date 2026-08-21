// ============================================================
// Chat API — POST /api/chat (SSE streaming)
// ============================================================

use axum::{
    extract::State,
    http::StatusCode,
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
use crate::agent::verifier::DefaultVerifier;
use crate::db::{MessageRow, RetrieveQuery};
use crate::llm::client::LlmClient;
use crate::llm::usage::DatabaseUsageRecorder;
use crate::safety::SecurityExecutionGateway;
use crate::server::{AppServer, CHAT_MEMORY_TOP_K};
use crate::utils::text::truncate_chars;

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
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<serde_json::Value>)>
{
    let message = req.message.trim().to_string();
    if message.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "消息内容不能为空"})),
        ));
    }

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<AgentEvent>(256);
    let (stream_tx, mut stream_rx) = tokio::sync::mpsc::channel::<AgentEvent>(256);

    let db = server.db.clone_connection();
    let legacy_config = server.config.read().clone();
    let active_model = db.get_active_llm_model().ok().flatten();
    let mut config = legacy_config.clone();
    if let Some(model) = active_model.as_ref() {
        let mut active_config = model.to_model_config();
        active_config.embedding_model = legacy_config.model.embedding_model.clone();
        active_config.embedding_base_url = legacy_config.model.embedding_base_url.clone();
        active_config.embedding_api_key = legacy_config.model.embedding_api_key.clone();
        active_config.embedding_api_key_env = legacy_config.model.embedding_api_key_env.clone();
        active_config.embedding_api_key_ref = legacy_config.model.embedding_api_key_ref.clone();
        config.model = active_config;
    }
    // Build one MCP-aware runtime registry snapshot for this whole chat run.
    // It is used by BOTH the LLM tool definitions and the Security Gateway so
    // the LLM, evaluation, and execution all see the same tool set.
    let tool_registry = server.build_agent_tool_registry().await;
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

    // Retrieve relevant memories using the current user message as the query.
    // Best-effort enhancement: any retrieval/embedding failure must not block chat.
    let mut memory_mode = "lexical";
    let mut query_embedding: Option<Vec<f32>> = None;
    if config.model.has_embedding() {
        let llm = LlmClient::new(&config.model, Arc::clone(&server.secret_resolver));
        match llm.embed(&message).await {
            Ok(vec) => {
                query_embedding = Some(vec);
                memory_mode = "hybrid";
            }
            Err(e) => {
                tracing::warn!(error = %e, "chat query embedding failed, falling back to lexical memory retrieval");
                memory_mode = "lexical_fallback";
            }
        }
    }

    let retrieval_query = RetrieveQuery {
        q: message.clone(),
        top_k: CHAT_MEMORY_TOP_K,
        category: None,
    };
    match db.retrieve_memories_hybrid(&retrieval_query, query_embedding.as_deref()) {
        Ok(scored) => {
            if !scored.is_empty() {
                server.append_memory_context(&mut system_prompt, &scored);
                tracing::info!(
                    mode = memory_mode,
                    count = scored.len(),
                    "memory context injected"
                );
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "memory retrieval failed; continuing chat without memory context");
        }
    }

    // Get or create conversation
    let conv_id = match req.conversation_id {
        Some(ref id) if !id.is_empty() => id.clone(),
        _ => {
            let summary = db.create_conversation("新对话").unwrap_or_else(|_| {
                crate::db::ConversationSummary {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: "新对话".to_string(),
                    run_status: crate::db::ConversationRunStatus::Idle,
                    run_error: None,
                    run_started_at: None,
                    run_finished_at: None,
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

    // Build agent state from history before persisting the current user message.
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
    let prev_msg_count = agent_state.messages.len();

    // Persist the current user message after taking the history snapshot.
    let now = chrono::Utc::now().timestamp_millis();
    let _ = db.add_message(&MessageRow {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: conv_id.clone(),
        role: "user".to_string(),
        content: message.clone(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_result: None,
        created_at: now,
    });

    // Auto-title: use first user message (trim to 40 chars)
    let title = truncate_chars(&message, 40);
    let _ = db.update_conversation_title(&conv_id, &title);
    let _ =
        db.set_conversation_run_status(&conv_id, crate::db::ConversationRunStatus::Running, None);

    // Add the current user message to the LLM context exactly once.
    agent_state.add_user_message(message.clone());

    // Log chat request
    let msg_preview = truncate_chars(&message, 60);
    server
        .log_buffer
        .push("chat", "api", &format!("收到消息: {}", msg_preview));

    // Spawn agent loop
    let usage_recorder = active_model.as_ref().map(|model| {
        Arc::new(DatabaseUsageRecorder::new(
            db.clone_connection(),
            model.id.clone(),
            model.provider.clone(),
            model.model.clone(),
            "chat",
        )) as Arc<dyn crate::llm::client::UsageRecorder>
    });
    let llm_client = LlmClient::new_with_usage_recorder(
        &config.model,
        Arc::clone(&server.secret_resolver),
        usage_recorder,
    );
    let conv_clone = conv_id.clone();
    let config_clone = config.clone();
    let log_buffer = server.log_buffer.clone();

    let db_clone = db.clone();

    tokio::spawn(async move {
        log_buffer.push("info", "agent", "Agent 循环开始");
        let security_gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            config_clone.sandbox.clone(),
            server.workspace_root.clone(),
            Arc::clone(&tool_registry),
            Arc::new(DefaultVerifier::new(&server.workspace_root)),
            Arc::new(server.audit_recorder.clone()),
        )
        .with_db(Arc::new(db_clone.clone()))
        .with_grant_enforcement();

        let result = engine::run_react_loop_with_channel(
            &mut agent_state,
            &llm_client,
            &tool_registry,
            &server.approval_store,
            &security_gateway,
            &config_clone,
            &conv_clone,
            &cancel_token,
            &event_tx,
            &log_buffer,
        )
        .await;

        // Save new messages (only assistant + tool produced after the loaded history).
        let total_msgs = agent_state.messages.len();
        let new_start = prev_msg_count; // current user starts here and is filtered below
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
                let _ = db_clone.set_conversation_run_status(
                    &conv_clone,
                    crate::db::ConversationRunStatus::Failed,
                    Some(e),
                );
                log_buffer.push("error", "agent", &format!("Agent 错误: {}", e));
                let _ = event_tx
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
                let _ = db_clone.set_conversation_run_status(
                    &conv_clone,
                    crate::db::ConversationRunStatus::Completed,
                    None,
                );
                log_buffer.push("info", "agent", "Agent 完成");
            }
            Ok(engine::RunOutcome::Paused { approval_id }) => {
                let _ = db_clone.set_conversation_run_status(
                    &conv_clone,
                    crate::db::ConversationRunStatus::WaitingApproval,
                    None,
                );
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

    // Persist tool lifecycle events independently of the browser connection.
    // This keeps the execution history recoverable even when the page is left
    // while the agent is still running.
    let persistence_db = db.clone_connection();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Err(error) = persistence_db.apply_execution_event(&event) {
                tracing::warn!(error = %error, "failed to persist conversation execution event");
            }
            let _ = stream_tx.send(event).await;
        }
    });

    // Return SSE stream
    let conv_stream = conv_id.clone();
    let stream = async_stream::stream! {
        yield Ok(Event::default()
            .event("connected")
            .data(serde_json::json!({"conversation_id": conv_stream}).to_string()));

        while let Some(event) = stream_rx.recv().await {
            let json = serde_json::to_string(&event).unwrap_or_default();
            yield Ok(Event::default().event("agent-event").data(json));
        }
    };

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    ))
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
            let _ = server.db.set_conversation_run_status(
                conv_id,
                crate::db::ConversationRunStatus::Cancelled,
                None,
            );
            return Json(serde_json::json!({"status": "cancelled"}));
        }
    }
    Json(serde_json::json!({"status": "not_found"}))
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use axum::{
        body::{to_bytes, Body},
        extract::State,
        http::{header::CONTENT_TYPE, Request, StatusCode},
        response::IntoResponse,
        routing::post,
        Json, Router,
    };
    use serde_json::{json, Value};
    use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};
    use tower::ServiceExt;

    use crate::{
        agent::engine::AgentEvent,
        api::build_router,
        db::MessageRow,
        safety::{ControlSession, CONTROL_SESSION_HEADER},
        server::AppServer,
    };

    type CapturedRequests = Arc<Mutex<Vec<Value>>>;

    struct TempDatabase(PathBuf);

    impl TempDatabase {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "yilian-chat-user-message-{}.db",
                uuid::Uuid::new_v4()
            )))
        }
    }

    impl Drop for TempDatabase {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    async fn capture_llm_request(
        State(requests): State<CapturedRequests>,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        requests.lock().await.push(body);
        (
            [(CONTENT_TYPE, "text/event-stream")],
            concat!(
                "data: {\"id\":\"test\",\"choices\":[{\"index\":0,",
                "\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n"
            ),
        )
    }

    async fn start_mock_llm() -> (String, CapturedRequests, JoinHandle<()>) {
        let requests = CapturedRequests::default();
        let app = Router::new()
            .route("/chat/completions", post(capture_llm_request))
            .with_state(requests.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), requests, task)
    }

    fn test_server(database: &TempDatabase, base_url: &str) -> (Arc<AppServer>, String) {
        let token = "a".repeat(64);
        let server = Arc::new(
            AppServer::new_with_control_session(
                &database.0,
                ".",
                ControlSession::new(token.clone()).unwrap(),
            )
            .unwrap(),
        );
        {
            let mut config = server.config.write();
            config.model.base_url = base_url.to_string();
            config.model.api_key = "test-key".to_string();
            config.model.api_key_env.clear();
            config.model.invoke_timeout_ms = 5_000;
        }
        (server, token)
    }

    async fn send_chat(server: Arc<AppServer>, token: &str, body: Value) {
        let response = build_router(server)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header(CONTROL_SESSION_HEADER, token)
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let _ = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    }

    fn message_row(
        conversation_id: &str,
        role: &str,
        content: &str,
        created_at: i64,
    ) -> MessageRow {
        MessageRow {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: conversation_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at,
        }
    }

    fn current_user_message_count(request: &Value, content: &str) -> usize {
        request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| message["role"] == "user" && message["content"] == content)
            .count()
    }

    #[tokio::test]
    async fn existing_conversation_sends_current_user_message_once_and_keeps_history() {
        let (base_url, requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let conversation = server.db.create_conversation("existing").unwrap();
        for message in [
            message_row(&conversation.id, "user", "previous question", 1),
            message_row(&conversation.id, "assistant", "previous answer", 2),
            message_row(&conversation.id, "tool", "previous tool result", 3),
        ] {
            server.db.add_message(&message).unwrap();
        }

        send_chat(
            server.clone(),
            &token,
            json!({
                "conversation_id": conversation.id,
                "message": "帮我检查 README"
            }),
        )
        .await;

        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(current_user_message_count(request, "帮我检查 README"), 1);
        assert!(request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["role"] == "assistant" && message["content"] == "previous answer"
            }));
        assert!(request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["role"] == "tool" && message["content"] == "previous tool result"
            }));
        drop(requests);

        let persisted = server.db.get_conversation(&conversation.id).unwrap();
        assert_eq!(
            persisted
                .messages
                .iter()
                .filter(|message| {
                    message.role == "user" && message.content == "帮我检查 README"
                })
                .count(),
            1
        );
        mock_llm.abort();
    }

    #[tokio::test]
    async fn new_conversation_sends_and_persists_current_user_message_once() {
        let (base_url, requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);

        send_chat(
            server.clone(),
            &token,
            json!({"message": "帮我检查 README"}),
        )
        .await;

        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            current_user_message_count(&requests[0], "帮我检查 README"),
            1
        );
        drop(requests);

        let conversations = server.db.list_conversations().unwrap();
        assert_eq!(conversations.len(), 1);
        assert_eq!(
            conversations[0].run_status,
            crate::db::ConversationRunStatus::Completed
        );
        assert!(conversations[0].run_started_at.is_some());
        assert!(conversations[0].run_finished_at.is_some());
        let persisted = server.db.get_conversation(&conversations[0].id).unwrap();
        assert_eq!(
            persisted
                .messages
                .iter()
                .filter(|message| {
                    message.role == "user" && message.content == "帮我检查 README"
                })
                .count(),
            1
        );
        mock_llm.abort();
    }

    #[tokio::test]
    async fn blank_message_is_rejected_without_creating_a_conversation() {
        let (base_url, _requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);

        let response = build_router(server.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header(CONTROL_SESSION_HEADER, token)
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"message": "   "}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(server.db.list_conversations().unwrap().is_empty());
        mock_llm.abort();
    }

    #[tokio::test]
    async fn empty_conversations_are_hidden_and_loaded_conversations_include_execution_history() {
        let (base_url, _requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let empty = server.db.create_conversation("新对话").unwrap();

        send_chat(server.clone(), &token, json!({"message": "保留这条记录"})).await;

        let summaries = server.db.list_conversations().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_ne!(summaries[0].id, empty.id);

        let tool_call_id = "call-persisted";
        for event in [
            AgentEvent {
                event_type: "tool_start".to_string(),
                conversation_id: summaries[0].id.clone(),
                token: None,
                tool_call_id: Some(tool_call_id.to_string()),
                tool_name: Some("open_notepad".to_string()),
                args: Some(json!({"title": "记事本"})),
                result: None,
                status: None,
                error: None,
                message_id: None,
                risk_level: Some("low".to_string()),
                reason: None,
                approval_id: None,
                verification_success: None,
                verification_reason: None,
                should_replan: None,
            },
            AgentEvent {
                event_type: "tool_end".to_string(),
                conversation_id: summaries[0].id.clone(),
                token: None,
                tool_call_id: Some(tool_call_id.to_string()),
                tool_name: Some("open_notepad".to_string()),
                args: None,
                result: Some("已打开记事本。".to_string()),
                status: Some("success".to_string()),
                error: None,
                message_id: None,
                risk_level: None,
                reason: None,
                approval_id: None,
                verification_success: None,
                verification_reason: None,
                should_replan: None,
            },
            AgentEvent {
                event_type: "verification".to_string(),
                conversation_id: summaries[0].id.clone(),
                token: None,
                tool_call_id: Some(tool_call_id.to_string()),
                tool_name: Some("open_notepad".to_string()),
                args: None,
                result: None,
                status: None,
                error: None,
                message_id: None,
                risk_level: None,
                reason: None,
                approval_id: None,
                verification_success: Some(true),
                verification_reason: Some("窗口已出现".to_string()),
                should_replan: Some(false),
            },
        ] {
            server.db.apply_execution_event(&event).unwrap();
        }

        let loaded = server.db.get_conversation(&summaries[0].id).unwrap();
        let json = serde_json::to_value(loaded).unwrap();
        assert_eq!(json["execution_history"].as_array().unwrap().len(), 1);
        assert_eq!(json["execution_history"][0]["executionStatus"], "succeeded");
        assert_eq!(json["execution_history"][0]["verificationStatus"], "passed");
        mock_llm.abort();
    }

    // ── Memory context injection tests ──

    use crate::db::{Memory, ScoredMemory};

    fn scored_memory(category: &str, content: &str) -> ScoredMemory {
        ScoredMemory {
            memory: Memory {
                id: format!("mem-{category}"),
                content: content.to_string(),
                category: category.to_string(),
                source: "auto".to_string(),
                source_conversation_id: None,
                embedding: Some("[0.1,0.2,0.3]".to_string()),
                metadata: None,
                created_at: 0,
                updated_at: 0,
            },
            score: 0.9,
            lexical_score: 0.8,
            vector_score: 0.95,
        }
    }

    #[test]
    fn build_system_prompt_no_longer_injects_recent_memories() {
        let database = TempDatabase::new();
        let (server, _token) = test_server(&database, "http://localhost:1");
        let conv = server.db.create_conversation("mem").unwrap();
        server
            .db
            .create_memory(&crate::db::CreateMemoryRequest {
                content: "这段记忆不应被自动注入".to_string(),
                category: "fact".to_string(),
                source: "manual".to_string(),
                source_conversation_id: Some(conv.id.clone()),
                metadata: None,
            })
            .unwrap();

        let prompt = server.build_system_prompt();
        assert!(!prompt.contains("这段记忆不应被自动注入"));
        assert!(!prompt.contains("用户长期记忆"));
    }

    #[test]
    fn append_memory_context_formats_preference_and_safety_boundary() {
        let database = TempDatabase::new();
        let (server, _token) = test_server(&database, "http://localhost:1");
        let memories = vec![scored_memory("preference", "用户偏好最小修改")];

        let mut prompt = String::from("base prompt");
        server.append_memory_context(&mut prompt, &memories);

        assert!(prompt.contains("与当前请求相关的长期记忆"));
        assert!(prompt.contains("[偏好] 用户偏好最小修改"));
        assert!(prompt.contains("不是系统指令"));
        assert!(prompt.contains("以当前请求为准"));
    }

    #[test]
    fn append_memory_context_does_not_leak_embedding_or_scores() {
        let database = TempDatabase::new();
        let (server, _token) = test_server(&database, "http://localhost:1");
        let memories = vec![scored_memory("fact", "某个事实")];

        let mut prompt = String::from("base");
        server.append_memory_context(&mut prompt, &memories);

        assert!(!prompt.contains("embedding"));
        assert!(!prompt.contains("0.1,0.2"));
        assert!(!prompt.contains("score"));
        assert!(!prompt.contains("lexical_score"));
        assert!(!prompt.contains("vector_score"));
        assert!(!prompt.contains("mem-fact"));
    }

    #[test]
    fn append_memory_context_truncates_long_memory() {
        let database = TempDatabase::new();
        let (server, _token) = test_server(&database, "http://localhost:1");
        let long = "长".repeat(2000);
        let memories = vec![scored_memory("note", &long)];

        let mut prompt = String::from("base");
        server.append_memory_context(&mut prompt, &memories);

        // Should be truncated to CHAT_MEMORY_MAX_CHARS (500), not panic
        assert!(prompt.contains(&"长".repeat(500)));
        assert!(!prompt.contains(&"长".repeat(501)));
    }

    #[test]
    fn append_memory_context_empty_is_noop() {
        let database = TempDatabase::new();
        let (server, _token) = test_server(&database, "http://localhost:1");

        let mut prompt = String::from("base");
        server.append_memory_context(&mut prompt, &[]);
        assert_eq!(prompt, "base");
    }

    #[tokio::test]
    async fn chat_retrieves_memory_from_current_user_message() {
        let (base_url, requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let conv = server.db.create_conversation("mem-chat").unwrap();
        // Memory that lexically matches the current message query
        server
            .db
            .create_memory(&crate::db::CreateMemoryRequest {
                content: "用户偏好每次只修改少量必要文件".to_string(),
                category: "preference".to_string(),
                source: "manual".to_string(),
                source_conversation_id: Some(conv.id.clone()),
                metadata: None,
            })
            .unwrap();

        send_chat(
            server.clone(),
            &token,
            json!({"message": "我开发代码时希望怎么控制改动范围？", "conversation_id": conv.id}),
        )
        .await;

        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        let system_msg = requests[0]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "system")
            .expect("system message");
        // Lexical match on the current message should inject the memory
        assert!(system_msg["content"]
            .as_str()
            .unwrap()
            .contains("用户偏好每次只修改少量必要文件"));
        drop(requests);
        mock_llm.abort();
    }

    #[tokio::test]
    async fn chat_uses_lexical_mode_without_embedding_provider() {
        let (base_url, requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        // Embedding provider not configured (default test config has empty embedding model)
        let conv = server.db.create_conversation("mem-lexical").unwrap();
        server
            .db
            .create_memory(&crate::db::CreateMemoryRequest {
                content: "关于 Rust 安全执行的关键记忆".to_string(),
                category: "knowledge".to_string(),
                source: "manual".to_string(),
                source_conversation_id: Some(conv.id.clone()),
                metadata: None,
            })
            .unwrap();

        send_chat(
            server.clone(),
            &token,
            json!({"message": "Rust 安全执行", "conversation_id": conv.id}),
        )
        .await;

        // Chat still worked, LLM was called once
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        let system_msg = requests[0]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "system")
            .expect("system message");
        assert!(system_msg["content"]
            .as_str()
            .unwrap()
            .contains("Rust 安全执行的关键记忆"));
        drop(requests);
        mock_llm.abort();
    }

    #[tokio::test]
    async fn chat_no_related_memory_omits_section() {
        let (base_url, requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let conv = server.db.create_conversation("mem-none").unwrap();
        server
            .db
            .create_memory(&crate::db::CreateMemoryRequest {
                content: "关于咖啡口味的信息".to_string(),
                category: "fact".to_string(),
                source: "manual".to_string(),
                source_conversation_id: Some(conv.id.clone()),
                metadata: None,
            })
            .unwrap();

        send_chat(
            server.clone(),
            &token,
            json!({"message": "k8s networking ingress", "conversation_id": conv.id}),
        )
        .await;

        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        let system_msg = requests[0]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "system")
            .expect("system message");
        assert!(!system_msg["content"]
            .as_str()
            .unwrap()
            .contains("与当前请求相关的长期记忆"));
        drop(requests);
        mock_llm.abort();
    }
}
