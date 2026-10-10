use axum::{
    body::{to_bytes, Body},
    extract::Query,
    http::{HeaderMap, Request, StatusCode},
    routing::{get, post},
    Json, Router,
};
use parking_lot::Mutex;
use secrecy::SecretString;
use serde_json::json;
use std::{
    collections::HashMap,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tower::ServiceExt;
use yilian_backend::db::Database;
use yilian_backend::integrations::secret::{
    cloud_access_token_ref, InMemorySecretStore, SecretStore,
};
use yilian_backend::modules::cloud::{
    contracts::{PullPage, RemoteRecord, ScopeLease, ScopedDb},
    domain::{NewSyncChange, SyncOperation},
    sync::CloudSyncService,
};
use yilian_backend::{
    api,
    safety::{ControlSession, CONTROL_SESSION_HEADER},
    AppServer,
};

fn memory_db() -> Database {
    Database::new(Path::new(":memory:")).unwrap()
}

fn lease() -> ScopeLease {
    ScopeLease {
        scope_id: "review-scope".into(),
        generation: "review-generation".into(),
    }
}

async fn secrets() -> Arc<dyn SecretStore> {
    let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    store
        .put(
            &cloud_access_token_ref(),
            SecretString::from("synthetic-review-access-value".to_owned()),
        )
        .await
        .unwrap();
    store
}

async fn mock_cloud(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (base, task)
}

#[tokio::test]
async fn first_sync_negotiates_and_persists_epoch() {
    let capabilities_hits = Arc::new(AtomicUsize::new(0));
    let hits = Arc::clone(&capabilities_hits);
    let remote = Router::new()
        .route(
            "/api/v1/sync/capabilities",
            get(move || {
                let hits = Arc::clone(&hits);
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    Json(json!({
                        "sync_protocol": 2,
                        "contract_revision": "v1.1-r2",
                        "sync_epoch": "review-epoch",
                        "entity_schemas": {},
                        "max_push_changes": 100,
                        "max_pull_changes": 100
                    }))
                }
            }),
        )
        .route(
            "/api/v1/sync/pull",
            get(|headers: HeaderMap| async move {
                if headers
                    .get("X-Yilian-Sync-Epoch")
                    .and_then(|value| value.to_str().ok())
                    != Some("review-epoch")
                {
                    return (
                        StatusCode::CONFLICT,
                        Json(json!({"error":{"code":"CURSOR_RESET_REQUIRED"}})),
                    );
                }
                (
                    StatusCode::OK,
                    Json(json!({"changes":[],"next_cursor":"0","has_more":false})),
                )
            }),
        );
    let (base, task) = mock_cloud(remote).await;
    let db = memory_db();
    let scoped = ScopedDb::new(db.clone(), lease());
    let report = CloudSyncService::new(db.clone(), secrets().await, base)
        .unwrap()
        .sync_now_scoped(&scoped, &lease())
        .await;
    task.abort();

    assert!(report.is_ok(), "fresh sync failed: {report:?}");
    assert_eq!(capabilities_hits.load(Ordering::SeqCst), 1);
    let epoch: Option<String> = db
        .conn()
        .query_row(
            "SELECT sync_epoch FROM cloud_sync_cursor WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(epoch.as_deref(), Some("review-epoch"));
}

#[tokio::test]
async fn failed_push_is_retried_with_the_same_idempotency_key() {
    let pushes = Arc::new(AtomicUsize::new(0));
    let keys = Arc::new(Mutex::new(Vec::<String>::new()));
    let observed_pushes = Arc::clone(&pushes);
    let observed_keys = Arc::clone(&keys);
    let remote = Router::new()
        .route(
            "/api/v1/sync/capabilities",
            get(|| async {
                Json(json!({"sync_protocol":2,"contract_revision":"v1.1-r2","sync_epoch":"review-epoch","entity_schemas":{},"max_push_changes":100,"max_pull_changes":100}))
            }),
        )
        .route(
            "/api/v1/sync/push",
            post(move |headers: HeaderMap| {
                let pushes = Arc::clone(&observed_pushes);
                let keys = Arc::clone(&observed_keys);
                async move {
                    pushes.fetch_add(1, Ordering::SeqCst);
                    keys.lock().push(
                        headers
                            .get("Idempotency-Key")
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .to_owned(),
                    );
                    StatusCode::SERVICE_UNAVAILABLE
                }
            }),
        )
        .route(
            "/api/v1/sync/pull",
            get(|| async { Json(json!({"changes":[],"next_cursor":"0","has_more":false})) }),
        );
    let (base, task) = mock_cloud(remote).await;
    let db = memory_db();
    db.enqueue_sync_change(NewSyncChange {
        entity_type: "memory".into(),
        entity_id: "retry-case".into(),
        base_revision: 0,
        revision: 1,
        operation: SyncOperation::Upsert,
        payload: Some(json!({"schema_version":1,"content":"review fixture","category":"note","source":"user","source_conversation_id":null,"created_at":1,"updated_at":1})),
    })
    .unwrap();
    let scoped = ScopedDb::new(db.clone(), lease());
    let service = CloudSyncService::new(db.clone(), secrets().await, base).unwrap();

    assert!(service.sync_now_scoped(&scoped, &lease()).await.is_err());
    assert!(service.sync_now_scoped(&scoped, &lease()).await.is_err());
    task.abort();

    assert_eq!(pushes.load(Ordering::SeqCst), 2);
    let observed_keys = keys.lock().clone();
    assert_eq!(observed_keys.len(), 2);
    assert_eq!(observed_keys[0], observed_keys[1]);
}

#[test]
fn unchanged_download_is_not_recaptured_as_a_local_edit() {
    let db = memory_db();
    let scoped = ScopedDb::new(db.clone(), lease());
    scoped
        .apply_pull_page(
            &lease(),
            "0",
            &PullPage {
                changes: vec![RemoteRecord {
                    change_id: "review-remote-change".into(),
                    entity_type: "conversation".into(),
                    entity_id: "review-conversation".into(),
                    revision: 1,
                    operation: SyncOperation::Upsert,
                    payload: Some(json!({"schema_version":1,"title":"unchanged","created_at":100,"updated_at":100})),
                    sequence: "1".into(),
                    device_id: "review-other-device".into(),
                    changed_at: 100,
                }],
                next_cursor: "1".into(),
                has_more: false,
            },
        )
        .unwrap();
    db.capture_sync_candidates().unwrap();
    let count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM cloud_sync_outbox WHERE entity_type='conversation' AND entity_id='review-conversation'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn advertised_capability_usage_is_projected_on_pull() {
    let db = memory_db();
    let scoped = ScopedDb::new(db.clone(), lease());
    scoped
        .apply_pull_page(
            &lease(),
            "0",
            &PullPage {
                changes: vec![RemoteRecord {
                    change_id: "review-usage-change".into(),
                    entity_type: "capability_usage".into(),
                    entity_id: "review-execution".into(),
                    revision: 1,
                    operation: SyncOperation::Upsert,
                    payload: Some(json!({
                        "schema_version":1,
                        "execution_id":"review-execution",
                        "capability_id":"builtin.tool.read_file",
                        "plugin_id":"builtin",
                        "version":"1",
                        "used_at":100,
                        "execution_status":"succeeded",
                        "latency_ms":1,
                        "approval_required":false,
                        "risk_level":"LOW",
                        "result_digest":null
                    })),
                    sequence: "1".into(),
                    device_id: "review-other-device".into(),
                    changed_at: 100,
                }],
                next_cursor: "1".into(),
                has_more: false,
            },
        )
        .unwrap();
    let count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM capability_usage WHERE execution_id='review-execution'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn pull_uses_negotiated_epoch_on_every_page() {
    let cursors = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&cursors);
    let remote = Router::new()
        .route(
            "/api/v1/sync/capabilities",
            get(|| async {
                Json(json!({"sync_protocol":2,"contract_revision":"v1.1-r2","sync_epoch":"review-epoch","entity_schemas":{},"max_push_changes":100,"max_pull_changes":100}))
            }),
        )
        .route(
            "/api/v1/sync/pull",
            get(move |headers: HeaderMap, Query(params): Query<HashMap<String, String>>| {
                let observed = Arc::clone(&observed);
                async move {
                    assert_eq!(
                        headers
                            .get("X-Yilian-Sync-Epoch")
                            .and_then(|value| value.to_str().ok()),
                        Some("review-epoch")
                    );
                    observed
                        .lock()
                        .push(params.get("cursor").cloned().unwrap_or_default());
                    Json(json!({"changes":[],"next_cursor":"0","has_more":false}))
                }
            }),
        );
    let (base, task) = mock_cloud(remote).await;
    let db = memory_db();
    let scoped = ScopedDb::new(db.clone(), lease());
    CloudSyncService::new(db, secrets().await, base)
        .unwrap()
        .sync_now_scoped(&scoped, &lease())
        .await
        .unwrap();
    task.abort();
    assert_eq!(*cursors.lock(), vec!["0".to_string()]);
}

#[tokio::test]
async fn login_route_activates_a_separate_database_for_each_account() {
    let data_dir =
        std::env::temp_dir().join(format!("yilian-review-login-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();

    let remote = Router::new().route(
        "/api/v1/identity/login",
        post(|Json(body): Json<serde_json::Value>| async move {
            let account = if body["email"] == "a@example.invalid" {
                "account-a"
            } else {
                "account-b"
            };
            Json(json!({
                "account":{"id":account,"email":body["email"],"status":"active","created_at":1},
                "session":{"id":format!("session-{account}"),"device_id":body["device"]["id"],"device_label":"review","current":true,"created_at":1,"last_seen_at":1,"revoked_at":null},
                "access_token":format!("synthetic-access-token-{account}"),
                "refresh_token":format!("synthetic-refresh-token-{account}"),
                "expires_in":3600
            }))
        }),
    );
    let (base, task) = mock_cloud(remote).await;
    let control = "review-control-session-token-with-more-than-32-characters";
    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let server = Arc::new(
        AppServer::new_with_control_session_and_store(
            &data_dir.join("runtime-guest.db"),
            data_dir.to_str().unwrap(),
            ControlSession::new(control).unwrap(),
            secret_store,
        )
        .unwrap(),
    );
    let app = api::build_router(server);

    for email in ["a@example.invalid", "b@example.invalid"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/cloud/login")
                    .header(CONTROL_SESSION_HEADER, control)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "cloud_base_url":base,
                            "email":email,
                            "password":"synthetic-review-password"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(body["status"], "awaiting_restart");

        let registry =
            yilian_backend::modules::cloud::profile::ProfileManager::load_or_initialize(&data_dir)
                .unwrap();
        let active = registry.profiles.get(&registry.active_profile_id).unwrap();
        let db = Database::new(
            &data_dir
                .join("profiles")
                .join(&active.directory_name)
                .join("app.db"),
        )
        .unwrap();
        if email.starts_with('a') {
            db.conn()
                .execute(
                    "INSERT INTO conversations(id,title,created_at,updated_at) VALUES('account-a-private','A only',1,1)",
                    [],
                )
                .unwrap();
        } else {
            let count: i64 = db
                .conn()
                .query_row(
                    "SELECT COUNT(*) FROM conversations WHERE id='account-a-private'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);
        }
    }

    task.abort();
    let _ = std::fs::remove_dir_all(data_dir);
}
