use yilian_backend::integrations::secret::{cloud_access_token_ref, cloud_refresh_token_ref, anysearch_key_ref};
use yilian_backend::modules::cloud::domain::{
    apply_revision, AccountDeletionState, IdentitySnapshot, IdentityStatus, LifecycleEvent,
    RevisionDecision, SyncOperation,
};
use yilian_backend::modules::cloud::search::{classify_anysearch_status, SearchFailureKind, SearchRequest};

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
