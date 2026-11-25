use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

use crate::{
    app::state::AppServer,
    db::MessageRow,
    integrations::secret::InMemorySecretStore,
    safety::{ControlSession, CONTROL_SESSION_HEADER},
};

struct Fixture {
    server: Arc<AppServer>,
    token: ControlSession,
}

impl Fixture {
    fn new(root: &std::path::Path) -> Self {
        std::fs::create_dir_all(root).unwrap();
        let token = ControlSession::generate();
        let server = AppServer::new_with_control_session_and_store(
            &root.join("isolated.db"),
            root.to_str().unwrap(),
            token.clone(),
            Arc::new(InMemorySecretStore::new()),
        )
        .unwrap();
        Self {
            server: Arc::new(server),
            token,
        }
    }

    async fn request(
        &self,
        method: Method,
        uri: &str,
        body: Value,
        authenticated: bool,
    ) -> (StatusCode, String) {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header(CONTENT_TYPE, "application/json");
        if authenticated {
            request = request.header(CONTROL_SESSION_HEADER, self.token.token());
        }
        let response = crate::api::build_router(Arc::clone(&self.server))
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    async fn json(&self, method: Method, uri: &str, body: Value) -> Value {
        let (status, response) = self.request(method, uri, body, true).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {response}");
        serde_json::from_str(&response).unwrap()
    }
}

#[tokio::test]
async fn integration_companion_feedback_only_reaches_model_after_review_and_restores_new_revision()
{
    let root = std::env::temp_dir().join(format!(
        "yilian-companion-feedback-{}",
        uuid::Uuid::new_v4()
    ));
    let app = Fixture::new(&root);
    let prompts = Arc::new(Mutex::new(Vec::<String>::new()));
    let model = Router::new().route("/chat/completions", post({
        let prompts = Arc::clone(&prompts);
        move |Json(body): Json<Value>| {
            let prompts = Arc::clone(&prompts);
            async move {
                if body["stream"] == true {
                    let system = body["messages"].as_array().unwrap().iter()
                        .filter(|message| message["role"] == "system")
                        .filter_map(|message| message["content"].as_str())
                        .collect::<Vec<_>>().join("\n");
                    prompts.lock().unwrap().push(system);
                    let chunk = json!({"id":"fixture-companion", "choices":[{"index":0,"delta":{"content":"Fixture answer"},"finish_reason":"stop"}],"usage":{"prompt_tokens":20,"completion_tokens":5,"total_tokens":25}});
                    ([(CONTENT_TYPE, "text/event-stream")], format!("data: {chunk}\n\ndata: [DONE]\n\n")).into_response()
                } else {
                    Json(json!({"id":"fixture-intent","model":"fixture-companion","choices":[{"index":0,"message":{"role":"assistant","content":"{}"},"finish_reason":"stop"}]})).into_response()
                }
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let model = tokio::spawn(async move { axum::serve(listener, model).await.unwrap() });
    {
        let mut config = app.server.config.write();
        config.model.provider = "lmstudio".into();
        config.model.name = "fixture-companion".into();
        config.model.base_url = format!("http://127.0.0.1:{port}");
        config.model.api_key.clear();
        config.model.api_key_env.clear();
        config.model.api_key_ref = None;
        config.model.embedding_model.clear();
    }
    const FEEDBACK: &str =
        "For contour reports, use the cobalt contour labels from the companion fixture.";
    const CORRECTED: &str =
        "For contour reports, use the amber contour labels from the companion fixture.";
    let source = app
        .server
        .db
        .create_conversation("Companion source")
        .unwrap();
    app.server
        .db
        .accept_conversation_turn(&MessageRow {
            id: "companion-feedback-source".into(),
            conversation_id: source.id.clone(),
            role: "user".into(),
            content: FEEDBACK.into(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at: chrono::Utc::now().timestamp_millis(),
        })
        .unwrap();
    let staged = app
        .json(
            Method::POST,
            "/api/memories/candidates/feedback",
            json!({"activity_id":"message:companion-feedback-source"}),
        )
        .await;
    let candidate = staged["candidate_ids"][0].as_str().unwrap();
    let list = app
        .json(Method::GET, "/api/memories/candidates", Value::Null)
        .await;
    assert_eq!(
        list["entries"][0]["source_message_id"],
        "companion-feedback-source"
    );

    async fn turn(app: &Fixture, prompts: &Mutex<Vec<String>>) -> (String, Value) {
        let before = prompts.lock().unwrap().len();
        let conversation = app
            .server
            .db
            .create_conversation("Companion target")
            .unwrap();
        let (status, body) = app.request(Method::POST, "/api/chat", json!({"conversation_id":conversation.id,"workspace_mode":"chat","message":"How should I format contour labels for contour reports?"}), true).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body.contains("Fixture answer"), "{body}");
        let prompt = prompts
            .lock()
            .unwrap()
            .iter()
            .skip(before)
            .last()
            .expect("fixture received a streaming chat request")
            .clone();
        let context = app
            .json(
                Method::GET,
                &format!("/api/conversations/{}/context-manifest", conversation.id),
                Value::Null,
            )
            .await;
        (prompt, context)
    }
    let has_memory = |context: &Value, id: &str| {
        context["manifest"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| {
                item["kind"] == "memory"
                    && item["source_id"] == id
                    && item["chars"].as_u64().unwrap_or(0) > 0
            })
    };

    let (pending, context) = turn(&app, &prompts).await;
    assert!(!pending.contains(FEEDBACK) && !has_memory(&context, candidate));
    let saved = app
        .json(
            Method::POST,
            &format!("/api/memories/candidates/{candidate}/accept"),
            json!({"content":FEEDBACK,"category":"preference","scope":"profile"}),
        )
        .await;
    let memory_id = saved["id"].as_str().unwrap();
    let memory_path = format!("/api/memories/{memory_id}");
    let (adopted, context) = turn(&app, &prompts).await;
    assert!(adopted.contains(FEEDBACK) && has_memory(&context, memory_id));

    app.json(
        Method::POST,
        &format!("{memory_path}/correct"),
        json!({"content":CORRECTED,"reason":"fixture correction","expected_revision":1,"source_ref":"later-different-evidence"}),
    )
    .await;
    app.json(
        Method::POST,
        &format!("{memory_path}/withdraw"),
        json!({"reason":"fixture withdrawal","expected_revision":2}),
    )
    .await;
    let (withdrawn, context) = turn(&app, &prompts).await;
    assert!(
        !withdrawn.contains(FEEDBACK)
            && !withdrawn.contains(CORRECTED)
            && !has_memory(&context, memory_id)
    );

    let restored = app
        .json(
            Method::POST,
            &format!("{memory_path}/restore"),
            json!({"version_id":format!("{memory_id}:1"),"expected_revision":3}),
        )
        .await;
    assert_eq!(restored["content"], FEEDBACK);
    let history = app
        .json(Method::GET, &format!("{memory_path}/history"), Value::Null)
        .await;
    assert_eq!(history.as_array().unwrap().len(), 4);
    assert_eq!(history[0]["revision"], 4);
    assert_eq!(history[1]["status"], "withdrawn");
    assert_eq!(history[2]["content"], CORRECTED);
    assert_eq!(history[3]["content"], FEEDBACK);
    assert_eq!(history[0]["source_ref"], source.id);
    let (restored, context) = turn(&app, &prompts).await;
    assert!(
        restored.contains(FEEDBACK)
            && !restored.contains(CORRECTED)
            && has_memory(&context, memory_id)
    );

    model.abort();
    drop(app);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn integration_companion_feedback_rejects_automatic_input_with_real_followup_available() {
    let root = std::env::temp_dir().join(format!(
        "yilian-companion-auto-source-{}",
        uuid::Uuid::new_v4()
    ));
    let app = Fixture::new(&root);
    let source = app
        .server
        .db
        .create_conversation("Delegated source")
        .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    for (offset, (id, text)) in [
        ("automatic-input", "Automated run instruction"),
        (
            "real-feedback",
            "Please use a short summary in my later reports.",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        app.server
            .db
            .accept_conversation_turn(&MessageRow {
                id: id.into(),
                conversation_id: source.id.clone(),
                role: "user".into(),
                content: text.into(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_result: None,
                created_at: now + offset as i64,
            })
            .unwrap();
    }
    app.server.db.conn().execute("INSERT INTO automation_occurrences(id,automation_id,occurrence_key,payload,created_at) VALUES(?1,?2,?3,?4,?5)",
        rusqlite::params!["fixture-occurrence", "fixture-automation", "fixture-round", json!({"conversation_id":source.id,"assistant_turn_id":"automatic-input","status":"completed"}).to_string(), now]).unwrap();
    let activity = app
        .json(
            Method::GET,
            "/api/memories/activity?recent=true",
            Value::Null,
        )
        .await;
    let entry = |id: &str| {
        activity["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == id)
            .unwrap()
    };
    assert_eq!(
        entry("message:automatic-input")["origin"],
        "automation_message"
    );
    assert_eq!(entry("message:real-feedback")["origin"], "user_message");
    let stage = "/api/memories/candidates/feedback";
    let rejected = app
        .request(
            Method::POST,
            stage,
            json!({"activity_id":"message:automatic-input"}),
            true,
        )
        .await;
    assert_eq!(rejected.0, StatusCode::NOT_FOUND);
    let accepted = app
        .json(
            Method::POST,
            stage,
            json!({"activity_id":"message:real-feedback"}),
        )
        .await;
    assert_eq!(accepted["candidate_ids"].as_array().unwrap().len(), 1);
    drop(app);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn integration_companion_pet_projection_only_exposes_display_fields() {
    use base64::Engine;

    let root =
        std::env::temp_dir().join(format!("yilian-companion-display-{}", uuid::Uuid::new_v4()));
    let app = Fixture::new(&root);
    let default = app
        .json(Method::GET, "/api/companion/display", Value::Null)
        .await;
    assert_eq!(default, json!({"display_name":"小涟", "avatar":null}));
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aHSsAAAAASUVORK5CYII=";
    let image = app
        .server
        .resource_service
        .ingest(
            "fixture.png",
            "image/png",
            &base64::engine::general_purpose::STANDARD
                .decode(PNG)
                .unwrap(),
            json!({}),
        )
        .unwrap();
    let mut events = app.server.event_hub.subscribe();
    app.json(Method::PUT, "/api/agents/default/profile", json!({"user_prompt":"Private fixture notes","persona_enabled":true,"persona":{"display_name":"轮廓伙伴","avatar_resource_id":image.id}})).await;
    let changed = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(changed.event_type, "companion.profile_changed");
    assert!(changed.payload.is_null());
    let unauthenticated = app
        .request(Method::GET, "/api/companion/display", Value::Null, false)
        .await;
    assert!(matches!(
        unauthenticated.0,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ));
    let display = app
        .json(Method::GET, "/api/companion/display", Value::Null)
        .await;
    assert_eq!(
        display,
        json!({"display_name":"轮廓伙伴","avatar":format!("data:image/png;base64,{PNG}")})
    );
    app.json(Method::DELETE, "/api/agents/default/profile", Value::Null)
        .await;
    let changed = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(changed.event_type, "companion.profile_changed");
    assert!(changed.payload.is_null());
    drop(app);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn integration_companion_restore_can_clear_a_later_source_reference() {
    let root = std::env::temp_dir().join(format!(
        "yilian-companion-restore-source-{}",
        uuid::Uuid::new_v4()
    ));
    let app = Fixture::new(&root);
    let memory = app
        .server
        .db
        .create_memory(&crate::db::CreateMemoryRequest {
            content: "An earlier version without a source reference".into(),
            category: "note".into(),
            source: "manual".into(),
            source_conversation_id: None,
            metadata: None,
        })
        .unwrap();
    let path = format!("/api/memories/{}", memory.id);
    app.json(Method::POST, &format!("{path}/correct"), json!({"content":"New evidence in a later revision","reason":"fixture update","expected_revision":1,"source_ref":"different-source"})).await;
    app.json(
        Method::POST,
        &format!("{path}/restore"),
        json!({"version_id":format!("{}:1",memory.id),"expected_revision":2}),
    )
    .await;
    let history = app
        .json(Method::GET, &format!("{path}/history"), Value::Null)
        .await;
    assert_eq!(history[0]["revision"], 3);
    assert!(history[0]["source_ref"].is_null());
    assert_eq!(history[1]["source_ref"], "different-source");
    drop(app);
    std::fs::remove_dir_all(root).unwrap();
}
