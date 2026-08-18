use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{header::CONTENT_TYPE, Request, StatusCode},
    routing::post,
    Json, Router,
};
use secrecy::SecretString;
use tower::ServiceExt;

use crate::db::LlmModelInput;
use crate::safety::{ControlSession, CONTROL_SESSION_HEADER};
use crate::secret::{llm_model_key_ref, InMemorySecretStore, SecretRef, SecretStore};
use crate::server::AppServer;

struct TempDatabase(PathBuf);

impl TempDatabase {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("yilian-llm-api-red-{}.db", uuid::Uuid::new_v4())))
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[tokio::test]
async fn list_models_redacts_api_key_and_exposes_configuration_status() {
    let db_path = TempDatabase::new();
    let store = Arc::new(InMemorySecretStore::new());
    store
        .put(
            &SecretRef::new("llm.model-1"),
            SecretString::from("SUPER_SECRET".to_string()),
        )
        .await
        .unwrap();
    let server = Arc::new(
        AppServer::new_with_control_session_and_store(
            &db_path.0,
            ".",
            ControlSession::new("a".repeat(64)).unwrap(),
            store,
        )
        .unwrap(),
    );
    server
        .db
        .create_llm_model(&LlmModelInput {
            id: "model-1".into(),
            provider: "deepseek".into(),
            label: "研发模型".into(),
            model: "deepseek-chat".into(),
            base_url: "http://127.0.0.1:9000/v1".into(),
            api_format: "openai".into(),
            api_key_ref: "llm.model-1".into(),
            api_key_env: String::new(),
            temperature: 0.0,
            max_tokens: 128,
            invoke_timeout_ms: 5_000,
        })
        .unwrap();

    let response = crate::api::build_router(server)
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/llm/models")
                .header(CONTROL_SESSION_HEADER, "a".repeat(64))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(!text.contains("SUPER_SECRET"));
    assert!(text.contains("api_key_configured"));
}

#[tokio::test]
async fn active_model_chat_uses_profile_and_persists_stream_usage() {
    let requests: Arc<tokio::sync::Mutex<Vec<serde_json::Value>>> = Default::default();
    let captured = Arc::clone(&requests);
    let mock = Router::new()
        .route(
            "/chat/completions",
            post(move |State(captured): State<Arc<tokio::sync::Mutex<Vec<serde_json::Value>>>>, Json(body): Json<serde_json::Value>| async move {
                captured.lock().await.push(body);
                (
                    [(CONTENT_TYPE, "text/event-stream")],
                    "data: {\"id\":\"chat-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\ndata: {\"id\":\"chat-1\",\"choices\":[],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":3,\"total_tokens\":12}}\n\ndata: [DONE]\n\n",
                )
            }),
        )
        .with_state(captured);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let mock_task = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });

    let db_path = TempDatabase::new();
    let store = Arc::new(InMemorySecretStore::new());
    let model_id = "model-1";
    let secret_ref = llm_model_key_ref(model_id);
    store
        .put(&secret_ref, SecretString::from("test-key".to_string()))
        .await
        .unwrap();
    let token = "b".repeat(64);
    let server = Arc::new(
        AppServer::new_with_control_session_and_store(
            &db_path.0,
            ".",
            ControlSession::new(token.clone()).unwrap(),
            store,
        )
        .unwrap(),
    );
    server
        .db
        .create_llm_model(&LlmModelInput {
            id: model_id.into(),
            provider: "deepseek".into(),
            label: "研发模型".into(),
            model: "profile-model".into(),
            base_url,
            api_format: "openai".into(),
            api_key_ref: secret_ref.key,
            api_key_env: String::new(),
            temperature: 0.0,
            max_tokens: 128,
            invoke_timeout_ms: 5_000,
        })
        .unwrap();
    server
        .db
        .set_llm_model_verification(model_id, Some(chrono::Utc::now().timestamp_millis()), None)
        .unwrap();
    server.db.activate_llm_model(model_id).unwrap();

    let response = crate::api::build_router(Arc::clone(&server))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat")
                .header(CONTROL_SESSION_HEADER, token)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"message":"hello"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let _ = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();

    let captured = requests.lock().await;
    assert_eq!(captured[0]["model"], "profile-model");
    assert_eq!(captured[0]["stream_options"]["include_usage"], true);
    drop(captured);
    let report = server
        .db
        .aggregate_llm_usage(None, 0, chrono::Utc::now().timestamp_millis() + 1)
        .unwrap();
    assert_eq!(report.total_tokens, 12);
    mock_task.abort();
}
