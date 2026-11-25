use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use yilian_backend::{
    app::router::build_router, integrations::secret::InMemorySecretStore, safety::ControlSession,
    AppServer,
};

const CONTROL: &str = "yilian-cli-model-switch-fixture-control-000";

#[tokio::test]
#[ignore = "requires llama-server with a local chat model"]
async fn local_cpu_model_completes_and_persists_the_normal_chat_turn() {
    let url = std::env::var("YILIAN_LOCAL_CHAT_QA_ENDPOINT").unwrap();
    let model = std::env::var("YILIAN_LOCAL_CHAT_QA_MODEL").unwrap();
    let path = std::env::temp_dir().join(format!("yilian-local-chat-{}.db", uuid::Uuid::new_v4()));
    let server = Arc::new(
        AppServer::new_with_control_session_and_store(
            &path,
            ".",
            ControlSession::new(CONTROL).unwrap(),
            Arc::new(InMemorySecretStore::new()),
        )
        .unwrap(),
    );
    {
        let mut config = server.config.write();
        config.model.provider = "llamacpp".into();
        config.model.name = model;
        config.model.base_url = url;
        config.model.api_key_env.clear();
        config.model.max_tokens = 24;
        config.model.invoke_timeout_ms = 240_000;
        assert!(config.model.uses_local_chat());
    }
    let router = build_router(server);
    let readiness: Value =
        serde_json::from_str(&request(&router, "GET", "/api/chat/readiness", Value::Null).await)
            .unwrap();
    assert_eq!(readiness["available"], true);
    let stream = request(
        &router,
        "POST",
        "/api/chat",
        json!({
            "message":"Say hello in one brief sentence.",
            "workspace_mode":"chat"
        }),
    )
    .await;
    assert!(stream.contains("\"type\":\"token\""), "{stream}");
    assert!(stream.contains("\"type\":\"done\""), "{stream}");
    let conversation = stream
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .find_map(|line| {
            serde_json::from_str::<Value>(line.trim())
                .ok()
                .filter(|event| event.get("user_message_id").is_some())
                .and_then(|event| event["conversation_id"].as_str().map(str::to_owned))
        })
        .unwrap();
    let stored = request(
        &router,
        "GET",
        &format!("/api/conversations/{conversation}"),
        Value::Null,
    )
    .await;
    assert!(
        stored.contains("Say hello in one brief sentence.") && stored.contains("\"assistant\""),
        "{stored}"
    );
}

async fn request(router: &Router, method: &str, path: &str, body: Value) -> String {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .header("X-Yilian-Control-Session", CONTROL)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let text = String::from_utf8(
        to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert_eq!(status, StatusCode::OK, "{method} {path}: {text}");
    text
}

#[tokio::test]
#[ignore = "requires an installed authenticated Codex CLI; sends only a synthetic conversation"]
async fn openai_compatible_then_codex_reuses_canonical_history() {
    let cli_profile = std::env::var("YILIAN_QA_CLI_PROFILE").unwrap_or_else(|_| "cli:codex".into());
    let path = std::env::temp_dir().join(format!("yilian-cli-switch-{}.db", uuid::Uuid::new_v4()));
    let server = Arc::new(
        AppServer::new_with_control_session_and_store(
            &path,
            ".",
            ControlSession::new(CONTROL).unwrap(),
            Arc::new(InMemorySecretStore::new()),
        )
        .unwrap(),
    );
    let fixture = Router::new().route("/v1/chat/completions", post(|Json(body): Json<Value>| async move {
        if body["stream"].as_bool().unwrap_or(false) {
            ([("content-type", "text/event-stream")], concat!(
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"The canonical answer is ORANGE-COMET-537.\"}}]}\n\n",
                "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n",
            )).into_response()
        } else {
            Json(json!({"id":"fixture","choices":[{"index":0,"message":{"role":"assistant","content":"The canonical answer is ORANGE-COMET-537."},"finish_reason":"stop"}]})).into_response()
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    {
        let mut config = server.config.write();
        config.model.provider = "ollama".into();
        config.model.name = "local-api-fixture".into();
        config.model.base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    }
    let fixture = tokio::spawn(async move { axum::serve(listener, fixture).await.unwrap() });
    let router = build_router(server.clone());
    let first = request(
        &router,
        "POST",
        "/api/chat",
        json!({"message":"Give me the test marker.", "workspace_mode":"chat"}),
    )
    .await;
    assert!(first.contains("ORANGE-COMET-537"), "{first}");
    let connected = first
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .find_map(|line| {
            serde_json::from_str::<Value>(line.trim())
                .ok()
                .filter(|value| value.get("user_message_id").is_some())
        })
        .unwrap();
    let conversation = connected["conversation_id"].as_str().unwrap();
    let api_receipt: Value = serde_json::from_str(
        &request(
            &router,
            "GET",
            &format!("/api/conversations/{conversation}/context-manifest"),
            Value::Null,
        )
        .await,
    )
    .unwrap();
    assert_eq!(
        api_receipt["projection"]["event_cursor"],
        connected["user_message_id"]
    );
    assert_eq!(
        api_receipt["projection"]["token_budget"]["unit"],
        "estimated_tokens"
    );
    let second = request(
        &router,
        "POST",
        "/api/chat",
        json!({
            "message":"From the previous assistant reply, return only the test marker unchanged.",
            "conversation_id":conversation,
        "model_profile_id":cli_profile,
            "workspace_mode":"chat"
        }),
    )
    .await;
    assert!(second.contains("\"type\":\"done\""), "{second}");
    assert!(second.contains("ORANGE-COMET-537"), "{second}");
    let detail = request(
        &router,
        "GET",
        &format!("/api/conversations/{conversation}"),
        Value::Null,
    )
    .await;
    assert!(detail.contains("Give me the test marker."), "{detail}");
    assert!(
        detail.contains("From the previous assistant reply"),
        "{detail}"
    );
    assert!(detail.matches("ORANGE-COMET-537").count() >= 2, "{detail}");
    let conn = rusqlite::Connection::open(&path).unwrap();
    let cli_turn: (String, String, String, i64, String) = conn
        .query_row(
            "SELECT provider,state,session_id,history_entries,context_digest FROM cli_chat_turns",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(cli_turn.0, cli_profile);
    assert_eq!(cli_turn.1, "completed");
    assert!(!cli_turn.2.is_empty() && cli_turn.3 >= 2);
    let cli_receipt: Value = serde_json::from_str(
        &request(
            &router,
            "GET",
            &format!("/api/conversations/{conversation}/context-manifest"),
            Value::Null,
        )
        .await,
    )
    .unwrap();
    let projection = &cli_receipt["projection"];
    assert_eq!(projection["conversation_id"], conversation);
    assert_eq!(projection["model_id"], cli_profile);
    assert_eq!(projection["content_digest"], cli_turn.4);
    assert_eq!(projection["token_budget"]["unit"], "characters");
    assert_ne!(
        projection["event_cursor"],
        api_receipt["projection"]["event_cursor"]
    );
    assert!(!cli_receipt.to_string().contains("ORANGE-COMET-537"));
    fixture.abort();
}

use axum::response::IntoResponse;
