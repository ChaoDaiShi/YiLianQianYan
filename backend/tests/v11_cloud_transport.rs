use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use yilian_backend::db::Database;
use yilian_backend::integrations::secret::memory_store::InMemorySecretStore;
use yilian_backend::modules::cloud::client::{CloudApiClient, CloudClientError};
use yilian_backend::modules::cloud::conflict_service::ConflictService;
use yilian_backend::modules::cloud::contracts::{ScopeLease, ScopedDb};
use yilian_backend::modules::cloud::domain::CloudAuthRequest;
use yilian_backend::modules::cloud::lifecycle::{CloudLifecycleError, CloudLifecycleService};
use yilian_backend::modules::cloud::session::CloudSessionService;
use yilian_backend::modules::cloud::sync::{CloudSyncError, CloudSyncService};

#[test]
fn remote_plaintext_is_rejected_at_construction() {
    assert_eq!(
        CloudApiClient::new("http://cloud.example.com/api/v1").unwrap_err(),
        CloudClientError::HttpsRequired
    );
    assert_eq!(
        CloudApiClient::new("http://192.168.1.10:8080/api/v1").unwrap_err(),
        CloudClientError::HttpsRequired
    );
    assert_eq!(
        CloudApiClient::new("http://localhost.evil.example/api/v1").unwrap_err(),
        CloudClientError::HttpsRequired
    );
    assert!(CloudApiClient::new("http://127.0.0.1:38081/api/v1").is_ok());
    assert!(CloudApiClient::new("http://[::1]:38081/api/v1").is_ok());
    assert!(CloudApiClient::new("https://cloud.example.com/api/v1").is_ok());
}

#[tokio::test]
async fn p1_01_remote_plaintext_login_fails_without_network_request() {
    let client_res = CloudApiClient::new("http://cloud.example.com/api/v1");
    assert_eq!(client_res.unwrap_err(), CloudClientError::HttpsRequired);
}

async fn mock_login_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "account": {
            "id": "acc-456",
            "email": "test@example.com"
        },
        "session": {
            "id": "sess-123"
        },
        "access_token": "access-token-abc",
        "refresh_token": "refresh-token-xyz",
        "expires_in": 3600
    }))
}

#[tokio::test]
async fn p1_02_loopback_http_login_and_auth_succeeds() {
    let app = Router::new().route("/identity/login", post(mock_login_handler));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = CloudApiClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let req = CloudAuthRequest {
        email: "test@example.com".into(),
        password: "password".into(),
        display_name: None,
        device_id: "dev-1".into(),
        device_label: "Device".into(),
        client_version: "1.1.0".into(),
    };

    let session = client
        .login(req)
        .await
        .expect("loopback login should succeed");
    assert_eq!(session.session_id, "sess-123");
    assert_eq!(session.account_id, "acc-456");
    assert_eq!(session.access_token, "access-token-abc");
}

#[tokio::test]
async fn p1_03_temporary_redirect_is_not_followed() {
    let target_counter = Arc::new(AtomicUsize::new(0));
    let target_counter_clone = Arc::clone(&target_counter);

    let target_app = Router::new().fallback(move || {
        let c = Arc::clone(&target_counter_clone);
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            StatusCode::OK
        }
    });

    let target_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_port = target_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(target_listener, target_app).await.unwrap();
    });

    let redirect_url = format!("http://127.0.0.1:{target_port}/intercepted");
    let origin_app = Router::new().route(
        "/identity/login",
        post({
            let target = redirect_url.clone();
            move || {
                let target = target.clone();
                async move {
                    (
                        StatusCode::TEMPORARY_REDIRECT,
                        [("Location", target)],
                        "Redirecting",
                    )
                }
            }
        }),
    );

    let origin_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_port = origin_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(origin_listener, origin_app).await.unwrap();
    });

    let client = CloudApiClient::new(format!("http://127.0.0.1:{origin_port}")).unwrap();
    let req = CloudAuthRequest {
        email: "test@example.com".into(),
        password: "secret-password".into(),
        display_name: None,
        device_id: "dev-1".into(),
        device_label: "Device".into(),
        client_version: "1.1.0".into(),
    };

    let result = client.login(req).await;
    // Redirect must NOT be followed; client fails and target server B receives 0 requests
    assert!(result.is_err(), "redirect response should be rejected");
    assert_eq!(
        target_counter.load(Ordering::SeqCst),
        0,
        "Target server B must receive zero requests"
    );
}

