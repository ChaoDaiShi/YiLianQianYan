use sha2::{Digest, Sha256};
use std::path::Path;
use yilian_backend::modules::cloud::contracts::{
    canonical_scope, validate_sync_payload, PullPage, RemoteRecord, ScopeLease, ScopedDb,
    SyncCapabilities, SyncError, CONTRACT_REVISION, SYNC_EPOCH_HEADER, SYNC_PROTOCOL_HEADER,
    SYNC_PROTOCOL_VERSION,
};
use yilian_backend::modules::cloud::domain::SyncOperation;

fn fixtures_dir() -> std::path::PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap()
        .join("cloud-contract/v1/fixtures/reliability-r2")
}

#[test]
fn test_fixtures_load_and_checksum_manifest_matches() {
    let dir = fixtures_dir();
    let checksum_path = dir.join("CHECKSUMS.sha256");
    assert!(checksum_path.exists(), "CHECKSUMS.sha256 must exist");

    let content = std::fs::read_to_string(&checksum_path).expect("read CHECKSUMS.sha256");
    let mut verified_count = 0;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(parts.len(), 2, "Expected '<hash> <filename>'");
        let expected_hash = parts[0];
        let filename = parts[1];

        let file_path = dir.join(filename);
        assert!(file_path.exists(), "Fixture file {filename} must exist");
        let bytes = std::fs::read(&file_path).expect("read fixture file");
        let actual_hash = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(
            actual_hash, expected_hash,
            "Checksum mismatch for fixture {filename}"
        );
        verified_count += 1;
    }
    assert!(verified_count >= 6, "Must verify at least 6 fixture files");
}

#[test]
fn test_capabilities_fixture_and_deserialization() {
    let file = fixtures_dir().join("capabilities.json");
    let text = std::fs::read_to_string(file).expect("read capabilities.json");
    let caps: SyncCapabilities = serde_json::from_str(&text).expect("parse SyncCapabilities");

    assert_eq!(caps.sync_protocol, SYNC_PROTOCOL_VERSION);
    assert_eq!(caps.contract_revision, CONTRACT_REVISION);
    assert_eq!(caps.sync_epoch, "33333333-3333-4333-8333-333333333333");
    assert_eq!(caps.max_push_changes, 100);
    assert_eq!(caps.max_pull_changes, 100);
    assert_eq!(caps.entity_schemas.get("conversation"), Some(&1));
    assert_eq!(caps.entity_schemas.get("memory"), Some(&1));
    assert_eq!(caps.entity_schemas.get("capability_usage"), Some(&1));
    assert_eq!(SYNC_PROTOCOL_HEADER, "X-Yilian-Sync-Protocol");
    assert_eq!(SYNC_EPOCH_HEADER, "X-Yilian-Sync-Epoch");
}

#[test]
fn test_remote_record_validation_and_delete_null_invariants() {
    let file = fixtures_dir().join("delete_null_payload_conflict.json");
    let text = std::fs::read_to_string(file).expect("read delete_null_payload_conflict.json");
    let json: serde_json::Value = serde_json::from_str(&text).expect("parse json");

    let valid_delete: RemoteRecord =
        serde_json::from_value(json["valid_delete_record"].clone()).expect("parse valid delete");
    assert_eq!(valid_delete.operation, SyncOperation::Delete);
    assert!(valid_delete.payload.is_none());
    assert!(valid_delete.validate().is_ok());

    let invalid_delete: RemoteRecord =
        serde_json::from_value(json["invalid_delete_with_payload"].clone())
            .expect("parse invalid delete");
    assert_eq!(invalid_delete.operation, SyncOperation::Delete);
    assert!(invalid_delete.payload.is_some());
    let err = invalid_delete.validate().unwrap_err();
    assert!(matches!(err, SyncError::PayloadInvalid(_)));

    let invalid_upsert: RemoteRecord =
        serde_json::from_value(json["invalid_upsert_null_payload"].clone())
            .expect("parse invalid upsert");
    assert_eq!(invalid_upsert.operation, SyncOperation::Upsert);
    assert!(invalid_upsert.payload.is_none());
    let err2 = invalid_upsert.validate().unwrap_err();
    assert!(matches!(err2, SyncError::PayloadInvalid(_)));

    // Non-numeric sequence check
    let mut bad_seq = valid_delete.clone();
    bad_seq.sequence = "not-a-number".into();
    assert!(matches!(
        bad_seq.validate().unwrap_err(),
        SyncError::InvalidPage(_)
    ));

    // Negative sequence check
    let mut neg_seq = valid_delete.clone();
    neg_seq.sequence = "-1".into();
    assert!(matches!(
        neg_seq.validate().unwrap_err(),
        SyncError::InvalidPage(_)
    ));

    // Invalid revision check
    let mut bad_rev = valid_delete.clone();
    bad_rev.revision = 0;
    assert!(matches!(
        bad_rev.validate().unwrap_err(),
        SyncError::InvalidRevision(_)
    ));
}

