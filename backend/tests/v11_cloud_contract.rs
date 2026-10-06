use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    routing::post,
    Json, Router,
};
use parking_lot::Mutex;
use secrecy::ExposeSecret;
use tower::ServiceExt;
use yilian_backend::{api, safety::{ControlSession, CONTROL_SESSION_HEADER}, AppServer};
use yilian_backend::integrations::secret::{
    anysearch_key_ref, cloud_access_token_ref, cloud_refresh_token_ref, InMemorySecretStore,
    SecretStore,
};
use yilian_backend::modules::cloud::domain::{
    apply_revision, AccountDeletionState, AuthenticatedSessionInput, CloudAuthRequest,
    IdentitySnapshot, IdentityStatus, LifecycleEvent, NewSyncChange, RevisionDecision,
    SyncOperation,
};
use yilian_backend::modules::cloud::client::CloudApiClient;
use yilian_backend::modules::cloud::search::{
    classify_anysearch_status, AnySearchProvider, SearchFailureKind, SearchProvider, SearchRequest,
};
use yilian_backend::modules::cloud::session::CloudSessionService;
use yilian_backend::modules::cloud::sync::CloudSyncService;
use yilian_backend::Database;

#[test]
fn guest_identity_keeps_local_core_available_without_cloud() {
    let identity = IdentitySnapshot::guest("local-1", "device-1");
    assert_eq!(identity.status, IdentityStatus::Guest);
    assert!(identity.local_core_available);
    assert!(!identity.cloud_enabled);
    assert!(identity.account_id.is_none());
}

#[test]
fn stale_upsert_conflicts_and_tombstone_blocks_resurrection() {
    assert_eq!(
        apply_revision(3, 4, SyncOperation::Upsert, false),
        RevisionDecision::Conflict { authoritative_revision: 4 },
    );
    assert_eq!(
        apply_revision(4, 4, SyncOperation::Upsert, true),
        RevisionDecision::TombstoneWins { authoritative_revision: 4 },
    );
    assert_eq!(
        apply_revision(4, 4, SyncOperation::Delete, false),
        RevisionDecision::Apply { next_revision: 5 },
    );
}

#[test]
fn deletion_lifecycle_revokes_sessions_before_deleting() {
    let mut state = AccountDeletionState::Active;
    state = state.transition(LifecycleEvent::Request).unwrap();
    assert_eq!(state, AccountDeletionState::Requested);
    state = state.transition(LifecycleEvent::SessionsRevoked).unwrap();
    assert_eq!(state, AccountDeletionState::Deleting);
    state = state.transition(LifecycleEvent::Complete).unwrap();
    assert_eq!(state, AccountDeletionState::Deleted);
    assert!(state.transition(LifecycleEvent::Retry).is_err());
}

#[test]
fn cloud_and_search_secret_refs_are_stable_and_value_free() {
    assert_eq!(cloud_access_token_ref().key, "cloud.identity.access_token");
    assert_eq!(cloud_refresh_token_ref().key, "cloud.identity.refresh_token");
    assert_eq!(anysearch_key_ref().key, "search.anysearch.api_key");
    for secret_ref in [cloud_access_token_ref(), cloud_refresh_token_ref(), anysearch_key_ref()] {
        secret_ref.validate().unwrap();
        assert!(!secret_ref.key.contains("Bearer"));
    }
}

#[test]
fn anysearch_request_and_failures_follow_public_contract() {
    let request = SearchRequest::new("Rust async cancellation", 50).unwrap();
    assert_eq!(request.max_results, 10);
    assert!(SearchRequest::new("   ", 5).is_err());
    assert_eq!(classify_anysearch_status(401), SearchFailureKind::Authentication);
    assert_eq!(classify_anysearch_status(402), SearchFailureKind::Quota);
    assert_eq!(classify_anysearch_status(429), SearchFailureKind::RateLimited);
    assert_eq!(classify_anysearch_status(502), SearchFailureKind::Unavailable);
}

