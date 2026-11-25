use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Json, Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::{
    api::chat::runtime,
    app::state::AppServer,
    db::{
        connectors::ConnectorBinding,
        model_pools::{ModelPoolInput, ModelPoolTier},
        LlmModelInput,
    },
    integrations::{
        llm::types::ChatMessage,
        secret::{InMemorySecretStore, SecretRef, SecretStore},
    },
    safety::ControlSession,
};

pub(super) const TOKEN: &str = "workspace-chat-settings-control-session-0000000";

pub(super) async fn request(
    router: &Router,
    method: &str,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("X-Yilian-Control-Session", TOKEN)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let raw = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let value = serde_json::from_slice(&raw).unwrap();
    (status, value)
}

async fn invoke(server: &Arc<AppServer>, conversation: &str, selected: Option<&str>) -> String {
    let prepared = runtime::prepare_with_model(server, Some(conversation), None, selected)
        .await
        .unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    runtime::client(server, &prepared, conversation, &tx)
        .unwrap()
        .invoke(&[ChatMessage::user("fixture")], &[])
        .await
        .unwrap()
        .choices[0]
        .message
        .content
        .clone()
        .unwrap()
}

#[tokio::test]
async fn global_settings_revision_receipt_matches_saved_snapshot_and_conflicts_before_key_write() {
    let root = std::env::temp_dir().join(format!("settings-revision-{}", uuid::Uuid::new_v4()));
    let store = Arc::new(InMemorySecretStore::new());
    let open = || {
        Arc::new(
            AppServer::new_with_control_session_and_store(
                &root.join("test.db"),
                root.to_str().unwrap(),
                ControlSession::new(TOKEN).unwrap(),
                store.clone(),
            )
            .unwrap(),
        )
    };
    let server = open();
    let router = crate::app::router::build_router(server.clone());
    let (status, snapshot) = request(&router, "GET", "/api/settings", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    let revision = snapshot["revision"].as_u64().expect("global revision");
    let (_, workspace) = request(
        &router,
        "POST",
        "/api/workspaces",
        json!({"name":"defaults"}),
    )
    .await;
    let workspace_id = workspace["id"].as_str().unwrap();
    let (_, chat) = request(
        &router,
        "GET",
        &format!("/api/workspaces/{workspace_id}/service-settings/chat"),
        Value::Null,
    )
    .await;
    assert_eq!(
        chat["inherited"],
        json!({
            "source": "global_config", "id":"app_config", "label": snapshot["model"]["name"],
            "scope":"global", "revision":revision
        })
    );
    let mut change = snapshot.clone();
    change["expected_revision"] = json!(revision);
    change["model"]["name"] = json!("fixture-model-a");
    change["model"]["api_key"] = json!("fixture-key-a");
    let mut competing = change.clone();
    competing["model"]["name"] = json!("fixture-model-b");
    competing["model"]["api_key"] = json!("fixture-key-b");
    let (first, second) = tokio::join!(
        request(&router, "PUT", "/api/settings", change),
        request(&router, "PUT", "/api/settings", competing)
    );
    let ((saved_status, saved), (conflict_status, conflict), winner) = if first.0 == StatusCode::OK
    {
        (first, second, "a")
    } else {
        (second, first, "b")
    };
    assert_eq!(saved_status, StatusCode::OK, "{saved}");
    assert_eq!(conflict_status, StatusCode::CONFLICT, "{conflict}");
    assert_eq!(saved["status"], "saved");
    assert_eq!(saved["application"], "next_read");
    assert_eq!(saved["revision"], revision + 1);
    assert_eq!(conflict["code"], "revision_conflict");
    assert_eq!(conflict["revision"], saved["revision"]);
    assert_eq!(
        server.config.read().model.name,
        format!("fixture-model-{winner}")
    );
    let reference = SecretRef::new(crate::integrations::secret::CHAT_KEY_REF);
    let key = store.get(&reference).await.unwrap().unwrap();
    assert_eq!(
        secrecy::ExposeSecret::expose_secret(&key),
        format!("fixture-key-{winner}")
    );
    let (_, current) = request(&router, "GET", "/api/settings", Value::Null).await;
    assert_eq!(current["revision"], saved["revision"]);
    assert_eq!(current["model"]["api_key"], "");
    let (status, response) = request(
        &router,
        "PUT",
        "/api/security/approval-mode",
        json!({"mode":"full_approval"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let (_, current) = request(&router, "GET", "/api/settings", Value::Null).await;
    assert_eq!(current["revision"], revision + 2);
    drop(router);
    drop(server);

    let restarted = open();
    let router = crate::app::router::build_router(restarted.clone());
    let (_, current) = request(&router, "GET", "/api/settings", Value::Null).await;
    assert_eq!(current["revision"], revision + 2);
    assert_eq!(current["model"]["name"], format!("fixture-model-{winner}"));
    assert_eq!(current["permissions"]["mode"], "full_approval");
}

#[tokio::test]
async fn workspace_chat_schema_migration_keeps_existing_vision_override_and_revision() {
    let root = std::env::temp_dir().join(format!("workspace-chat-legacy-{}", uuid::Uuid::new_v4()));
    let open = || {
        Arc::new(
            AppServer::new_with_control_session_and_store(
                &root.join("test.db"),
                root.to_str().unwrap(),
                ControlSession::new(TOKEN).unwrap(),
                Arc::new(InMemorySecretStore::new()),
            )
            .unwrap(),
        )
    };
    let server = open();
    let router = crate::app::router::build_router(server.clone());
    let (_, workspace) = request(
        &router,
        "POST",
        "/api/workspaces",
        json!({"name":"migrated"}),
    )
    .await;
    let workspace_id = workspace["id"].as_str().unwrap();
    {
        let conn = server.db.conn();
        conn.execute_batch(
            "DROP TABLE workspace_service_overrides;
             CREATE TABLE workspace_service_overrides (
                workspace_id TEXT NOT NULL REFERENCES workspaces(id),
                service_kind TEXT NOT NULL CHECK(service_kind IN ('vision', 'image')),
                source TEXT NOT NULL CHECK(source IN ('profile', 'pool')),
                target_id TEXT NOT NULL,
                PRIMARY KEY (workspace_id, service_kind)
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace_service_revisions VALUES (?1, 7)",
            [workspace_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace_service_overrides VALUES (?1, 'vision', 'profile', 'legacy')",
            [workspace_id],
        )
        .unwrap();
    }
    drop(router);
    drop(server);
    let restarted = open();
    let router = crate::app::router::build_router(restarted.clone());
    let path = format!("/api/workspaces/{workspace_id}/service-settings");
    let (_, vision) = request(&router, "GET", &format!("{path}/vision"), Value::Null).await;
    assert_eq!(vision["revision"], 7);
    assert_eq!(vision["override"]["id"], "legacy");
    restarted
        .db
        .conn()
        .execute(
            "INSERT INTO workspace_service_overrides VALUES (?1, 'chat', 'profile', 'new-chat')",
            [workspace_id],
        )
        .unwrap();
    let (status, chat) = request(&router, "GET", &format!("{path}/chat"), Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{chat}");
    assert_eq!(chat["revision"], 7);
    assert_eq!(chat["override"]["id"], "new-chat");
}

#[tokio::test]
async fn workspace_chat_override_routes_actual_turn_clients_and_restores_latest_global() {
    let root = std::env::temp_dir().join(format!("workspace-chat-{}", uuid::Uuid::new_v4()));
    let store = Arc::new(InMemorySecretStore::new());
    store
        .put(
            &SecretRef::new("fixture.chat"),
            secrecy::SecretString::from("fixture-local-only"),
        )
        .await
        .unwrap();
    let open = || {
        Arc::new(
            AppServer::new_with_control_session_and_store(
                &root.join("test.db"),
                root.to_str().unwrap(),
                ControlSession::new(TOKEN).unwrap(),
                store.clone(),
            )
            .unwrap(),
        )
    };
    let server = open();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let provider = tokio::spawn(async move {
        let app = Router::new().route("/chat/completions", axum::routing::post(
            |Json(body): Json<Value>| async move {
                Json(json!({"id":"fixture", "choices":[{"index":0,"message":{"role":"assistant","content":body["model"]},"finish_reason":"stop"}]}))
            }
        ));
        axum::serve(listener, app).await.unwrap();
    });
    for id in [
        "global-first",
        "global-next",
        "workspace-chat",
        "explicit-turn",
        "agent-model",
    ] {
        server
            .db
            .create_llm_model(&LlmModelInput {
                id: id.into(),
                provider: "fixture".into(),
                label: id.into(),
                model: id.into(),
                base_url: format!("http://{address}"),
                api_format: "openai".into(),
                api_key_ref: "fixture.chat".into(),
                api_key_env: String::new(),
                temperature: 0.2,
                max_tokens: 32,
                invoke_timeout_ms: 2000,
            })
            .unwrap();
    }
    server.db.activate_llm_model("global-first").unwrap();
    let router = crate::app::router::build_router(server.clone());
    let (status, workspace) = request(
        &router,
        "POST",
        "/api/workspaces",
        json!({"name":"chat scope"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let workspace_id = workspace["id"].as_str().unwrap();
    let path = format!("/api/workspaces/{workspace_id}/service-settings/chat");
    let conversation = server.db.create_conversation("scoped").unwrap();
    server
        .db
        .set_conversation_workspace(&conversation.id, Some(workspace_id))
        .unwrap();
    let other = server.db.create_conversation("global").unwrap();
    let (status, inherited) = request(&router, "GET", &path, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{inherited}");
    assert_eq!(inherited["effective"]["id"], "global-first");
    let (status, saved) = request(
        &router,
        "PUT",
        &path,
        json!({"expected_revision":0,"override":{"source":"profile","id":"workspace-chat"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["effective"]["scope"], "workspace");
    assert_eq!(saved["application"], "next_call");
    assert_eq!(
        runtime::readiness(&server, Some(&conversation.id))
            .await
            .model,
        "workspace-chat"
    );
    assert_eq!(
        invoke(&server, &conversation.id, None).await,
        "workspace-chat"
    );
    assert_eq!(invoke(&server, &other.id, None).await, "global-first");
    assert_eq!(
        invoke(&server, &conversation.id, Some("explicit-turn")).await,
        "explicit-turn"
    );
    assert!(server.db.delete_llm_model("workspace-chat").is_err());
    server.db.activate_llm_model("global-next").unwrap();
    assert_eq!(
        invoke(&server, &conversation.id, None).await,
        "workspace-chat"
    );
    assert_eq!(invoke(&server, &other.id, None).await, "global-next");
    let (status, _) = request(
        &router,
        "PUT",
        &path,
        json!({"expected_revision":0,"override":null}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (_, listing) = request(
        &router,
        "GET",
        "/api/settings/service-overrides",
        Value::Null,
    )
    .await;
    assert_eq!(listing["workspaces"][0]["kinds"], json!(["chat"]));
    drop(router);
    drop(server);
    let restarted = open();
    let router = crate::app::router::build_router(restarted.clone());
    let (_, persisted) = request(&router, "GET", &path, Value::Null).await;
    assert_eq!(persisted["effective"]["id"], "workspace-chat");
    let (status, reset) = request(
        &router,
        "PUT",
        &path,
        json!({"expected_revision":1,"override":null}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reset}");
    assert_eq!(reset["revision"], 2);
    assert_eq!(reset["effective"]["id"], "global-next");
    assert_eq!(
        invoke(&restarted, &conversation.id, None).await,
        "global-next"
    );

    let pool = restarted
        .db
        .save_model_pool(
            "chat-pool",
            ModelPoolInput {
                label: "Fixture chat pool".into(),
                service_kind: "chat".into(),
                enabled: true,
                attempt_timeout_ms: 2000,
                total_timeout_ms: 4000,
                tiers: vec![ModelPoolTier {
                    id: "first".into(),
                    label: "First".into(),
                    profile_ids: vec!["workspace-chat".into()],
                }],
                expected_revision: None,
            },
        )
        .unwrap();
    let (status, saved) = request(
        &router,
        "PUT",
        &path,
        json!({"expected_revision":2,"override":{"source":"pool","id":pool.id}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["effective"]["source"], "pool");
    assert_eq!(
        runtime::readiness(&restarted, Some(&conversation.id))
            .await
            .model,
        "Fixture chat pool"
    );
    assert_eq!(
        invoke(&restarted, &conversation.id, None).await,
        "workspace-chat"
    );
    assert!(restarted
        .db
        .delete_model_pool(&pool.id, pool.revision)
        .is_err());

    use crate::modules::task::{AgentDefinition, AgentId, AgentSource};
    for (agent_id, model) in [
        ("chat-agent", "agent-model"),
        ("legacy-agent", "bare-agent-model"),
    ] {
        restarted
            .db
            .create_agent_definition(&AgentDefinition {
                id: AgentId::new(agent_id).unwrap(),
                name: "Fixture agent".into(),
                description: String::new(),
                instructions: String::new(),
                allowed_tools: vec![],
                model: Some(model.into()),
                capabilities: vec![],
                max_iterations: 10,
                enabled: true,
                source: AgentSource::Database,
            })
            .unwrap();
        restarted
            .db
            .save_connector_binding(
                agent_id,
                &ConnectorBinding {
                    id: String::new(),
                    kind: "conversation_agent".into(),
                    source_id: format!("conversation:{}", conversation.id),
                    target_id: format!("agent:{agent_id}"),
                    service_kind: None,
                    model_source: None,
                },
            )
            .unwrap();
        assert_eq!(
            runtime::readiness(&restarted, Some(&conversation.id))
                .await
                .model,
            model
        );
        assert_eq!(invoke(&restarted, &conversation.id, None).await, model);
    }
    assert_eq!(
        invoke(&restarted, &conversation.id, Some("explicit-turn")).await,
        "explicit-turn"
    );
    restarted
        .db
        .set_model_pool_binding("default", Some(&pool.id))
        .unwrap();
    assert_eq!(
        invoke(&restarted, &conversation.id, None).await,
        "bare-agent-model"
    );
    let (status, inherited_pool) = request(
        &router,
        "PUT",
        &path,
        json!({"expected_revision":3,"override":null}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{inherited_pool}");
    assert_eq!(inherited_pool["effective"]["id"], pool.id);
    assert_eq!(
        invoke(&restarted, &conversation.id, None).await,
        "workspace-chat"
    );
    provider.abort();
}
