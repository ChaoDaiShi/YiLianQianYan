use axum::{
    routing::{get, post},
    Json, Router,
};
use parking_lot::Mutex;
use secrecy::SecretString;
use std::sync::Arc;
use yilian_backend::db::Database;
use yilian_backend::integrations::secret::{
    cloud_access_token_ref, InMemorySecretStore, SecretStore,
};
use yilian_backend::modules::cloud::contracts::{
    canonical_scope, compute_canonical_digest, decide_remote_version, PullPage, PushAck,
    PushResult, PushStatus, RemoteRecord, ScopeLease, ScopedDb, SyncError, VersionDecision,
};
use yilian_backend::modules::cloud::domain::{NewSyncChange, SyncOperation};
use yilian_backend::modules::cloud::sync::CloudSyncService;

fn temp_db() -> (Database, std::path::PathBuf) {
    let path =
        std::env::temp_dir().join(format!("yilian-test-atomicity-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    (db, path)
}

fn create_lease(
    base_url: &str,
    account_id: &str,
    generation: &str,
) -> (ScopeLease, ScopedDb, Database, std::path::PathBuf) {
    let (db, path) = temp_db();
    let (_, scope_id) = canonical_scope(base_url, account_id).unwrap();
    let lease = ScopeLease {
        scope_id,
        generation: generation.to_string(),
    };
    let scoped_db = ScopedDb::new(db.clone(), lease.clone());
    (lease, scoped_db, db, path)
}

#[test]
fn test_decide_remote_version_pure_function() {
    assert_eq!(
        decide_remote_version(Some(8), Some("new"), 7, "old"),
        VersionDecision::Stale
    );
    assert_eq!(
        decide_remote_version(Some(8), Some("new"), 8, "new"),
        VersionDecision::Duplicate
    );
    assert_eq!(
        decide_remote_version(Some(8), Some("new"), 8, "other"),
        VersionDecision::ContentMismatch
    );
    assert_eq!(
        decide_remote_version(None, None, 1, "any"),
        VersionDecision::Apply
    );
    assert_eq!(
        decide_remote_version(Some(8), Some("new"), 9, "next"),
        VersionDecision::Apply
    );
}

#[tokio::test]
async fn test_push_ack_never_advances_download_cursor() {
    let pulled_cursors = Arc::new(Mutex::new(Vec::new()));
    let pulled_cursors_clone = Arc::clone(&pulled_cursors);

    let remote = Router::new()
        .route(
            "/api/v1/sync/push",
            post(|Json(body): Json<serde_json::Value>| async move {
                let change_id = body["changes"][0]["change_id"].as_str().unwrap();
                // Push response returns cursor="12", which must NOT advance download cursor!
                Json(serde_json::json!({
                    "results": [{
                        "change_id": change_id,
                        "status": "applied",
                        "authoritative_revision": 12,
                        "error_code": null
                    }],
                    "cursor": "12"
                }))
            }),
        )
        .route(
            "/api/v1/sync/pull",
            get(move |axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>| {
                let pulled_cursors = Arc::clone(&pulled_cursors_clone);
                async move {
                    let cursor = params.get("cursor").cloned().unwrap_or_default();
                    pulled_cursors.lock().push(cursor.clone());

                    if cursor == "10" {
                        // Return sequence 11 and 12
                        Json(serde_json::json!({
                            "changes": [
                                {
                                    "change_id": "other-device-change",
                                    "entity_type": "memory",
                                    "entity_id": "mem-11",
                                    "revision": 1,
                                    "operation": "upsert",
                                    "payload": {"content": "other device text", "category": "note"},
                                    "sequence": "11",
                                    "device_id": "other-device",
                                    "changed_at": 100
                                },
                                {
                                    "change_id": "local-device-echo",
                                    "entity_type": "memory",
                                    "entity_id": "mem-12",
                                    "revision": 12,
                                    "operation": "upsert",
                                    "payload": {"content": "local device text", "category": "note"},
                                    "sequence": "12",
                                    "device_id": "my-device",
                                    "changed_at": 200
                                }
                            ],
                            "next_cursor": "12",
                            "has_more": false
                        }))
                    } else {
                        Json(serde_json::json!({
                            "changes": [],
                            "next_cursor": cursor,
                            "has_more": false
                        }))
                    }
                }
            }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_task = tokio::spawn(async move { axum::serve(listener, remote).await.unwrap() });

    let (db, path) = temp_db();
    let conn = db.conn();
    // Initialize download_cursor to "10"
    conn.execute(
        "UPDATE cloud_sync_cursor SET download_cursor='10', cursor='10' WHERE singleton=1",
        [],
    )
    .unwrap();
    drop(conn);

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    secret_store
        .put(&cloud_access_token_ref(), SecretString::from("test-token"))
        .await
        .unwrap();

    // Enqueue a local change
    db.enqueue_sync_change(NewSyncChange {
        entity_type: "memory".into(),
        entity_id: "mem-12".into(),
        base_revision: 11,
        revision: 12,
        operation: SyncOperation::Upsert,
        payload: Some(serde_json::json!({"content": "local device text", "category": "note"})),
    })
    .unwrap();

    let service =
        CloudSyncService::new(db.clone(), secret_store, format!("http://{addr}/api/v1")).unwrap();

    let report = service.sync_now().await.unwrap();

    server_task.abort();
    let cursors_requested = pulled_cursors.lock().clone();

    // 1. Verify push_ack did NOT advance download cursor: pull requested from "10", NOT "12"!
    assert_eq!(cursors_requested, vec!["10".to_string()]);

    // 2. Both seq 11 and seq 12 were pulled
    assert_eq!(report.pulled, 2);
    assert_eq!(report.pushed, 1);
    assert_eq!(report.cursor, "12");

    let status = db.cloud_sync_status().unwrap();
    assert_eq!(status.cursor.as_deref(), Some("12"));

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_stale_change_never_writes_projection() {
    let (lease, scoped_db, db, path) = create_lease("http://127.0.0.1:8080", "acc-1", "gen-1");
    let now = chrono::Utc::now().timestamp_millis();

    // Setup local authoritative revision 8
    let conn = db.conn();
    conn.execute(
        "INSERT INTO conversations (id, title, run_status, created_at, updated_at) VALUES ('conv-1', 'Authoritative V8', 'completed', ?1, ?1)",
        [now],
    ).unwrap();
    let digest = compute_canonical_digest(
        "conversation",
        "conv-1",
        8,
        SyncOperation::Upsert,
        Some(&serde_json::json!({"title": "Authoritative V8", "run_status": "completed"})),
    );
    conn.execute(
        "INSERT INTO cloud_synced_entities (entity_type, entity_id, revision, deleted, payload_json, canonical_digest, server_sequence, updated_at)
         VALUES ('conversation', 'conv-1', 8, 0, ?1, ?2, 8, ?3)",
        rusqlite::params![serde_json::json!({"title": "Authoritative V8"}).to_string(), digest, now],
    ).unwrap();
    drop(conn);

    // Incoming remote change with revision 7 (STALE!)
    let page = PullPage {
        changes: vec![RemoteRecord {
            change_id: "c-old".into(),
            entity_type: "conversation".into(),
            entity_id: "conv-1".into(),
            revision: 7,
            operation: SyncOperation::Upsert,
            payload: Some(serde_json::json!({"title": "Old V7 Title", "run_status": "completed"})),
            sequence: "1".into(),
            device_id: "other".into(),
            changed_at: 10,
        }],
        next_cursor: "1".into(),
        has_more: false,
    };

    let report = scoped_db.apply_pull_page(&lease, "0", &page).unwrap();
    assert_eq!(report.stale, 1);
    assert_eq!(report.applied, 0);

    // Assert business projection NOT overwritten!
    let conv = db.get_conversation("conv-1").unwrap();
    assert_eq!(conv.title, "Authoritative V8");

    // Assert cloud_synced_entities revision still 8!
    {
        let conn = db.conn();
        let rev: i64 = conn
            .query_row(
                "SELECT revision FROM cloud_synced_entities WHERE entity_id='conv-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rev, 8);
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_equal_revision_different_payload_is_conflict() {
    let (lease, scoped_db, db, path) = create_lease("http://127.0.0.1:8080", "acc-1", "gen-1");
    let now = chrono::Utc::now().timestamp_millis();

    let local_payload = serde_json::json!({"content": "Local Note", "category": "note"});
    let digest = compute_canonical_digest(
        "memory",
        "mem-1",
        8,
        SyncOperation::Upsert,
        Some(&local_payload),
    );
    let conn = db.conn();
    conn.execute(
        "INSERT INTO memories (id, content, category, source, created_at, updated_at) VALUES ('mem-1', 'Local Note', 'note', 'user', ?1, ?1)",
        [now],
    ).unwrap();
    conn.execute(
        "INSERT INTO cloud_synced_entities (entity_type, entity_id, revision, deleted, payload_json, canonical_digest, server_sequence, updated_at)
         VALUES ('memory', 'mem-1', 8, 0, ?1, ?2, 8, ?3)",
        rusqlite::params![local_payload.to_string(), digest, now],
    ).unwrap();
    drop(conn);

    // Incoming remote change with rev=8 but DIFFERENT payload
    let page = PullPage {
        changes: vec![RemoteRecord {
            change_id: "c-remote".into(),
            entity_type: "memory".into(),
            entity_id: "mem-1".into(),
            revision: 8,
            operation: SyncOperation::Upsert,
            payload: Some(
                serde_json::json!({"content": "Remote Note Different", "category": "note"}),
            ),
            sequence: "1".into(),
            device_id: "other".into(),
            changed_at: 10,
        }],
        next_cursor: "1".into(),
        has_more: false,
    };

    let report = scoped_db.apply_pull_page(&lease, "0", &page).unwrap();
    assert_eq!(report.conflicts, 1);
    assert_eq!(report.applied, 0);

    // Assert original content preserved
    {
        let conn = db.conn();
        let content: String = conn
            .query_row("SELECT content FROM memories WHERE id='mem-1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(content, "Local Note");

        // Assert conflict record created
        let conflict_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cloud_sync_conflicts WHERE entity_id='mem-1' AND status='open'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(conflict_count, 1);
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn test_page_failure_rolls_back_everything() {
    let (lease, scoped_db, db, path) = create_lease("http://127.0.0.1:8080", "acc-1", "gen-1");

    let page = PullPage {
        changes: vec![
            // Item 1: Valid conversation
            RemoteRecord {
                change_id: "c-valid".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-atomic-1".into(),
                revision: 1,
                operation: SyncOperation::Upsert,
                payload: Some(
                    serde_json::json!({"title": "Valid Atomic Conversation", "run_status": "completed"}),
                ),
                sequence: "1".into(),
                device_id: "other".into(),
                changed_at: 10,
            },
            // Item 2: Invalid entity with missing title
            RemoteRecord {
                change_id: "c-invalid".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-atomic-2".into(),
                revision: 1,
                operation: SyncOperation::Upsert,
                payload: Some(serde_json::json!({"bad_field": 123})), // missing title!
                sequence: "2".into(),
                device_id: "other".into(),
                changed_at: 20,
            },
        ],
        next_cursor: "2".into(),
        has_more: false,
    };

    let res = scoped_db.apply_pull_page(&lease, "0", &page);
    assert!(res.is_err(), "Page should fail due to item 2 validation");

    // Verify atomic rollback: item 1 was NOT committed!
    {
        let conn = db.conn();
        let conv_exists: bool = conn
            .query_row(
                "SELECT 1 FROM conversations WHERE id='conv-atomic-1'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        assert!(
            !conv_exists,
            "First item must be rolled back on page failure"
        );

        // Download cursor must NOT advance
        let cursor: String = conn
            .query_row(
                "SELECT download_cursor FROM cloud_sync_cursor WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cursor, "0");
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn test_page_replay_after_crash_is_idempotent() {
    let (lease, scoped_db, db, path) = create_lease("http://127.0.0.1:8080", "acc-1", "gen-1");

    let page = PullPage {
        changes: vec![RemoteRecord {
            change_id: "c-dup".into(),
            entity_type: "conversation".into(),
            entity_id: "conv-dup".into(),
            revision: 1,
            operation: SyncOperation::Upsert,
            payload: Some(
                serde_json::json!({"title": "Duplicate Conv", "run_status": "completed"}),
            ),
            sequence: "1".into(),
            device_id: "other".into(),
            changed_at: 10,
        }],
        next_cursor: "1".into(),
        has_more: false,
    };

    // First application succeeds
    let report1 = scoped_db.apply_pull_page(&lease, "0", &page).unwrap();
    assert_eq!(report1.applied, 1);
    assert_eq!(report1.download_cursor, "1");

    // Replay with reset cursor to simulate crash before saving cursor
    let conn = db.conn();
    conn.execute(
        "UPDATE cloud_sync_cursor SET download_cursor='0', cursor='0' WHERE singleton=1",
        [],
    )
    .unwrap();
    drop(conn);

    let report2 = scoped_db.apply_pull_page(&lease, "0", &page).unwrap();
    assert_eq!(report2.duplicate, 1);
    assert_eq!(report2.applied, 0);
    assert_eq!(report2.conflicts, 0);

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_pending_local_edit_survives_remote_echo() {
    let (lease, scoped_db, db, path) = create_lease("http://127.0.0.1:8080", "acc-1", "gen-1");

    // Local user created rev 3
    let item = db
        .enqueue_sync_change(NewSyncChange {
            entity_type: "conversation".into(),
            entity_id: "conv-echo".into(),
            base_revision: 2,
            revision: 3,
            operation: SyncOperation::Upsert,
            payload: Some(
                serde_json::json!({"title": "Version 3 Local", "run_status": "completed"}),
            ),
        })
        .unwrap();

    // Remote returns echo for this exact change_id
    let page = PullPage {
        changes: vec![RemoteRecord {
            change_id: item.change_id.clone(),
            entity_type: "conversation".into(),
            entity_id: "conv-echo".into(),
            revision: 3,
            operation: SyncOperation::Upsert,
            payload: Some(
                serde_json::json!({"title": "Version 3 Local", "run_status": "completed"}),
            ),
            sequence: "1".into(),
            device_id: "".into(),
            changed_at: 10,
        }],
        next_cursor: "1".into(),
        has_more: false,
    };

    let report = scoped_db.apply_pull_page(&lease, "0", &page).unwrap();
    assert_eq!(report.applied, 1);
    assert_eq!(report.conflicts, 0);

    // Verify authoritative mirror updated to 3
    {
        let conn = db.conn();
        let auth_rev: i64 = conn
            .query_row(
                "SELECT authoritative_revision FROM cloud_entity_revisions WHERE entity_id='conv-echo'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(auth_rev, 3);
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn test_invalid_page_cannot_loop_forever() {
    let (lease, scoped_db, db, path) = create_lease("http://127.0.0.1:8080", "acc-1", "gen-1");

    // Empty page with has_more=true
    let empty_page_loop = PullPage {
        changes: vec![],
        next_cursor: "0".into(),
        has_more: true,
    };
    let err = scoped_db.apply_pull_page(&lease, "0", &empty_page_loop);
    assert!(matches!(err, Err(SyncError::InvalidPage(_))));

    // Page whose next_cursor does not advance
    let non_advancing = PullPage {
        changes: vec![RemoteRecord {
            change_id: "c1".into(),
            entity_type: "memory".into(),
            entity_id: "m1".into(),
            revision: 1,
            operation: SyncOperation::Upsert,
            payload: Some(serde_json::json!({"content": "hi", "category": "note"})),
            sequence: "5".into(),
            device_id: "dev".into(),
            changed_at: 10,
        }],
        next_cursor: "5".into(),
        has_more: false,
    };
    // Update cursor to 5 so assert_checkpoint_tx matches expected cursor 5
    let conn = db.conn();
    conn.execute(
        "UPDATE cloud_sync_cursor SET download_cursor='5', cursor='5' WHERE singleton=1",
        [],
    )
    .unwrap();
    drop(conn);

    // Expected cursor is 5, but next_cursor is also 5
    let err2 = scoped_db.apply_pull_page(&lease, "5", &non_advancing);
    assert!(matches!(err2, Err(SyncError::InvalidPage(_))));

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_scope_switch_rejects_late_ack() {
    let (_lease_a, scoped_db_a, db, path) = create_lease("http://127.0.0.1:8080", "acc-1", "gen-A");
    let lease_b = canonical_scope("http://127.0.0.1:8080", "acc-1")
        .map(|(_, sid)| ScopeLease {
            scope_id: sid,
            generation: "gen-B".into(),
        })
        .unwrap();

    let ack = PushAck {
        results: vec![PushResult {
            change_id: "some-change".into(),
            status: PushStatus::Applied,
            authoritative_revision: Some(1),
            authoritative: None,
            error_code: None,
        }],
    };

    // ScopedDb bound to Lease A rejects ACK with Lease B
    let err = scoped_db_a.ack_push(&lease_b, &ack);
    assert!(matches!(err, Err(SyncError::ScopeChanged)));

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_reconcile_repairs_skipped_history() {
    let (db, path) = temp_db();
    let conn = db.conn();

    // Artificially advance cursor to "20"
    conn.execute(
        "UPDATE cloud_sync_cursor SET download_cursor='20', cursor='20' WHERE singleton=1",
        [],
    )
    .unwrap();
    drop(conn);

    // Enqueue 2 changes
    db.enqueue_sync_change(NewSyncChange {
        entity_type: "memory".into(),
        entity_id: "mem-unpushed-1".into(),
        base_revision: 0,
        revision: 1,
        operation: SyncOperation::Upsert,
        payload: Some(serde_json::json!({"content": "unpushed 1", "category": "note"})),
    })
    .unwrap();
    db.enqueue_sync_change(NewSyncChange {
        entity_type: "memory".into(),
        entity_id: "mem-unpushed-2".into(),
        base_revision: 0,
        revision: 1,
        operation: SyncOperation::Upsert,
        payload: Some(serde_json::json!({"content": "unpushed 2", "category": "note"})),
    })
    .unwrap();

    let report = db.reconcile_skipped_history().unwrap();
    assert_eq!(report.previous_cursor.as_deref(), Some("20"));
    assert_eq!(report.reset_cursor, "0");
    assert_eq!(report.outbox_count, 2);

    // Verify cursor in DB was reset to "0"
    let status = db.cloud_sync_status().unwrap();
    assert_eq!(status.cursor.as_deref(), Some("0"));
    assert_eq!(status.pending_count, 2);

    drop(db);
    let _ = std::fs::remove_file(path);
}
