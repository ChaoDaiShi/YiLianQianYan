use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use yilian_backend::{
    db::ConversationRunStatus,
    integrations::{llm::types::ChatMessage, secret::InMemorySecretStore},
    modules::automation::model::{Automation, AutomationTarget, Schedule},
    safety::ControlSession,
    AppServer,
};

struct Fixture {
    server: Arc<AppServer>,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("yilian-delete-lifecycle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let server = Arc::new(
            AppServer::new_with_control_session_and_store(
                Path::new(":memory:"),
                root.to_str().unwrap(),
                ControlSession::new("a".repeat(64)).unwrap(),
                Arc::new(InMemorySecretStore::new()),
            )
            .unwrap(),
        );
        Self { server, root }
    }

    fn conversation(&self, title: &str) -> String {
        let id = self.server.db.create_conversation(title).unwrap().id;
        self.server
            .db
            .persist_chat_message(&id, &ChatMessage::user("retain this history"), 1)
            .unwrap();
        id
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // The fixture creates an empty root; successful cancellation writes no file.
        let _ = std::fs::remove_dir(&self.root);
    }
}

fn request(method: &str, path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("X-Yilian-Control-Session", "a".repeat(64))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn call(server: &Arc<AppServer>, method: &str, path: &str, body: Value) -> (u16, Value) {
    let response = yilian_backend::app::router::build_router(server.clone())
        .oneshot(request(method, path, body))
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn running_delete_preserves_history_until_explicit_stop_drains_without_file_write() {
    let fixture = Fixture::new();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let output = fixture.root.join("pending-write.txt");
    let provider = Router::new().route(
        "/chat/completions",
        post({
            let entered = entered.clone();
            let release = release.clone();
            let calls = calls.clone();
            let output = output.clone();
            move || {
                let entered = entered.clone();
                let release = release.clone();
                let calls = calls.clone();
                let output = output.clone();
                async move {
                    let index = calls.fetch_add(1, Ordering::SeqCst);
                    let chunk = if index == 0 {
                        entered.notify_one();
                        release.notified().await;
                        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"pending-write","type":"function","function":{"name":"write_file","arguments":json!({"path":output.to_string_lossy(),"content":"isolated regression output"}).to_string()}}]},"finish_reason":"tool_calls"}]})
                    } else {
                        json!({"choices":[{"index":0,"delta":{"content":"done"},"finish_reason":"stop"}]})
                    };
                    (
                        [("Content-Type", "text/event-stream")],
                        format!("data: {chunk}\n\ndata: [DONE]\n\n"),
                    )
                        .into_response()
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let provider_task = tokio::spawn(async move { axum::serve(listener, provider).await.unwrap() });
    {
        let mut config = fixture.server.config.write();
        config.model.base_url = format!("http://{address}");
        config.model.api_key = "local-test-placeholder".into();
        config.model.api_key_env.clear();
        config.model.invoke_timeout_ms = 10_000;
        config.permissions.mode = yilian_backend::config::types::PermissionMode::FullAccess;
    }
    let conversation = fixture.conversation("Running deletion");
    let app = yilian_backend::app::router::build_router(fixture.server.clone());
    let response = app
        .oneshot(request("POST", "/api/chat", json!({"conversation_id":conversation,"message":"write the isolated regression output"})))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    tokio::time::timeout(Duration::from_secs(10), entered.notified())
        .await
        .unwrap();
    let token = fixture.server.active_tasks.lock()[&conversation].clone();
    let before = fixture.server.db.get_conversation(&conversation).unwrap();
    let uri = format!("/api/conversations/{conversation}");
    let deletion = call(&fixture.server, "DELETE", &uri, Value::Null).await;
    let retained = fixture.server.db.find_conversation(&conversation).unwrap();
    let cancelled_by_delete = token.is_cancelled();

    // Always settle the blocked request, including on the pre-fix RED run.
    let stop = call(
        &fixture.server,
        "POST",
        "/api/chat/stop",
        json!({"conversation_id":conversation}),
    )
    .await;
    release.notify_one();
    let stream = tokio::time::timeout(
        Duration::from_secs(10),
        to_bytes(response.into_body(), 1_000_000),
    )
    .await
    .unwrap()
    .unwrap();
    let final_conversation = fixture.server.db.find_conversation(&conversation).unwrap();
    let final_delete = call(&fixture.server, "DELETE", &uri, Value::Null).await;
    provider_task.abort();

    assert_eq!(deletion.0, 409, "{deletion:?}");
    assert_eq!(deletion.1["code"], "conversation_turn_active");
    let retained = retained.expect("DELETE must retain the executing conversation");
    assert_eq!(json!(retained.messages), json!(before.messages));
    assert_eq!(retained.run_status, ConversationRunStatus::Running);
    assert!(!cancelled_by_delete, "only explicit STOP cancels execution");
    assert_eq!(stop, (200, json!({"status":"cancelled"})));
    assert!(token.is_cancelled());
    assert_eq!(
        final_conversation.unwrap().run_status,
        ConversationRunStatus::Cancelled
    );
    assert!(!fixture
        .server
        .active_tasks
        .lock()
        .contains_key(&conversation));
    assert!(
        !output.exists(),
        "pending tool wrote after STOP: {}",
        String::from_utf8_lossy(&stream)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(final_delete, (200, json!({"status":"deleted"})));
    assert!(fixture
        .server
        .db
        .find_conversation(&conversation)
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn preparing_turn_gate_rejects_delete_before_running_status_is_recorded() {
    let fixture = Fixture::new();
    let conversation = fixture.conversation("Preparing");
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    fixture
        .server
        .chat_turn_gates
        .lock()
        .insert(conversation.clone(), gate.clone());
    let held = gate.lock_owned().await;
    let uri = format!("/api/conversations/{conversation}");
    let deletion = call(&fixture.server, "DELETE", &uri, Value::Null).await;
    assert_eq!(deletion.0, 409, "{deletion:?}");
    assert_eq!(
        fixture
            .server
            .db
            .get_conversation(&conversation)
            .unwrap()
            .messages
            .len(),
        1
    );
    drop(held);
    assert_eq!(
        call(&fixture.server, "DELETE", &uri, Value::Null).await.0,
        200
    );
}

#[tokio::test]
async fn active_task_rejects_delete_even_before_persisted_running_status() {
    let fixture = Fixture::new();
    let conversation = fixture.conversation("Registered task");
    let token = CancellationToken::new();
    fixture
        .server
        .active_tasks
        .lock()
        .insert(conversation.clone(), token.clone());
    let deletion = call(
        &fixture.server,
        "DELETE",
        &format!("/api/conversations/{conversation}"),
        Value::Null,
    )
    .await;
    assert_eq!(deletion.0, 409, "{deletion:?}");
    assert_eq!(
        fixture
            .server
            .db
            .get_conversation(&conversation)
            .unwrap()
            .messages
            .len(),
        1
    );
    assert!(!token.is_cancelled());
}

#[tokio::test]
async fn persisted_running_and_waiting_states_preserve_history_without_active_task() {
    let fixture = Fixture::new();
    for status in [
        ConversationRunStatus::Running,
        ConversationRunStatus::WaitingApproval,
        ConversationRunStatus::WaitingInput,
    ] {
        let conversation = fixture.conversation(status.as_str());
        fixture
            .server
            .db
            .set_conversation_run_status(&conversation, status, None)
            .unwrap();
        let deletion = call(
            &fixture.server,
            "DELETE",
            &format!("/api/conversations/{conversation}"),
            Value::Null,
        )
        .await;
        assert_eq!(deletion.0, 409, "{status:?}: {deletion:?}");
        let retained = fixture.server.db.get_conversation(&conversation).unwrap();
        assert_eq!(retained.run_status, status);
        assert_eq!(retained.messages[0].content, "retain this history");
    }
}

#[tokio::test]
async fn settled_conversations_delete_with_their_history() {
    let fixture = Fixture::new();
    for status in [
        ConversationRunStatus::Idle,
        ConversationRunStatus::Completed,
        ConversationRunStatus::Failed,
        ConversationRunStatus::Cancelled,
        ConversationRunStatus::Interrupted,
    ] {
        let conversation = fixture.conversation(status.as_str());
        fixture
            .server
            .db
            .set_conversation_run_status(&conversation, status, None)
            .unwrap();
        let uri = format!("/api/conversations/{conversation}");
        assert_eq!(
            call(&fixture.server, "DELETE", &uri, Value::Null).await,
            (200, json!({"status":"deleted"}))
        );
        assert!(fixture
            .server
            .db
            .find_conversation(&conversation)
            .unwrap()
            .is_none());
        let count: i64 = fixture
            .server
            .db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE conversation_id=?1",
                [&conversation],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn settled_personal_delegation_still_requires_deleting_its_automation_first() {
    let fixture = Fixture::new();
    let conversation = fixture.conversation("Personal delegation");
    fixture
        .server
        .db
        .save_automation(
            &Automation {
                attention: None,
                id: "bound-delegation".into(),
                revision: 1,
                name: "Controlled delegation".into(),
                workflow_graph_id: String::new(),
                target: Some(AutomationTarget::PersonalAssistant {
                    goal: "Controlled goal".into(),
                    conversation_id: conversation.clone(),
                    root_path: String::new(),
                    allow_file_writes: false,
                    allow_file_discovery: false,
                    max_runs: 1,
                }),
                workspace_id: String::new(),
                timezone: "UTC".into(),
                schedule: Schedule::Daily {
                    time: "09:00".into(),
                },
                enabled: false,
                next_run_at: None,
                created_at: 1,
                updated_at: 1,
                file_fingerprint: None,
                archived: false,
            },
            true,
        )
        .unwrap();
    let deletion = call(
        &fixture.server,
        "DELETE",
        &format!("/api/conversations/{conversation}"),
        Value::Null,
    )
    .await;
    assert_eq!(deletion.0, 409);
    assert!(deletion.1["error"].as_str().unwrap().contains("个人委托"));
    assert_eq!(
        fixture
            .server
            .db
            .get_conversation(&conversation)
            .unwrap()
            .messages
            .len(),
        1
    );
    assert!(fixture
        .server
        .db
        .get_automation("bound-delegation")
        .unwrap()
        .is_some());
}
