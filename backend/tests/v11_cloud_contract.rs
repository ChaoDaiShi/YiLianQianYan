use std::sync::Arc;

use secrecy::ExposeSecret;
use yilian_backend::integrations::secret::{
    anysearch_key_ref, cloud_access_token_ref, cloud_refresh_token_ref, InMemorySecretStore,
    SecretStore,
};
use yilian_backend::modules::cloud::domain::{
    apply_revision, AccountDeletionState, AuthenticatedSessionInput, IdentitySnapshot,
    IdentityStatus, LifecycleEvent, NewSyncChange, RevisionDecision, SyncOperation,
};
use yilian_backend::modules::cloud::search::{classify_anysearch_status, SearchFailureKind, SearchRequest};
use yilian_backend::modules::cloud::session::CloudSessionService;
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
