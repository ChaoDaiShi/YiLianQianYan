use axum::{
    body::{to_bytes, Body},
    http::Request,
    response::IntoResponse,
    routing::post,
    Json, Router,
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
use tokio::{
    sync::{Mutex, Notify},
    task::JoinHandle,
};
use tower::ServiceExt;
use yilian_backend::{
    db::{ConversationRunStatus, MessageRow},
    integrations::secret::InMemorySecretStore,
    safety::ControlSession,
    AppServer,
};

struct Fixture {
    server: Arc<AppServer>,
    root: PathBuf,
    entered: Arc<Notify>,
    release: Arc<Notify>,
    calls: Arc<AtomicUsize>,
    messages: Arc<Mutex<Vec<String>>>,
    provider: JoinHandle<()>,
}

impl Fixture {
    async fn new(block_embedding: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("yilian-preparation-{}", uuid::Uuid::new_v4()));
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
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let messages = Arc::new(Mutex::new(Vec::new()));
        let provider = Router::new()
            .route("/embeddings", post({
                let entered = entered.clone();
                let release = release.clone();
                move || {
                    let entered = entered.clone();
                    let release = release.clone();
                    async move {
                        entered.notify_one();
                        release.notified().await;
                        Json(json!({"data":[{"embedding":[0.1,0.2]}]}))
                    }
                }
            }))
            .route("/chat/completions", post({
                let calls = calls.clone();
                let messages = messages.clone();
                move |Json(body): Json<Value>| {
                    let calls = calls.clone();
                    let messages = messages.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        let last = body["messages"].as_array().unwrap().iter().rev()
                            .find(|m| m["role"] == "user").unwrap()["content"].as_str().unwrap().to_owned();
                        messages.lock().await.push(last);
                        ([("Content-Type", "text/event-stream")],
                            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n")
                            .into_response()
                    }
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let provider = tokio::spawn(async move { axum::serve(listener, provider).await.unwrap() });
        {
            let mut config = server.config.write();
            config.model.base_url = format!("http://{address}");
            config.model.api_key = "local-test-placeholder".into();
            config.model.api_key_env.clear();
            config.model.invoke_timeout_ms = 10_000;
            if block_embedding {
                config.model.embedding_base_url = format!("http://{address}");
                config.model.embedding_model = "local-test-embedding".into();
                config.model.embedding_api_key = "local-test-placeholder".into();
            }
        }
        Self {
            server,
            root,
            entered,
            release,
            calls,
            messages,
            provider,
        }
    }

    fn conversation(&self) -> String {
        let id = self
            .server
            .db
            .create_conversation("original completed turn")
            .unwrap()
            .id;
        self.server
            .db
            .accept_conversation_turn(&MessageRow {
                id: uuid::Uuid::new_v4().to_string(),
                conversation_id: id.clone(),
                role: "user".into(),
                content: "previous accepted message".into(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_result: None,
                created_at: 1,
            })
            .unwrap();
        self.server
            .db
            .set_conversation_run_status(&id, ConversationRunStatus::Completed, None)
            .unwrap();
        id
    }

    fn snapshot(&self, id: &str) -> Value {
        json!({"conversation":self.server.db.get_conversation(id).unwrap(),
            "turns":self.server.db.conversation_turns(id).unwrap()})
    }

    fn assert_clean(&self, id: &str) {
        assert!(
            !self.server.active_tasks.lock().contains_key(id),
            "preparation token leaked"
        );
        let gate = self.server.chat_turn_gates.lock()[id].clone();
        assert!(gate.try_lock_owned().is_ok(), "preparation gate leaked");
    }

    fn disable_embedding(&self) {
        self.server.config.write().model.embedding_model.clear();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.release.notify_waiters();
        self.provider.abort();
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

async fn send(server: &Arc<AppServer>, id: &str, message: &str) -> u16 {
    let response = yilian_backend::app::router::build_router(server.clone())
        .oneshot(request(
            "POST",
            "/api/chat",
            json!({"conversation_id":id,"message":message}),
        ))
        .await
        .unwrap();
    let status = response.status().as_u16();
    let _ = to_bytes(response.into_body(), 1_000_000).await.unwrap();
    status
}

#[tokio::test]
async fn stop_during_embedding_returns_before_provider_release_without_accepting_a_turn() {
    let fixture = Fixture::new(true).await;
    let id = fixture.conversation();
    let before = fixture.snapshot(&id);
    let app = yilian_backend::app::router::build_router(fixture.server.clone());
    let chat = tokio::spawn(app.oneshot(request(
        "POST",
        "/api/chat",
        json!({"conversation_id":id,"message":"cancel this preparation"}),
    )));
    tokio::time::timeout(Duration::from_secs(10), fixture.entered.notified())
        .await
        .unwrap();
    let deletion = call(
        &fixture.server,
        "DELETE",
        &format!("/api/conversations/{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(deletion.0, 409);
    assert_eq!(fixture.snapshot(&id), before);
    let token = fixture.server.active_tasks.lock().get(&id).cloned();
    let stop = tokio::time::timeout(
        Duration::from_secs(2),
        call(
            &fixture.server,
            "POST",
            "/api/chat/stop",
            json!({"conversation_id":id}),
        ),
    )
    .await;
    assert!(
        stop.is_ok(),
        "STOP waited for blocked embedding instead of cancelling preparation"
    );
    assert_eq!(stop.unwrap(), (200, json!({"status":"cancelled"})));
    assert!(token
        .expect("preparing owner must be visible to STOP")
        .is_cancelled());
    let response = tokio::time::timeout(Duration::from_secs(2), chat)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), 409);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 100_000).await.unwrap()).unwrap();
    assert_eq!(body["code"], "chat_cancelled");
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        fixture.snapshot(&id),
        before,
        "pre-admission STOP changed prior accepted state"
    );
    fixture.assert_clean(&id);
    fixture.disable_embedding();
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(10),
            send(&fixture.server, &id, "next send")
        )
        .await
        .unwrap(),
        200
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    fixture.assert_clean(&id);
    assert_eq!(
        call(
            &fixture.server,
            "DELETE",
            &format!("/api/conversations/{id}"),
            Value::Null
        )
        .await
        .0,
        200
    );
}

#[tokio::test]
async fn aborting_actual_preparing_handler_removes_registration_and_preserves_prior_state() {
    let fixture = Fixture::new(true).await;
    let id = fixture.conversation();
    let before = fixture.snapshot(&id);
    // The in-process oneshot owns the actual handler future; aborting it drops
    // preparation directly, without assuming a remote HTTP disconnect cancels it.
    let app = yilian_backend::app::router::build_router(fixture.server.clone());
    let chat = tokio::spawn(app.oneshot(request(
        "POST",
        "/api/chat",
        json!({"conversation_id":id,"message":"abort preparation"}),
    )));
    tokio::time::timeout(Duration::from_secs(10), fixture.entered.notified())
        .await
        .unwrap();
    let registered = fixture.server.active_tasks.lock().contains_key(&id);
    chat.abort();
    assert!(chat.await.unwrap_err().is_cancelled());
    fixture.assert_clean(&id);
    assert!(
        registered,
        "preparing handler must publish its cancellation token"
    );
    assert_eq!(fixture.snapshot(&id), before);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    fixture.disable_embedding();
    assert_eq!(send(&fixture.server, &id, "after abort").await, 200);
}

#[tokio::test]
async fn setup_rejection_cleans_registration_without_changing_prior_turn() {
    let fixture = Fixture::new(false).await;
    let id = fixture.conversation();
    let before = fixture.snapshot(&id);
    let rejected = call(&fixture.server, "POST", "/api/chat",
        json!({"conversation_id":id,"message":"invalid model","model_profile_id":"missing-profile"})).await;
    assert_eq!(rejected.0, 400);
    assert_eq!(rejected.1["code"], "connector_runtime_invalid");
    fixture.assert_clean(&id);
    assert_eq!(fixture.snapshot(&id), before);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn invalid_compaction_after_runtime_setup_cleans_registration_and_prior_state() {
    let fixture = Fixture::new(false).await;
    let id = fixture.conversation();
    let before = fixture.snapshot(&id);
    fixture
        .server
        .db
        .conn()
        .execute(
            "INSERT INTO settings(key,value) VALUES(?1,'invalid-json')",
            [format!("conversation_compaction:{id}")],
        )
        .unwrap();
    let rejected = call(
        &fixture.server,
        "POST",
        "/api/chat",
        json!({"conversation_id":id,"message":"invalid history checkpoint"}),
    )
    .await;
    assert_eq!(rejected.0, 500);
    assert_eq!(rejected.1["code"], "context_checkpoint_read_failed");
    fixture.assert_clean(&id);
    assert_eq!(fixture.snapshot(&id), before);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn two_queued_initial_requests_keep_fifo_order_and_both_complete() {
    let fixture = Fixture::new(false).await;
    let id = fixture.conversation();
    let gate = Arc::new(Mutex::new(()));
    fixture
        .server
        .chat_turn_gates
        .lock()
        .insert(id.clone(), gate.clone());
    let held = gate.lock_owned().await;
    let first = send(&fixture.server, &id, "first queued");
    let second = send(&fixture.server, &id, "second queued");
    tokio::pin!(first, second);
    assert!(futures::poll!(first.as_mut()).is_pending());
    assert!(futures::poll!(second.as_mut()).is_pending());
    assert!(!fixture.server.active_tasks.lock().contains_key(&id));
    drop(held);
    let results = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(first, second)
    })
    .await
    .expect("queued requests stalled instead of retaining their FIFO lock futures");
    assert_eq!(results, (200, 200));
    assert_eq!(
        *fixture.messages.lock().await,
        ["first queued", "second queued"]
    );
    assert_eq!(fixture.server.db.conversation_turns(&id).unwrap().len(), 3);
    fixture.assert_clean(&id);
}

#[tokio::test]
async fn deletion_winning_before_queued_admission_returns_404_without_orphan_data() {
    let fixture = Fixture::new(false).await;
    let id = fixture.conversation();
    let gate = Arc::new(Mutex::new(()));
    fixture
        .server
        .chat_turn_gates
        .lock()
        .insert(id.clone(), gate.clone());
    let held = gate.lock_owned().await;
    let chat = call(
        &fixture.server,
        "POST",
        "/api/chat",
        json!({"conversation_id":id,"message":"deleted request"}),
    );
    tokio::pin!(chat);
    assert!(futures::poll!(chat.as_mut()).is_pending());
    // Simulate deletion's existing gate ownership and database operation winning first.
    fixture.server.db.delete_conversation(&id).unwrap();
    drop(held);
    let response = tokio::time::timeout(Duration::from_secs(10), chat)
        .await
        .unwrap();
    assert_eq!(response.0, 404, "{response:?}");
    assert!(fixture.server.db.find_conversation(&id).unwrap().is_none());
    let count: i64 = fixture
        .server
        .db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE conversation_id=?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    fixture.assert_clean(&id);
}

#[tokio::test]
async fn idle_stop_preserves_completed_turn_and_reports_no_active_run() {
    let fixture = Fixture::new(false).await;
    let id = fixture.conversation();
    let before = fixture.snapshot(&id);
    assert_eq!(
        call(
            &fixture.server,
            "POST",
            "/api/chat/stop",
            json!({"conversation_id":id})
        )
        .await,
        (200, json!({"status":"not_found"}))
    );
    assert_eq!(fixture.snapshot(&id), before);
    fixture.assert_clean(&id);
}

#[tokio::test]
async fn idle_stop_still_cancels_pending_input_without_dispatch() {
    let fixture = Fixture::new(false).await;
    let id = fixture
        .server
        .db
        .create_conversation("waiting input")
        .unwrap()
        .id;
    fixture
        .server
        .db
        .accept_conversation_turn(&MessageRow {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: id.clone(),
            role: "user".into(),
            content: "ask a question".into(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at: 1,
        })
        .unwrap();
    let input = fixture
        .server
        .db
        .create_conversation_input(
            &id,
            "ask",
            &yilian_backend::agent::user_input::UserInputRequest {
                title: "Continue?".into(),
                kind: "text".into(),
                options: vec![],
                allow_text: true,
            },
        )
        .unwrap();
    fixture
        .server
        .db
        .set_conversation_run_status(&id, ConversationRunStatus::WaitingInput, None)
        .unwrap();
    assert_eq!(
        call(
            &fixture.server,
            "POST",
            "/api/chat/stop",
            json!({"conversation_id":id})
        )
        .await,
        (200, json!({"status":"cancelled"}))
    );
    assert_eq!(
        fixture
            .server
            .db
            .conversation_input(&input.question_id)
            .unwrap()
            .status,
        "cancelled"
    );
    assert_eq!(
        fixture.server.db.get_conversation(&id).unwrap().run_status,
        ConversationRunStatus::Cancelled
    );
    assert_eq!(
        fixture.server.db.conversation_turns(&id).unwrap()[0].status,
        "cancelled"
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    fixture.assert_clean(&id);
}
