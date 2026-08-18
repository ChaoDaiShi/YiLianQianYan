use std::sync::Arc;

use super::types::{ChatCompletionRequest, ChatMessage, StreamChunk, Usage};
use super::usage::DatabaseUsageRecorder;
use crate::db::{Database, LlmModelInput};
use crate::llm::client::UsageRecorder;

#[test]
fn stream_chunk_keeps_provider_usage() {
    let chunk: StreamChunk = serde_json::from_str(
        r#"{
            "id":"chatcmpl-1",
            "choices":[],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        }"#,
    )
    .unwrap();

    let usage = chunk.usage.expect("stream usage should be retained");
    assert_eq!(usage.total_tokens, 18);
}

#[test]
fn streaming_request_asks_openai_compatible_servers_for_usage() {
    let request = ChatCompletionRequest {
        model: "test-model".into(),
        messages: vec![ChatMessage {
            role: "user".into(),
            content: Some("ping".into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }],
        tools: None,
        tool_choice: None,
        temperature: Some(0.0),
        max_tokens: Some(10),
        stream: true,
        stream_options: Some(super::types::StreamOptions {
            include_usage: true,
        }),
    };
    let json = serde_json::to_value(request).unwrap();
    assert_eq!(json["stream_options"]["include_usage"], true);
}

#[test]
fn database_usage_recorder_persists_provider_usage() {
    let path = std::env::temp_dir().join(format!(
        "yilian-usage-recorder-red-{}.db",
        uuid::Uuid::new_v4()
    ));
    let db = Database::new(&path).unwrap();
    db.create_llm_model(&LlmModelInput {
        id: "model-1".into(),
        provider: "openai".into(),
        label: "GPT".into(),
        model: "gpt-4o-mini".into(),
        base_url: "http://127.0.0.1:9000/v1".into(),
        api_format: "openai".into(),
        api_key_ref: "llm.model-1".into(),
        api_key_env: String::new(),
        temperature: 0.0,
        max_tokens: 128,
        invoke_timeout_ms: 5_000,
    })
    .unwrap();
    let recorder = DatabaseUsageRecorder::new(
        db.clone_connection(),
        "model-1",
        "openai",
        "gpt-4o-mini",
        "chat",
    );
    recorder.record(&Usage {
        prompt_tokens: 20,
        completion_tokens: 5,
        total_tokens: 25,
    });
    let report = db
        .aggregate_llm_usage(None, 0, chrono::Utc::now().timestamp_millis() + 1)
        .unwrap();
    assert_eq!(report.total_tokens, 25);
    drop(Arc::new(recorder));
    let _ = std::fs::remove_file(path);
}