#[tokio::test]
async fn p1_04_redirect_302_303_is_not_followed() {
    let target_counter = Arc::new(AtomicUsize::new(0));
    let target_counter_clone = Arc::clone(&target_counter);

    let target_app = Router::new().fallback(move || {
        let c = Arc::clone(&target_counter_clone);
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            StatusCode::OK
        }
    });

    let target_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_port = target_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(target_listener, target_app).await.unwrap();
    });

    let redirect_url = format!("http://127.0.0.1:{target_port}/other");
    let origin_app = Router::new().route(
        "/identity/login",
        post({
            let target = redirect_url.clone();
            move || {
                let target = target.clone();
                async move { (StatusCode::FOUND, [("Location", target)], "Found") }
            }
        }),
    );

    let origin_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_port = origin_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(origin_listener, origin_app).await.unwrap();
    });

    let client = CloudApiClient::new(format!("http://127.0.0.1:{origin_port}")).unwrap();
    let req = CloudAuthRequest {
        email: "test@example.com".into(),
        password: "password".into(),
        display_name: None,
        device_id: "dev-1".into(),
        device_label: "Device".into(),
        client_version: "1.1.0".into(),
    };

    let result = client.login(req).await;
    assert!(result.is_err());
    assert_eq!(
        target_counter.load(Ordering::SeqCst),
        0,
        "Redirect target must not be contacted"
    );
}

#[test]
fn p1_05_embedded_credentials_and_query_rejected() {
    assert_eq!(
        CloudApiClient::new("http://user:pass@127.0.0.1:8080/api/v1").unwrap_err(),
        CloudClientError::InvalidBaseUrl
    );
    assert_eq!(
        CloudApiClient::new("http://127.0.0.1:8080/api/v1?token=123").unwrap_err(),
        CloudClientError::InvalidBaseUrl
    );
    assert_eq!(
        CloudApiClient::new("http://127.0.0.1:8080/api/v1#section").unwrap_err(),
        CloudClientError::InvalidBaseUrl
    );
    assert_eq!(
        CloudApiClient::new("http://:pass@127.0.0.1:8080/api/v1").unwrap_err(),
        CloudClientError::InvalidBaseUrl
    );
}

#[tokio::test]
async fn p1_06_old_db_remote_plaintext_url_rejected_across_all_services() {
    let tmp = std::env::temp_dir().join(format!("yilian-p1-db-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&tmp).unwrap();
    let secret_store: Arc<dyn yilian_backend::integrations::secret::SecretStore> =
        Arc::new(InMemorySecretStore::new());

    let remote_http = "http://remote.example.com/api/v1";

    // 1. CloudSyncService rejects remote http
    let sync_res = CloudSyncService::new(db.clone(), Arc::clone(&secret_store), remote_http);
    assert!(matches!(
        sync_res.err(),
        Some(CloudSyncError::HttpsRequired)
    ));

    // 2. CloudLifecycleService rejects remote http
    let lifecycle_res =
        CloudLifecycleService::new(db.clone(), Arc::clone(&secret_store), remote_http);
    assert!(matches!(
        lifecycle_res.err(),
        Some(CloudLifecycleError::HttpsRequired)
    ));

    // 3. ConflictService rejects remote http
    let lease = ScopeLease {
        scope_id: "scope-1".into(),
        generation: "gen-1".into(),
    };
    let scoped_db = ScopedDb::new(db.clone(), lease);
    let conflict_res = ConflictService::new(scoped_db, Arc::clone(&secret_store), remote_http);
    assert!(conflict_res.is_err());
    let err_msg = conflict_res.err().unwrap().to_string();
    assert!(
        err_msg.contains("HTTPS"),
        "Error should mention HTTPS: {err_msg}"
    );

    // 4. CloudSessionService list_remote_sessions rejects remote http
    let session_service = CloudSessionService::new(db.clone(), Arc::clone(&secret_store));
    let list_res = session_service.list_remote_sessions(remote_http).await;
    assert!(matches!(
        list_res.err(),
        Some(yilian_backend::modules::cloud::session::CloudSessionError::HttpsRequired)
    ));

    // 5. CloudSessionService revoke_remote_session rejects remote http
    let revoke_res = session_service
        .revoke_remote_session(remote_http, "sess-1")
        .await;
    assert!(matches!(
        revoke_res.err(),
        Some(yilian_backend::modules::cloud::session::CloudSessionError::HttpsRequired)
    ));

    let _ = std::fs::remove_file(&tmp);
}
