use super::*;
use crate::{
    config::types::ModelConfig,
    integrations::secret::{InMemorySecretStore, SecretResolver},
};
use axum::{extract::State, routing::post, Json, Router};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct MockLlm {
    window: usize,
    requests: Arc<Mutex<Vec<Value>>>,
}

async fn summarize(State(state): State<MockLlm>, Json(body): Json<Value>) -> Json<Value> {
    let messages: Vec<ChatMessage> = serde_json::from_value(body["messages"].clone()).unwrap();
    let upper_bound = conservative_input_token_upper_bound(&messages, &[]).unwrap();
    let max_output = body["max_tokens"].as_u64().unwrap();
    assert!(
        upper_bound + max_output <= state.window as u64,
        "summary request exceeded the configured window"
    );
    assert_eq!(body["model"], "compaction-loopback");
    assert!(body.get("tools").is_none());
    state.requests.lock().unwrap().push(body);
    Json(json!({
        "id":"compaction-fixture",
        "choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"已保留历史目标、约束和来源。"}}],
        "usage":{"prompt_tokens":upper_bound,"completion_tokens":16,"total_tokens":upper_bound+16}
    }))
}

#[tokio::test]
async fn small_windows_compact_via_loopback_without_losing_history_or_splitting_tool_pairs() {
    for window in [2048, 4096, 8192] {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/chat/completions", post(summarize))
            .with_state(MockLlm {
                window,
                requests: Arc::clone(&requests),
            });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let model = ModelConfig {
            provider: "lmstudio".into(),
            name: "compaction-loopback".into(),
            base_url: format!("http://{address}"),
            api_key: String::new(),
            api_key_env: String::new(),
            api_key_ref: None,
            max_tokens: (window / 4) as u32,
            invoke_timeout_ms: 2000,
            ..ModelConfig::default()
        };
        let client = LlmClient::new(
            &model,
            Arc::new(SecretResolver::new(Arc::new(InMemorySecretStore::new()))),
        );
        let config = CompactionConfig {
            enabled: true,
            context_window: window,
            trigger_threshold: 0.5,
            keep_recent_tokens: 1,
        };
        let mut canonical = vec![
            ChatMessage::system("保留用户要求，依据工具结果回答。"),
            ChatMessage::user("旧资料：中文、引号\"与路径\\。".repeat(window / 8)),
            serde_json::from_value(json!({"role":"assistant","content":"历史轮次已完成"})).unwrap(),
            ChatMessage::user("现在读取当前文件并说明结果。"),
            serde_json::from_value(json!({"role":"assistant","content":null,"tool_calls":[{"id":"current-read","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"current.txt\"}"}}]})).unwrap(),
            serde_json::from_value(json!({"role":"tool","tool_call_id":"current-read","content":"当前文件的真实内容"})).unwrap(),
            serde_json::from_value(json!({"role":"assistant","content":"已读取当前文件。"})).unwrap(),
        ];
        let before = serde_json::to_value(&canonical).unwrap();
        let historical_json = serde_json::to_string(&canonical[1..3]).unwrap();
        let tools = vec![
            json!({"type":"function","function":{"name":"read_file","parameters":{"type":"object"}}}),
        ];
        let mut checkpoint = None;
        let projected = project(
            &canonical,
            &tools,
            &config,
            &client,
            &CancellationToken::new(),
            &mut checkpoint,
        )
        .await
        .unwrap();
        assert_eq!(serde_json::to_value(&canonical).unwrap(), before);
        assert_eq!(checkpoint.as_ref().unwrap().through, 3);
        assert_eq!(
            serde_json::to_value(&projected[2..]).unwrap(),
            serde_json::to_value(&canonical[3..]).unwrap(),
            "complete current user turn and assistant/tool pairs must remain unchanged"
        );

        let count = {
            let captured = requests.lock().unwrap();
            assert!(
                captured.len() > 1,
                "history must actually be processed in multiple bounded requests"
            );
            let reconstructed: String = captured
                .iter()
                .enumerate()
                .map(|(index, request)| {
                    let text = request["messages"][1]["content"].as_str().unwrap();
                    if index > 0 {
                        assert!(text.starts_with("既有摘要：\n已保留历史目标、约束和来源。\n"));
                    }
                    text.split_once("\n历史片段：\n").unwrap().1
                })
                .collect();
            assert_eq!(
                reconstructed, historical_json,
                "every historical character must reach the summarizer exactly once"
            );
            captured.len()
        };

        // Serialize and reload as persistence does, then add a new user turn.
        let mut restored: Option<CompactedContext> =
            serde_json::from_value(serde_json::to_value(&checkpoint).unwrap()).unwrap();
        canonical.push(ChatMessage::user("继续，沿用刚才的事实。"));
        let appended_before = serde_json::to_value(&canonical).unwrap();
        let reused = project(
            &canonical,
            &tools,
            &config,
            &client,
            &CancellationToken::new(),
            &mut restored,
        )
        .await
        .unwrap();
        assert_eq!(
            requests.lock().unwrap().len(),
            count,
            "valid persisted checkpoint should be reused without another model call"
        );
        assert_eq!(serde_json::to_value(&canonical).unwrap(), appended_before);
        assert_eq!(
            serde_json::to_value(&reused[2..]).unwrap(),
            serde_json::to_value(&canonical[3..]).unwrap()
        );
        server.abort();
    }
}