#[test]
fn local_identity_is_stable_and_sync_failures_are_visible() {
    let path = std::env::temp_dir().join(format!("yilian-v11-cloud-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    let first = db.get_or_create_cloud_identity().unwrap();
    let second = db.get_or_create_cloud_identity().unwrap();
    assert_eq!(first.local_identity_id, second.local_identity_id);
    assert_eq!(first.device_id, second.device_id);
    assert_eq!(first.status, IdentityStatus::Guest);

    let queued = db.enqueue_sync_change(NewSyncChange {
        entity_type: "memory".into(),
        entity_id: "memory-1".into(),
        base_revision: 0,
        revision: 1,
        operation: SyncOperation::Upsert,
        payload: Some(serde_json::json!({"content":"safe metadata"})),
    }).unwrap();
    db.mark_sync_change_failed(&queued.change_id, "SERVICE_UNAVAILABLE", 1_791_260_000_000).unwrap();
    let status = db.cloud_sync_status().unwrap();
    assert_eq!(status.pending_count, 1);
    assert_eq!(status.error_count, 1);
    assert_eq!(status.last_error_code.as_deref(), Some("SERVICE_UNAVAILABLE"));

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn authenticated_tokens_only_enter_secret_store_and_logout_clears_them() {
    let path = std::env::temp_dir().join(format!("yilian-v11-session-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let service = CloudSessionService::new(db.clone_connection(), Arc::clone(&store));
    service.persist_authenticated_session(AuthenticatedSessionInput {
        account_id: "account-1".into(),
        email: "user@example.com".into(),
        session_id: "session-1".into(),
        access_token: "access-secret-value".into(),
        refresh_token: "refresh-secret-value".into(),
        expires_at: 1_791_260_000_000,
    }).await.unwrap();

    let identity = db.cloud_identity().unwrap().unwrap();
    assert_eq!(identity.status, IdentityStatus::Authenticated);
    assert_eq!(identity.account_id.as_deref(), Some("account-1"));
    assert_eq!(
        store.get(&cloud_access_token_ref()).await.unwrap().unwrap().expose_secret(),
        "access-secret-value",
    );
    let persisted = std::fs::read(&path).unwrap();
    assert!(!persisted.windows("access-secret-value".len()).any(|value| value == b"access-secret-value"));

    service.logout().await.unwrap();
    assert!(store.get(&cloud_access_token_ref()).await.unwrap().is_none());
    assert!(store.get(&cloud_refresh_token_ref()).await.unwrap().is_none());
    assert_eq!(db.cloud_identity().unwrap().unwrap().status, IdentityStatus::Guest);

    drop(service);
    drop(db);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn anysearch_provider_uses_secret_store_and_normalizes_results() {
    let observed_authorization = Arc::new(Mutex::new(None::<String>));
    let observed = Arc::clone(&observed_authorization);
    let app = Router::new().route(
        "/v1/search",
        post(move |headers: HeaderMap, State(observed): State<Arc<Mutex<Option<String>>>>, Json(body): Json<serde_json::Value>| async move {
            *observed.lock() = headers.get("authorization").and_then(|value| value.to_str().ok()).map(str::to_string);
            assert_eq!(body["query"], "Rust cancellation");
            Json(serde_json::json!({
                "code": 0,
                "message": "success",
                "request_id": "request-1",
                "data": {
                    "results": [{"title":"Tokio cancellation", "url":"https://example.test/tokio", "snippet":"Safe cancellation"}],
                    "metadata": {"total_results":1,"search_time_ms":12}
                }
            }))
        }),
    ).with_state(observed);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    store.put(&anysearch_key_ref(), secrecy::SecretString::from("search-secret")).await.unwrap();
    let provider = AnySearchProvider::new(format!("http://{address}"), store);
    let response = provider.search(SearchRequest::new("Rust cancellation", 5).unwrap()).await.unwrap();
    assert_eq!(response.request_id, "request-1");
    assert_eq!(response.results[0].title, "Tokio cancellation");
    assert_eq!(observed_authorization.lock().as_deref(), Some("Bearer search-secret"));

    server.abort();
}

#[tokio::test]
async fn cloud_client_maps_login_contract_without_logging_or_persisting_tokens() {
    let app = Router::new().route(
        "/api/v1/identity/login",
        post(|Json(body): Json<serde_json::Value>| async move {
            assert_eq!(body["email"], "user@example.com");
            assert_eq!(body["device"]["id"], "device-1");
            Json(serde_json::json!({
                "account": {"id":"account-1","email":"user@example.com","status":"active","created_at":1},
                "session": {"id":"session-1","device_id":"device-1","device_label":"Windows","current":true,"created_at":1,"last_seen_at":1,"revoked_at":null},
                "access_token":"access-value",
                "refresh_token":"refresh-value",
                "expires_in":3600
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = CloudApiClient::new(format!("http://{address}/api/v1")).unwrap();
    let session = client.login(CloudAuthRequest {
        email: "user@example.com".into(),
        password: "password-value".into(),
        display_name: None,
        device_id: "device-1".into(),
        device_label: "Windows".into(),
        client_version: "1.1.0-rc.1".into(),
    }).await.unwrap();
    assert_eq!(session.account_id, "account-1");
    assert_eq!(session.session_id, "session-1");
    assert_eq!(session.access_token, "access-value");
    assert!(session.expires_at > chrono::Utc::now().timestamp_millis());
    server.abort();
}

#[tokio::test]
async fn local_cloud_identity_endpoint_is_guest_first_and_control_session_protected() {
    let root = std::env::temp_dir().join(format!("yilian-v11-api-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let token = "v".repeat(64);
    let server = Arc::new(AppServer::new_with_control_session_and_store(
        &root.join("cloud-api.db"),
        root.to_str().unwrap(),
        ControlSession::new(token.clone()).unwrap(),
        Arc::new(InMemorySecretStore::new()),
    ).unwrap());
    let app = api::build_router(server);
    let unauthorized = app.clone().oneshot(
        Request::builder().uri("/api/cloud/identity").body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let response = app.clone().oneshot(
        Request::builder()
            .uri("/api/cloud/identity")
            .header(CONTROL_SESSION_HEADER, token)
            .body(Body::empty())
            .unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &to_bytes(response.into_body(), 64 * 1024).await.unwrap(),
    ).unwrap();
    assert_eq!(body["identity"]["status"], "guest");
    assert_eq!(body["identity"]["local_core_available"], true);
    assert_eq!(body["sync"]["pending_count"], 0);
    let sync_status = app.oneshot(
        Request::builder()
            .uri("/api/cloud/sync/status")
            .header(CONTROL_SESSION_HEADER, "v".repeat(64))
            .body(Body::empty())
            .unwrap(),
    ).await.unwrap();
    assert_eq!(sync_status.status(), StatusCode::OK);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn local_login_proxies_cloud_and_returns_only_redacted_identity() {
    let remote = Router::new().route(
        "/api/v1/identity/login",
        post(|| async {
            Json(serde_json::json!({
                "account": {"id":"account-2","email":"person@example.com","status":"active","created_at":1},
                "session": {"id":"session-2","device_id":"ignored","device_label":"Windows","current":true,"created_at":1,"last_seen_at":1,"revoked_at":null},
                "access_token":"never-return-access",
                "refresh_token":"never-return-refresh",
                "expires_in":3600
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let remote_address = listener.local_addr().unwrap();
    let remote_server = tokio::spawn(async move { axum::serve(listener, remote).await.unwrap() });

    let root = std::env::temp_dir().join(format!("yilian-v11-login-api-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let token = "l".repeat(64);
    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let server = Arc::new(AppServer::new_with_control_session_and_store(
        &root.join("cloud-login.db"), root.to_str().unwrap(),
        ControlSession::new(token.clone()).unwrap(), Arc::clone(&secret_store),
    ).unwrap());
    let response = api::build_router(server).oneshot(
        Request::builder()
            .method("POST")
            .uri("/api/cloud/login")
            .header(CONTROL_SESSION_HEADER, token)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({
                "cloud_base_url": format!("http://{remote_address}/api/v1"),
                "email":"person@example.com",
                "password":"password-value"
            }).to_string()))
            .unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!text.contains("never-return-access"));
    assert!(!text.contains("never-return-refresh"));
    let body: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["identity"]["status"], "authenticated");
    assert_eq!(body["identity"]["email"], "person@example.com");
    assert!(secret_store.get(&cloud_access_token_ref()).await.unwrap().is_some());
    remote_server.abort();
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn search_api_keeps_key_write_only_and_returns_normalized_results() {
    let remote = Router::new().route(
        "/v1/search",
        post(|headers: HeaderMap| async move {
            assert_eq!(headers.get("authorization").unwrap(), "Bearer search-api-secret");
            Json(serde_json::json!({
                "code":0,"message":"success","request_id":"search-request-2",
                "data":{"results":[{"title":"Result","url":"https://example.test/result","snippet":"Summary"}],"metadata":{"total_results":1,"search_time_ms":8}}
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let remote_address = listener.local_addr().unwrap();
    let remote_server = tokio::spawn(async move { axum::serve(listener, remote).await.unwrap() });
    let root = std::env::temp_dir().join(format!("yilian-v11-search-api-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let token = "s".repeat(64);
    let server = Arc::new(AppServer::new_with_control_session_and_store(
        &root.join("search.db"), root.to_str().unwrap(),
        ControlSession::new(token.clone()).unwrap(), Arc::new(InMemorySecretStore::new()),
    ).unwrap());
    let app = api::build_router(server);
    let configured = app.clone().oneshot(
        Request::builder().method("PUT").uri("/api/search/config")
            .header(CONTROL_SESSION_HEADER, &token).header("content-type","application/json")
            .body(Body::from(serde_json::json!({
                "base_url":format!("http://{remote_address}"),"api_key":"search-api-secret"
            }).to_string())).unwrap(),
    ).await.unwrap();
    assert_eq!(configured.status(), StatusCode::OK);
    let configured_text = String::from_utf8(to_bytes(configured.into_body(), 64 * 1024).await.unwrap().to_vec()).unwrap();
    assert!(!configured_text.contains("search-api-secret"));
    assert_eq!(serde_json::from_str::<serde_json::Value>(&configured_text).unwrap()["configured"], true);

    let searched = app.oneshot(
        Request::builder().method("POST").uri("/api/search")
            .header(CONTROL_SESSION_HEADER, token).header("content-type","application/json")
            .body(Body::from(r#"{"query":"current Rust docs","max_results":5}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(searched.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(&to_bytes(searched.into_body(), 64 * 1024).await.unwrap()).unwrap();
    assert_eq!(body["request_id"], "search-request-2");
    assert_eq!(body["results"][0]["title"], "Result");
    remote_server.abort();
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn sync_service_pushes_outbox_pulls_cursor_and_clears_only_applied_changes() {
    let remote = Router::new()
        .route("/api/v1/sync/push", post(|Json(body): Json<serde_json::Value>| async move {
            let change_id = body["changes"][0]["change_id"].as_str().unwrap();
            Json(serde_json::json!({
                "results":[{"change_id":change_id,"status":"applied","authoritative_revision":1,"error_code":null}],
                "cursor":"1"
            }))
        }))
        .route("/api/v1/sync/pull", axum::routing::get(|| async {
            Json(serde_json::json!({"changes":[],"next_cursor":"1","has_more":false}))
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let remote_address = listener.local_addr().unwrap();
    let remote_server = tokio::spawn(async move { axum::serve(listener, remote).await.unwrap() });
    let path = std::env::temp_dir().join(format!("yilian-v11-sync-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    store.put(&cloud_access_token_ref(), secrecy::SecretString::from("cloud-access")).await.unwrap();
    db.enqueue_sync_change(NewSyncChange {
        entity_type:"memory".into(), entity_id:"memory-sync-1".into(), base_revision:0,
        revision:1, operation:SyncOperation::Upsert, payload:Some(serde_json::json!({"category":"note"})),
    }).unwrap();
    let service = CloudSyncService::new(
        db.clone_connection(), store, format!("http://{remote_address}/api/v1"),
    ).unwrap();
    let report = service.sync_now().await.unwrap();
    assert_eq!(report.pushed, 1);
    assert_eq!(report.pulled, 0);
    let status = db.cloud_sync_status().unwrap();
    assert_eq!(status.pending_count, 0);
    assert_eq!(status.cursor.as_deref(), Some("1"));
    assert!(status.last_success_at.is_some());
    remote_server.abort();
    drop(db);
    let _ = std::fs::remove_file(path);
}
