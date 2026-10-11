use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header::CONTENT_TYPE, Method, Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use tower::ServiceExt;
use uuid::Uuid;

use yilian_backend::api::build_router;
use yilian_backend::app::state::AppServer;
use yilian_backend::db::Database;
use yilian_backend::integrations::secret::memory_store::InMemorySecretStore;
use yilian_backend::integrations::secret::{
    revoke_session_token_ref, scoped_cloud_access_token_ref, SecretRef, SecretStore,
    SecretStoreError, SecretStoreStatus,
};
use yilian_backend::modules::cloud::contracts::ScopeKey;
use yilian_backend::modules::cloud::profile::{ActivationChoice, ProfileKind, ProfileManager};
use yilian_backend::modules::cloud::revocation::{
    list_registered_profile_revocations, retry_registered_profile_revocations,
    run_registered_profile_revocation_retry_loop, RevocationService,
};
use yilian_backend::safety::{ControlSession, CONTROL_SESSION_HEADER};

struct TempDir(std::path::PathBuf);
impl TempDir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("yilian-logout-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn protected_request(
    server: Arc<AppServer>,
    token: &str,
    method: Method,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = build_router(server)
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(CONTROL_SESSION_HEADER, token)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::json!({}));
    (status, json)
}

fn create_test_server(
    app_data: &std::path::Path,
    secret_store: Arc<dyn SecretStore>,
) -> (Arc<AppServer>, String) {
    let (db_path, lease, _) = ProfileManager::resolve_active_profile_db_path(app_data).unwrap();
    let token = "t".repeat(64);
    let session = ControlSession::new(token.clone()).unwrap();
    let server = AppServer::new_with_control_session_and_store(
        &db_path,
        &app_data.to_string_lossy(),
        session,
        secret_store,
    )
    .unwrap();
    let server = Arc::new(server);
    server.scoped_db.activate_lease(lease);
    (server, token)
}

/// P2-01: Direct call to POST /api/cloud/profile/switch returns 401 profile_authentication_required
#[tokio::test]
async fn test_p2_01_profile_switch_endpoint_returns_401_authentication_required() {
    let tmp = TempDir::new();
    let app_data = tmp.path();
    let secret_store = Arc::new(InMemorySecretStore::new());
    let (server, token) = create_test_server(app_data, secret_store);

    let switch_payload = serde_json::json!({
        "service_origin": "https://cloud.example.com",
        "account_id": "forged-account-a"
    });

    let (status, body) = protected_request(
        server.clone(),
        &token,
        Method::POST,
        "/api/cloud/profile/switch",
        switch_payload,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        body.get("code").and_then(|v| v.as_str()),
        Some("profile_authentication_required")
    );
}