#[test]
fn test_same_revision_different_content_fixture() {
    let file = fixtures_dir().join("same_revision_different_content.json");
    let text = std::fs::read_to_string(file).expect("read same_revision_different_content.json");
    let json: serde_json::Value = serde_json::from_str(&text).expect("parse json");

    let remote: RemoteRecord =
        serde_json::from_value(json["remote_record"].clone()).expect("parse remote record");
    assert_eq!(remote.revision, 2);
    assert_eq!(json["local_change"]["revision"], 2);
    assert_ne!(
        Some(&json["local_change"]["payload"]),
        remote.payload.as_ref()
    );
    assert_eq!(json["expected_error_code"], "REVISION_CONTENT_MISMATCH");
}

#[test]
fn test_dual_device_sync_fixture_and_pull_page() {
    let file = fixtures_dir().join("dual_device_sync.json");
    let text = std::fs::read_to_string(file).expect("read dual_device_sync.json");
    let json: serde_json::Value = serde_json::from_str(&text).expect("parse json");

    let page: PullPage =
        serde_json::from_value(json["pull_page"].clone()).expect("parse pull page");
    assert_eq!(page.changes.len(), 2);
    assert_eq!(page.changes[0].sequence, "1001");
    assert_eq!(page.changes[1].sequence, "1002");
    assert_eq!(page.next_cursor, "1002");
    assert!(!page.has_more);
    assert_ne!(page.changes[0].device_id, page.changes[1].device_id);
}

#[test]
fn test_canonical_scope_and_lease_validation() {
    let file = fixtures_dir().join("account_switch_scope.json");
    let text = std::fs::read_to_string(file).expect("read account_switch_scope.json");
    let json: serde_json::Value = serde_json::from_str(&text).expect("parse json");

    let origin = json["scope_input"]["service_origin"].as_str().unwrap();
    let account = json["scope_input"]["account_id"].as_str().unwrap();
    let (scope_key, scope_id) = canonical_scope(origin, account).expect("canonical scope");

    assert_eq!(scope_key.service_origin, "https://cloud.example.com/api/v1");
    assert_eq!(scope_key.account_id, account);
    assert_eq!(scope_id, json["canonical_scope_id"].as_str().unwrap());

    // Normalization edge cases: trailing slash, uppercase host, default port
    let (normalized_key, normalized_id) =
        canonical_scope("HTTPS://Cloud.Example.COM:443/api/v1/", account)
            .expect("canonical scope normalized");
    assert_eq!(normalized_key, scope_key);
    assert_eq!(normalized_id, scope_id);

    // Rejections
    assert!(canonical_scope("https://user:pass@cloud.example.com/api/v1", account).is_err());
    assert!(canonical_scope("https://cloud.example.com/api/v1?token=123", account).is_err());
    assert!(canonical_scope("https://cloud.example.com/api/v1#anchor", account).is_err());
    assert!(canonical_scope("ftp://cloud.example.com/api/v1", account).is_err());
    assert!(canonical_scope(origin, "").is_err());
    assert!(canonical_scope(origin, "   ").is_err());

    // Lease validation with ScopedDb
    let lease = ScopeLease {
        scope_id: scope_id.clone(),
        generation: "gen-1".into(),
    };
    let path = std::env::temp_dir().join(format!("yilian-scoped-{}.db", uuid::Uuid::new_v4()));
    let db = yilian_backend::Database::new(&path).expect("create db");
    let scoped = ScopedDb::new(db, lease.clone());

    assert!(scoped.validate_lease(&lease).is_ok());

    let mismatched_scope = ScopeLease {
        scope_id: "other-scope".into(),
        generation: "gen-1".into(),
    };
    assert!(matches!(
        scoped.validate_lease(&mismatched_scope).unwrap_err(),
        SyncError::ScopeChanged
    ));

    let mismatched_gen = ScopeLease {
        scope_id,
        generation: "gen-old".into(),
    };
    assert!(matches!(
        scoped.validate_lease(&mismatched_gen).unwrap_err(),
        SyncError::ScopeChanged
    ));

    let _ = std::fs::remove_file(path);
}

#[test]
fn test_validate_sync_payload_and_schema_support() {
    assert!(validate_sync_payload("memory", SyncOperation::Delete, None).is_ok());
    assert!(validate_sync_payload(
        "memory",
        SyncOperation::Upsert,
        Some(&serde_json::json!({"content": "test"}))
    )
    .is_ok());

    // Unknown entity type
    assert!(matches!(
        validate_sync_payload("unknown_type", SyncOperation::Delete, None).unwrap_err(),
        SyncError::UnsupportedSchema(_)
    ));

    // Delete with payload
    assert!(matches!(
        validate_sync_payload(
            "memory",
            SyncOperation::Delete,
            Some(&serde_json::json!({}))
        )
        .unwrap_err(),
        SyncError::PayloadInvalid(_)
    ));

    // Upsert with null
    assert!(matches!(
        validate_sync_payload("memory", SyncOperation::Upsert, None).unwrap_err(),
        SyncError::PayloadInvalid(_)
    ));
}