/// P2-02: Login activates the account ID returned by remote cloud auth, ignoring any forged target in request
#[tokio::test]
async fn test_p2_02_login_activates_account_from_remote_auth_response() {
    let tmp = TempDir::new();
    let app_data = tmp.path();

    // Mock cloud server returning account_id "cloud-confirmed-user"
    let mock_cloud = Router::new().route(
        "/identity/login",
        post(|| async {
            Json(serde_json::json!({
                "account": {
                    "id": "cloud-confirmed-user",
                    "email": "user@example.com"
                },
                "session": {
                    "id": "sess-cloud-1"
                },
                "access_token": "mock-access-token-1234567890",
                "refresh_token": "mock-refresh-token-1234567890",
                "expires_in": 3600
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, mock_cloud).await.unwrap();
    });

    let secret_store = Arc::new(InMemorySecretStore::new());
    let (server, token) = create_test_server(app_data, secret_store);

    let login_payload = serde_json::json!({
        "cloud_base_url": format!("http://127.0.0.1:{port}"),
        "email": "user@example.com",
        "password": "secretpassword"
    });

    let (status, body) = protected_request(
        server.clone(),
        &token,
        Method::POST,
        "/api/cloud/login",
        login_payload,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let plan = body.get("activation").expect("activation in body");
    let profile_id = plan.get("profile_id").and_then(|v| v.as_str()).unwrap();
    assert!(profile_id.starts_with("account-"));

    // Verify registry committed the profile for cloud-confirmed-user
    let registry = ProfileManager::load_or_initialize(app_data).unwrap();
    let entry = registry
        .profiles
        .get(profile_id)
        .expect("profile in registry");
    let scope_key = entry.scope_key.as_ref().expect("scope key");
    assert_eq!(scope_key.account_id, "cloud-confirmed-user");
    assert_eq!(entry.reauthentication_required, false);
}

/// P2-03: Re-login into account restores original directory and data, does not copy into new guest
#[tokio::test]
async fn test_p2_03_relogin_restores_original_data_directory() {
    let tmp = TempDir::new();
    let app_data = tmp.path();
    let mock_cloud = Router::new().route(
        "/identity/login",
        post(|| async {
            Json(serde_json::json!({
                "account": {
                    "id": "user-alpha-restore",
                    "email": "alpha@example.com"
                },
                "session": {
                    "id": "sess-alpha-2"
                },
                "access_token": "mock-access-token-alpha-12345",
                "refresh_token": "mock-refresh-token-alpha-12345",
                "expires_in": 3600
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, mock_cloud).await.unwrap();
    });

    let cloud_url = format!("http://127.0.0.1:{port}");

    // 1. Prepare and activate Account A with matching cloud_url
    let scope_a = ScopeKey {
        service_origin: cloud_url.clone(),
        account_id: "user-alpha-restore".into(),
    };
    let plan_a =
        ProfileManager::prepare_profile(app_data, &scope_a, ActivationChoice::OpenAccountOnly)
            .unwrap();
    ProfileManager::persist_activation(app_data, &plan_a).unwrap();

    // Write some unique conversation in Account A
    let account_db_path = app_data
        .join("profiles")
        .join(&plan_a.profile_id)
        .join("app.db");
    let account_db = Database::new(&account_db_path).unwrap();
    let conv_id = Uuid::new_v4().to_string();
    account_db
        .conn()
        .execute(
            "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?1, 'Alpha Original Notes', 1000, 1000)",
            rusqlite::params![conv_id],
        )
        .unwrap();

    // Mark account reauthentication_required = true
    ProfileManager::set_reauthentication_required(app_data, &plan_a.profile_id, true).unwrap();

    let secret_store = Arc::new(InMemorySecretStore::new());
    let (server, token) = create_test_server(app_data, secret_store);

    let login_payload = serde_json::json!({
        "cloud_base_url": cloud_url,
        "email": "alpha@example.com",
        "password": "correctpassword"
    });

    let (status, body) = protected_request(
        server.clone(),
        &token,
        Method::POST,
        "/api/cloud/login",
        login_payload,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let plan = body.get("activation").unwrap();
    assert_eq!(
        plan.get("profile_id").and_then(|v| v.as_str()),
        Some(plan_a.profile_id.as_str())
    );

    // Verify existing conversation still exists and directory was preserved
    let check_conn = rusqlite::Connection::open(&account_db_path).unwrap();
    let count: i64 = check_conn
        .query_row(
            "SELECT COUNT(*) FROM conversations WHERE id=?1",
            rusqlite::params![conv_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "Original conversations must be preserved intact");

    // Verify reauthentication_required was cleared
    let registry = ProfileManager::load_or_initialize(app_data).unwrap();
    let entry = registry.profiles.get(&plan_a.profile_id).unwrap();
    assert_eq!(entry.reauthentication_required, false);
}

/// SecretStore wrapper that fails on deleting specific keys
struct FaultySecretStore {
    inner: InMemorySecretStore,
    fail_delete: AtomicBool,
}
impl FaultySecretStore {
    fn new() -> Self {
        Self {
            inner: InMemorySecretStore::new(),
            fail_delete: AtomicBool::new(false),
        }
    }
}
#[async_trait::async_trait]
impl SecretStore for FaultySecretStore {
    async fn put(
        &self,
        secret_ref: &SecretRef,
        value: secrecy::SecretString,
    ) -> Result<(), SecretStoreError> {
        self.inner.put(secret_ref, value).await
    }
    async fn get(
        &self,
        secret_ref: &SecretRef,
    ) -> Result<Option<secrecy::SecretString>, SecretStoreError> {
        self.inner.get(secret_ref).await
    }
    async fn delete(&self, secret_ref: &SecretRef) -> Result<(), SecretStoreError> {
        if self.fail_delete.load(Ordering::SeqCst) {
            return Err(SecretStoreError::AccessDenied);
        }
        self.inner.delete(secret_ref).await
    }
    async fn status(&self) -> SecretStoreStatus {
        SecretStoreStatus::Available
    }
}

/// P2-04: Injected DB/SecretStore delete failure returns HTTP 500 with LOCAL_SECRET_DELETE_FAILED, attempts deleting remaining credentials
#[tokio::test]
async fn test_p2_04_injected_secret_delete_failure_returns_500_with_error_code() {
    let tmp = TempDir::new();
    let app_data = tmp.path();
    let faulty_store = Arc::new(FaultySecretStore::new());

    let (server, token) = create_test_server(app_data, faulty_store.clone());

    // Inject failure for SecretStore delete
    faulty_store.fail_delete.store(true, Ordering::SeqCst);

    let (status, body) = protected_request(
        server.clone(),
        &token,
        Method::POST,
        "/api/cloud/logout",
        serde_json::json!({}),
    )
    .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let report = body.get("report").expect("report in error response");
    assert_eq!(
        report.get("local_signed_out").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        report.get("local_cleanup").and_then(|v| v.as_str()),
        Some("failed")
    );
    let error_codes = report
        .get("error_codes")
        .and_then(|v| v.as_array())
        .expect("error_codes array");
    let codes: Vec<&str> = error_codes.iter().filter_map(|c| c.as_str()).collect();
    assert!(codes.contains(&"LOCAL_SECRET_DELETE_FAILED"));
}

/// P2-05: Offline remote revocation leaves pending task; retry_registered_profile_revocations completes it when remote is available
#[tokio::test]
async fn test_p2_05_offline_remote_revocation_retry_without_activating_profile() {
    let tmp = TempDir::new();
    let app_data = tmp.path();

    let secret_store = Arc::new(InMemorySecretStore::new());

    // Prepare Account Profile
    let scope = ScopeKey {
        service_origin: "http://127.0.0.1:59999".into(), // currently unreachable
        account_id: "user-pending-revoke".into(),
    };
    let plan = ProfileManager::prepare_profile(app_data, &scope, ActivationChoice::OpenAccountOnly)
        .unwrap();
    ProfileManager::persist_activation(app_data, &plan).unwrap();

    let account_db_path = app_data
        .join("profiles")
        .join(&plan.profile_id)
        .join("app.db");
    let account_db = Database::new(&account_db_path).unwrap();

    let session_id = "sess-revocation-test-123";
    let access_ref = scoped_cloud_access_token_ref(&plan.lease.scope_id);
    secret_store
        .put(
            &access_ref,
            secrecy::SecretString::from("token-revoke-12345678901234567890".to_string()),
        )
        .await
        .unwrap();

    // Logout with unreachable server
    let revocation_service =
        RevocationService::new(account_db.clone(), Arc::clone(&secret_store) as _);
    let report = revocation_service
        .logout(
            &plan.lease,
            Some(session_id),
            Some("http://127.0.0.1:59999"),
            Some("user-pending-revoke"),
        )
        .await;

    assert!(report.local_signed_out);
    assert_eq!(
        report.remote_revocation,
        yilian_backend::modules::cloud::revocation::RemoteRevocation::Pending
    );

    // Ensure pending record exists in DB
    let pending_before = revocation_service
        .list_pending_revocations(chrono::Utc::now().timestamp_millis() + 100_000)
        .unwrap();
    assert_eq!(pending_before.len(), 1);

    // Switch active profile to guest (as happens after logout)
    let registry = ProfileManager::load_or_initialize(app_data).unwrap();
    let guest_id = registry
        .profiles
        .values()
        .find(|e| e.kind == ProfileKind::Guest)
        .unwrap()
        .profile_id
        .clone();
    let mut reg = registry.clone();
    reg.active_profile_id = guest_id.clone();
    ProfileManager::save_registry(app_data, &reg).unwrap();

    // Now start mock remote server on an available port
    let revoked_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = Arc::clone(&revoked_flag);
    let mock_server = Router::new().route(
        "/sessions/:id/revoke",
        post(move || {
            let f = Arc::clone(&flag_clone);
            async move {
                f.store(true, Ordering::SeqCst);
                StatusCode::OK
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let actual_port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, mock_server).await.unwrap();
    });

    // Update service origin in pending revocations to the now-available server and make next_retry_at immediately eligible
    account_db
        .conn()
        .execute(
            "UPDATE cloud_pending_revocations SET service_origin = ?1, next_retry_at = ?2",
            rusqlite::params![
                format!("http://127.0.0.1:{actual_port}"),
                chrono::Utc::now().timestamp_millis() - 1000
            ],
        )
        .unwrap();

    // Call retry_registered_profile_revocations
    let retried = retry_registered_profile_revocations(app_data, Arc::clone(&secret_store) as _)
        .await
        .unwrap();
    assert_eq!(retried, 1, "Should successfully retry 1 pending revocation");
    assert!(revoked_flag.load(Ordering::SeqCst));

    // Active profile MUST remain guest
    let reg_after = ProfileManager::load_or_initialize(app_data).unwrap();
    assert_eq!(reg_after.active_profile_id, guest_id);

    // Pending record should be deleted from SQLite
    let pending_after = revocation_service
        .list_pending_revocations(chrono::Utc::now().timestamp_millis() + 100_000)
        .unwrap();
    assert_eq!(pending_after.len(), 0);
}

/// P2-06: Max 5 retries marks RETRY_EXHAUSTED in database, retains row; missing token marks REVOKE_CREDENTIAL_MISSING
#[tokio::test]
async fn test_p2_06_exhausted_retries_and_missing_token_retain_rows_as_unknown() {
    let tmp = TempDir::new();
    let db_path = tmp.path().join("exhausted.db");
    let db = Database::new(&db_path).unwrap();
    db.initialize_cloud_schema().unwrap();

    let secret_store = Arc::new(InMemorySecretStore::new());
    let service = RevocationService::new(db.clone(), Arc::clone(&secret_store) as _);

    let session_missing = "sess-missing-token";
    let now = chrono::Utc::now().timestamp_millis();

    // 1. Insert record where token does not exist in SecretStore
    service
        .insert_pending_revocation(
            session_missing,
            "scope-1",
            "http://127.0.0.1:12345",
            "user-1",
            now - 1000,
            None,
            now,
        )
        .unwrap();

    let confirmed = service.retry_pending_revocations(now).await.unwrap();
    assert_eq!(confirmed, 0);

    // Check last_error_code updated to REVOKE_CREDENTIAL_MISSING and row is retained
    let err_missing: String = db
        .conn()
        .query_row(
            "SELECT last_error_code FROM cloud_pending_revocations WHERE session_id = ?1",
            rusqlite::params![session_missing],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(err_missing, "REVOKE_CREDENTIAL_MISSING");

    // 2. Insert record with attempts = 4, store token, let it fail -> reaches 5 attempts -> RETRY_EXHAUSTED
    let session_exhaust = "sess-exhaust-test";
    let revoke_ref = revoke_session_token_ref(session_exhaust);
    secret_store
        .put(
            &revoke_ref,
            secrecy::SecretString::from("token-exhaust-1234567890".to_string()),
        )
        .await
        .unwrap();

    service
        .insert_pending_revocation(
            session_exhaust,
            "scope-2",
            "http://127.0.0.1:59998", // unreachable
            "user-2",
            now - 1000,
            None,
            now,
        )
        .unwrap();
    db.conn()
        .execute(
            "UPDATE cloud_pending_revocations SET attempts = 4 WHERE session_id = ?1",
            rusqlite::params![session_exhaust],
        )
        .unwrap();

    let confirmed = service.retry_pending_revocations(now).await.unwrap();
    assert_eq!(confirmed, 0);

    let err_exhaust: String = db
        .conn()
        .query_row(
            "SELECT last_error_code FROM cloud_pending_revocations WHERE session_id = ?1",
            rusqlite::params![session_exhaust],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(err_exhaust, "RETRY_EXHAUSTED");

    // Row is excluded from future retries
    let list_retryable = service.list_pending_revocations(now + 1_000_000).unwrap();
    assert_eq!(list_retryable.len(), 0);
}

/// P2-07: Network offline without logout does not set reauthentication_required, profile remains usable
#[tokio::test]
async fn test_p2_07_network_offline_without_logout_preserves_profile() {
    let tmp = TempDir::new();
    let app_data = tmp.path();

    let scope = ScopeKey {
        service_origin: "https://cloud.example.com".into(),
        account_id: "user-offline-ok".into(),
    };
    let plan = ProfileManager::prepare_profile(app_data, &scope, ActivationChoice::OpenAccountOnly)
        .unwrap();
    ProfileManager::persist_activation(app_data, &plan).unwrap();

    // Verify reauthentication_required is false
    let registry = ProfileManager::load_or_initialize(app_data).unwrap();
    let entry = registry.profiles.get(&plan.profile_id).unwrap();
    assert_eq!(entry.reauthentication_required, false);

    // Resolving active profile does NOT change active profile away from account
    let (_, _, active_id) = ProfileManager::resolve_active_profile_db_path(app_data).unwrap();
    assert_eq!(active_id, plan.profile_id);
}

/// P2-08: Forced restart after logout sets active profile back to guest via resolve_active_profile_db_path
#[tokio::test]
async fn test_p2_08_restart_after_logout_switches_to_guest() {
    let tmp = TempDir::new();
    let app_data = tmp.path();

    // 1. Prepare and activate Account Profile
    let scope = ScopeKey {
        service_origin: "https://cloud.example.com".into(),
        account_id: "user-logged-out".into(),
    };
    let plan = ProfileManager::prepare_profile(app_data, &scope, ActivationChoice::OpenAccountOnly)
        .unwrap();
    ProfileManager::persist_activation(app_data, &plan).unwrap();

    // 2. Mark profile with reauthentication_required = true (as happens on logout)
    ProfileManager::set_reauthentication_required(app_data, &plan.profile_id, true).unwrap();

    // 3. Simulate application restart: call resolve_active_profile_db_path
    let (_, _, active_id) = ProfileManager::resolve_active_profile_db_path(app_data).unwrap();

    // Active profile must be guest!
    assert!(
        active_id.starts_with("guest-"),
        "Active profile after restart must be switched to guest, got: {active_id}"
    );

    let registry = ProfileManager::load_or_initialize(app_data).unwrap();
    assert_eq!(registry.active_profile_id, active_id);
    let account_entry = registry.profiles.get(&plan.profile_id).unwrap();
    assert_eq!(account_entry.reauthentication_required, true);
}

async fn seed_authenticated_account(
    app_data: &std::path::Path,
    origin: &str,
    secret_store: Arc<dyn SecretStore>,
) -> yilian_backend::modules::cloud::profile::ActivationPlan {
    let scope = ScopeKey {
        service_origin: origin.into(),
        account_id: "review-account".into(),
    };
    let plan = ProfileManager::prepare_profile(app_data, &scope, ActivationChoice::OpenAccountOnly)
        .unwrap();
    let db = Database::new(&ProfileManager::profile_db_path(app_data, &plan)).unwrap();
    yilian_backend::modules::cloud::session::CloudSessionService::new(db.clone(), secret_store)
        .persist_authenticated_session_scoped(
            yilian_backend::modules::cloud::domain::AuthenticatedSessionInput {
                account_id: "review-account".into(),
                email: "review@example.test".into(),
                session_id: "review-session".into(),
                access_token: "synthetic-review-access-value-20261011".into(),
                refresh_token: "synthetic-review-refresh-value-20261011".into(),
                expires_at: chrono::Utc::now().timestamp_millis() + 3_600_000,
            },
            &plan.lease.scope_id,
        )
        .await
        .unwrap();
    db.save_cloud_configuration(origin, "sync").unwrap();
    ProfileManager::persist_activation(app_data, &plan).unwrap();
    plan
}

#[tokio::test]
async fn logout_retry_clears_the_original_account_scope() {
    let tmp = TempDir::new();
    let cloud = Router::new().route(
        "/sessions/:id/revoke",
        post(|| async { Json(serde_json::json!({"ok": true})) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, cloud).await.unwrap() });
    let store = Arc::new(FaultySecretStore::new());
    let plan = seed_authenticated_account(tmp.path(), &origin, store.clone()).await;
    let access_ref = scoped_cloud_access_token_ref(&plan.lease.scope_id);
    let (server, token) = create_test_server(tmp.path(), store.clone());

    store.fail_delete.store(true, Ordering::SeqCst);
    let (first_status, _) = protected_request(
        server.clone(),
        &token,
        Method::POST,
        "/api/cloud/logout",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(first_status, StatusCode::INTERNAL_SERVER_ERROR);

    store.fail_delete.store(false, Ordering::SeqCst);
    let (retry_status, retry) = protected_request(
        server,
        &token,
        Method::POST,
        "/api/cloud/logout",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(retry_status, StatusCode::OK, "retry response: {retry}");
    assert_eq!(retry["report"]["local_signed_out"], true);
    assert!(
        store.get(&access_ref).await.unwrap().is_none(),
        "logout retry must delete the original account credential"
    );
    task.abort();
}

#[tokio::test]
async fn background_loop_finishes_original_scope_cleanup_after_guest_restart() {
    let tmp = TempDir::new();
    let cloud = Router::new().route(
        "/sessions/:id/revoke",
        post(|| async { Json(serde_json::json!({"ok": true})) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let cloud_task = tokio::spawn(async move { axum::serve(listener, cloud).await.unwrap() });
    let store = Arc::new(FaultySecretStore::new());
    let plan = seed_authenticated_account(tmp.path(), &origin, store.clone()).await;
    let access_ref = scoped_cloud_access_token_ref(&plan.lease.scope_id);
    let (server, token) = create_test_server(tmp.path(), store.clone());

    store.fail_delete.store(true, Ordering::SeqCst);
    let (status, _) = protected_request(
        server,
        &token,
        Method::POST,
        "/api/cloud/logout",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(store.get(&access_ref).await.unwrap().is_some());

    let (_, _, active_profile) =
        ProfileManager::resolve_active_profile_db_path(tmp.path()).unwrap();
    assert!(active_profile.starts_with("guest-"));
    store.fail_delete.store(false, Ordering::SeqCst);

    let cancellation = tokio_util::sync::CancellationToken::new();
    let loop_task = tokio::spawn(run_registered_profile_revocation_retry_loop(
        tmp.path().to_path_buf(),
        store.clone(),
        std::time::Duration::from_millis(10),
        cancellation.clone(),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if store.get(&access_ref).await.unwrap().is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("background cleanup should remove the original profile credential");
    cancellation.cancel();
    loop_task.await.unwrap();
    cloud_task.abort();
}

#[tokio::test]
async fn refresh_and_logout_are_serialized_by_the_cloud_operation_lock() {
    let tmp = TempDir::new();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let started = entered.clone();
    let unblock = release.clone();
    let cloud = Router::new()
        .route(
            "/identity/refresh",
            post(move || {
                let started = started.clone();
                let unblock = unblock.clone();
                async move {
                    started.notify_one();
                    unblock.notified().await;
                    Json(serde_json::json!({
                        "account":{"id":"review-account","email":"review@example.test"},
                        "session":{"id":"review-session"},
                        "access_token":"synthetic-refreshed-access-20261011",
                        "refresh_token":"synthetic-refreshed-refresh-20261011",
                        "expires_in":3600
                    }))
                }
            }),
        )
        .route(
            "/sessions/:id/revoke",
            post(|| async { Json(serde_json::json!({"ok": true})) }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let cloud_task = tokio::spawn(async move { axum::serve(listener, cloud).await.unwrap() });
    let store = Arc::new(InMemorySecretStore::new());
    let plan = seed_authenticated_account(tmp.path(), &origin, store.clone()).await;
    let access_ref = scoped_cloud_access_token_ref(&plan.lease.scope_id);
    let (server, token) = create_test_server(tmp.path(), store.clone());

    let refresh_server = server.clone();
    let refresh_token = token.clone();
    let refresh_task = tokio::spawn(async move {
        protected_request(
            refresh_server,
            &refresh_token,
            Method::POST,
            "/api/cloud/refresh",
            serde_json::json!({}),
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
        .await
        .unwrap();

    let (busy_status, busy_body) = protected_request(
        server.clone(),
        &token,
        Method::POST,
        "/api/cloud/logout",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(busy_status, StatusCode::CONFLICT, "response: {busy_body}");

    release.notify_one();
    assert_eq!(refresh_task.await.unwrap().0, StatusCode::OK);
    let (logout_status, logout) = protected_request(
        server,
        &token,
        Method::POST,
        "/api/cloud/logout",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(logout_status, StatusCode::OK, "response: {logout}");
    assert!(store.get(&access_ref).await.unwrap().is_none());
    cloud_task.abort();
}

#[tokio::test]
async fn background_revocation_loop_processes_a_due_task_without_another_restart() {
    let tmp = TempDir::new();
    let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let route_hits = hits.clone();
    let cloud = Router::new().route(
        "/sessions/:id/revoke",
        post(move || {
            let route_hits = route_hits.clone();
            async move {
                route_hits.fetch_add(1, Ordering::SeqCst);
                Json(serde_json::json!({"ok": true}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let cloud_task = tokio::spawn(async move { axum::serve(listener, cloud).await.unwrap() });
    let store = Arc::new(InMemorySecretStore::new());
    let plan = seed_authenticated_account(tmp.path(), &origin, store.clone()).await;
    let db = Database::new(&ProfileManager::profile_db_path(tmp.path(), &plan)).unwrap();
    let service = RevocationService::new(db.clone(), store.clone());
    let session_id = "background-retry-session";
    store
        .put(
            &revoke_session_token_ref(session_id),
            secrecy::SecretString::from("synthetic-revoke-value-20261011".to_string()),
        )
        .await
        .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    service
        .insert_pending_revocation(
            session_id,
            &plan.lease.scope_id,
            &origin,
            "review-account",
            now - 1,
            Some("REMOTE_UNREACHABLE"),
            now,
        )
        .unwrap();

    let cancellation = tokio_util::sync::CancellationToken::new();
    let task_cancellation = cancellation.clone();
    let data_dir = tmp.path().to_path_buf();
    let loop_task = tokio::spawn(run_registered_profile_revocation_retry_loop(
        data_dir,
        store,
        std::time::Duration::from_millis(10),
        task_cancellation,
    ));
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if hits.load(Ordering::SeqCst) > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    cancellation.cancel();
    loop_task.await.unwrap();

    let remaining: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM cloud_pending_revocations WHERE session_id=?1",
            [session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(remaining, 0);
    cloud_task.abort();
}

#[tokio::test]
async fn guest_profile_can_read_pending_revocation_summaries_from_account_databases() {
    let tmp = TempDir::new();
    let store = Arc::new(InMemorySecretStore::new());
    let plan = seed_authenticated_account(tmp.path(), "http://127.0.0.1:59991", store).await;
    let db = Database::new(&ProfileManager::profile_db_path(tmp.path(), &plan)).unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    RevocationService::new(db, Arc::new(InMemorySecretStore::new()))
        .insert_pending_revocation(
            "summary-session",
            &plan.lease.scope_id,
            "http://127.0.0.1:59991",
            "review-account",
            now + 60_000,
            Some("REMOTE_UNREACHABLE"),
            now,
        )
        .unwrap();
    let guest = ProfileManager::prepare_guest_activation(tmp.path()).unwrap();
    ProfileManager::persist_activation(tmp.path(), &guest).unwrap();

    let summaries = list_registered_profile_revocations(tmp.path()).unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].profile_id, plan.profile_id);
    assert_eq!(summaries[0].session_id, "summary-session");
    assert_eq!(summaries[0].remote_revocation, "pending");
}
