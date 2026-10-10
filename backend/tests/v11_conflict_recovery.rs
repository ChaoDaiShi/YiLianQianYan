use axum::{routing::get, Json, Router};
use secrecy::SecretString;
use std::sync::Arc;

use yilian_backend::db::Database;
use yilian_backend::integrations::secret::{
    scoped_cloud_access_token_ref, InMemorySecretStore, SecretStore,
};
use yilian_backend::modules::cloud::conflict_service::ConflictService;
use yilian_backend::modules::cloud::contracts::{
    canonical_scope, finalize_resolution_ack_tx, store_conflict_tx, PendingChange, PushResult,
    PushStatus, RemoteRecord, ResolutionChoice, ResolveConflictRequest, ScopeLease, ScopedDb,
    SyncError,
};
use yilian_backend::modules::cloud::domain::SyncOperation;

fn temp_db() -> (Database, std::path::PathBuf) {
    let path =
        std::env::temp_dir().join(format!("yilian-test-conflict-{}.db", uuid::Uuid::new_v4()));
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
fn test_choose_remote_upsert_with_null_payload_fails() {
    let (lease, scoped_db, _db, _path) = create_lease("https://cloud.example.com", "user1", "gen1");
    let now = chrono::Utc::now().timestamp_millis();

    let conflict_id = scoped_db
        .with_write_tx(&lease, |tx| {
            let local = PendingChange {
                change_id: "local-ch-1".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-1".into(),
                base_revision: 1,
                revision: 2,
                operation: SyncOperation::Upsert,
                payload: Some(serde_json::json!({"id": "conv-1", "title": "Local"})),
                device_id: "dev-1".into(),
                changed_at: now,
                local_generation: 1,
            };
            let remote = RemoteRecord {
                change_id: "remote-ch-1".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-1".into(),
                revision: 3,
                operation: SyncOperation::Upsert,
                payload: None, // Null payload!
                sequence: "10".into(),
                device_id: "dev-2".into(),
                changed_at: now,
            };
            store_conflict_tx(tx, &local, &remote)
        })
        .unwrap();

    let request = ResolveConflictRequest {
        conflict_id: conflict_id.clone(),
        expected_conflict_version: 1,
        expected_remote_revision: 3,
        choice: ResolutionChoice::UseRemote,
        request_id: "req-1".into(),
    };

    let result = yilian_backend::modules::cloud::conflict_service::resolve_conflict(
        &scoped_db, &lease, &request, "dev-1",
    );

    match result {
        Err(SyncError::RemotePayloadMissing(_)) => {}
        other => panic!("expected RemotePayloadMissing, got: {:?}", other),
    }

    let conflict = scoped_db
        .with_read_tx(&lease, |tx| {
            yilian_backend::modules::cloud::contracts::get_conflict_tx(tx, &conflict_id)
        })
        .unwrap();
    assert_eq!(conflict.status, "open");
}

#[test]
fn test_local_delete_with_keep_local_enqueues_delete_with_null_payload() {
    let (lease, scoped_db, db, _path) = create_lease("https://cloud.example.com", "user1", "gen1");
    let now = chrono::Utc::now().timestamp_millis();

    let conflict_id = scoped_db
        .with_write_tx(&lease, |tx| {
            let local = PendingChange {
                change_id: "local-ch-2".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-2".into(),
                base_revision: 1,
                revision: 2,
                operation: SyncOperation::Delete,
                payload: None,
                device_id: "dev-1".into(),
                changed_at: now,
                local_generation: 1,
            };
            let remote = RemoteRecord {
                change_id: "remote-ch-2".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-2".into(),
                revision: 3,
                operation: SyncOperation::Upsert,
                payload: Some(serde_json::json!({"id": "conv-2", "title": "Remote"})),
                sequence: "20".into(),
                device_id: "dev-2".into(),
                changed_at: now,
            };
            store_conflict_tx(tx, &local, &remote)
        })
        .unwrap();

    let request = ResolveConflictRequest {
        conflict_id: conflict_id.clone(),
        expected_conflict_version: 1,
        expected_remote_revision: 3,
        choice: ResolutionChoice::KeepLocal,
        request_id: "req-2".into(),
    };

    let res = yilian_backend::modules::cloud::conflict_service::resolve_conflict(
        &scoped_db, &lease, &request, "dev-1",
    )
    .unwrap();

    assert_eq!(res.state, "resolving");
    let queued_id = res.queued_change_id.expect("must have queued change id");

    let conn = db.conn();
    let (op, payload, base_rev, rev): (String, Option<String>, i64, i64) = conn
        .query_row(
            "SELECT operation, payload_json, base_revision, revision FROM cloud_sync_outbox WHERE change_id=?1",
            [&queued_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();

    assert_eq!(op, "delete");
    assert_eq!(payload, None);
    assert_eq!(base_rev, 3);
    assert_eq!(rev, 4);
}

#[test]
fn test_remote_tombstone_local_upsert_restore_requires_new_entity_and_keep_local_as_copy() {
    let (lease, scoped_db, db, _path) = create_lease("https://cloud.example.com", "user1", "gen1");
    let now = chrono::Utc::now().timestamp_millis();

    {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO conversations (id, title, run_status, created_at, updated_at) VALUES ('conv-3', 'Local Note', 'idle', ?1, ?1)",
            [now],
        ).unwrap();
    }

    let conflict_id = scoped_db
        .with_write_tx(&lease, |tx| {
            let local = PendingChange {
                change_id: "local-ch-3".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-3".into(),
                base_revision: 1,
                revision: 2,
                operation: SyncOperation::Upsert,
                payload: Some(serde_json::json!({"id": "conv-3", "title": "Local Note"})),
                device_id: "dev-1".into(),
                changed_at: now,
                local_generation: 1,
            };
            let remote = RemoteRecord {
                change_id: "remote-ch-3".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-3".into(),
                revision: 5,
                operation: SyncOperation::Delete,
                payload: None,
                sequence: "30".into(),
                device_id: "dev-2".into(),
                changed_at: now,
            };
            store_conflict_tx(tx, &local, &remote)
        })
        .unwrap();

    // 1. KeepLocal MUST BE REJECTED
    let req_fail = ResolveConflictRequest {
        conflict_id: conflict_id.clone(),
        expected_conflict_version: 1,
        expected_remote_revision: 5,
        choice: ResolutionChoice::KeepLocal,
        request_id: "req-3a".into(),
    };
    let err = yilian_backend::modules::cloud::conflict_service::resolve_conflict(
        &scoped_db, &lease, &req_fail, "dev-1",
    );
    match err {
        Err(SyncError::RestoreRequiresNewEntity(_)) => {}
        other => panic!("expected RestoreRequiresNewEntity, got: {:?}", other),
    }

    // 2. KeepLocalAsCopy MUST SUCCEED
    let req_copy = ResolveConflictRequest {
        conflict_id: conflict_id.clone(),
        expected_conflict_version: 1,
        expected_remote_revision: 5,
        choice: ResolutionChoice::KeepLocalAsCopy,
        request_id: "req-3b".into(),
    };
    let res = yilian_backend::modules::cloud::conflict_service::resolve_conflict(
        &scoped_db, &lease, &req_copy, "dev-1",
    )
    .unwrap();

    assert_eq!(res.state, "resolved");
    let copied_id = res.copied_entity_id.expect("must have copied_entity_id");
    assert_ne!(copied_id, "conv-3");

    let conn = db.conn();
    let title: String = conn
        .query_row(
            "SELECT title FROM conversations WHERE id=?1",
            [&copied_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(title, "Local Note (副本)");

    // Original entity marked as deleted (tombstone preserved)
    let orig_deleted: i64 = conn
        .query_row(
            "SELECT deleted FROM cloud_synced_entities WHERE entity_type='conversation' AND entity_id='conv-3'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(orig_deleted, 1);
}

#[test]
fn test_atomic_rollback_on_conflict_update_failure() {
    let (lease, scoped_db, db, _path) = create_lease("https://cloud.example.com", "user1", "gen1");
    let now = chrono::Utc::now().timestamp_millis();

    let conflict_id = scoped_db
        .with_write_tx(&lease, |tx| {
            let local = PendingChange {
                change_id: "local-ch-4".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-4".into(),
                base_revision: 1,
                revision: 2,
                operation: SyncOperation::Upsert,
                payload: Some(serde_json::json!({"id": "conv-4", "title": "Local 4"})),
                device_id: "dev-1".into(),
                changed_at: now,
                local_generation: 1,
            };
            let remote = RemoteRecord {
                change_id: "remote-ch-4".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-4".into(),
                revision: 3,
                operation: SyncOperation::Upsert,
                payload: Some(serde_json::json!({"id": "conv-4", "title": "Remote 4"})),
                sequence: "40".into(),
                device_id: "dev-2".into(),
                changed_at: now,
            };
            store_conflict_tx(tx, &local, &remote)
        })
        .unwrap();

    let bad_req = ResolveConflictRequest {
        conflict_id: conflict_id.clone(),
        expected_conflict_version: 999, // Mismatched!
        expected_remote_revision: 3,
        choice: ResolutionChoice::UseRemote,
        request_id: "req-4".into(),
    };

    let result = yilian_backend::modules::cloud::conflict_service::resolve_conflict(
        &scoped_db, &lease, &bad_req, "dev-1",
    );
    assert!(result.is_err());

    let conn = db.conn();
    let outbox_cnt: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cloud_sync_outbox WHERE entity_id='conv-4'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(outbox_cnt, 0);

    let status: String = conn
        .query_row(
            "SELECT status FROM cloud_sync_conflicts WHERE id=?1",
            [&conflict_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "open");
}

#[test]
fn test_idempotent_retry_with_duplicate_request_id() {
    let (lease, scoped_db, db, _path) = create_lease("https://cloud.example.com", "user1", "gen1");
    let now = chrono::Utc::now().timestamp_millis();

    let conflict_id = scoped_db
        .with_write_tx(&lease, |tx| {
            let local = PendingChange {
                change_id: "local-ch-5".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-5".into(),
                base_revision: 1,
                revision: 2,
                operation: SyncOperation::Upsert,
                payload: Some(serde_json::json!({"id": "conv-5", "title": "Local 5"})),
                device_id: "dev-1".into(),
                changed_at: now,
                local_generation: 1,
            };
            let remote = RemoteRecord {
                change_id: "remote-ch-5".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-5".into(),
                revision: 3,
                operation: SyncOperation::Delete,
                payload: None,
                sequence: "50".into(),
                device_id: "dev-2".into(),
                changed_at: now,
            };
            store_conflict_tx(tx, &local, &remote)
        })
        .unwrap();

    let req = ResolveConflictRequest {
        conflict_id: conflict_id.clone(),
        expected_conflict_version: 1,
        expected_remote_revision: 3,
        choice: ResolutionChoice::KeepLocalAsCopy,
        request_id: "req-idem-5".into(),
    };

    let first_res = yilian_backend::modules::cloud::conflict_service::resolve_conflict(
        &scoped_db, &lease, &req, "dev-1",
    )
    .unwrap();

    let second_res = yilian_backend::modules::cloud::conflict_service::resolve_conflict(
        &scoped_db, &lease, &req, "dev-1",
    )
    .unwrap();

    assert_eq!(first_res, second_res);
    assert_eq!(first_res.copied_entity_id, second_res.copied_entity_id);

    let conn = db.conn();
    let outbox_cnt: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cloud_sync_outbox WHERE operation='upsert'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(outbox_cnt, 1);
}

#[test]
fn test_remote_revision_changed_during_resolution_conflict_changed() {
    let (lease, scoped_db, _db, _path) = create_lease("https://cloud.example.com", "user1", "gen1");
    let now = chrono::Utc::now().timestamp_millis();

    let conflict_id = scoped_db
        .with_write_tx(&lease, |tx| {
            let local = PendingChange {
                change_id: "local-ch-6".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-6".into(),
                base_revision: 1,
                revision: 2,
                operation: SyncOperation::Upsert,
                payload: Some(serde_json::json!({"id": "conv-6", "title": "Local 6"})),
                device_id: "dev-1".into(),
                changed_at: now,
                local_generation: 1,
            };
            let remote = RemoteRecord {
                change_id: "remote-ch-6".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-6".into(),
                revision: 5,
                operation: SyncOperation::Upsert,
                payload: Some(serde_json::json!({"id": "conv-6", "title": "Remote 6"})),
                sequence: "60".into(),
                device_id: "dev-2".into(),
                changed_at: now,
            };
            store_conflict_tx(tx, &local, &remote)
        })
        .unwrap();

    // User expected remote 4, but conflict is 5
    let req = ResolveConflictRequest {
        conflict_id: conflict_id.clone(),
        expected_conflict_version: 1,
        expected_remote_revision: 4,
        choice: ResolutionChoice::UseRemote,
        request_id: "req-6".into(),
    };

    let res = yilian_backend::modules::cloud::conflict_service::resolve_conflict(
        &scoped_db, &lease, &req, "dev-1",
    );

    match res {
        Err(SyncError::ConflictChanged(_)) => {}
        other => panic!("expected ConflictChanged, got: {:?}", other),
    }

    // Now resolve with KeepLocal and test server push conflict re-opening
    let valid_req = ResolveConflictRequest {
        conflict_id: conflict_id.clone(),
        expected_conflict_version: 1,
        expected_remote_revision: 5,
        choice: ResolutionChoice::KeepLocal,
        request_id: "req-6-valid".into(),
    };
    let resolving_res = yilian_backend::modules::cloud::conflict_service::resolve_conflict(
        &scoped_db, &lease, &valid_req, "dev-1",
    )
    .unwrap();
    assert_eq!(resolving_res.state, "resolving");
    let queued_id = resolving_res.queued_change_id.unwrap();

    // Server returns PushStatus::Conflict with higher revision 6
    scoped_db
        .with_write_tx(&lease, |tx| {
            let ack_result = PushResult {
                change_id: queued_id.clone(),
                status: PushStatus::Conflict,
                authoritative_revision: Some(6),
                authoritative: Some(RemoteRecord {
                    change_id: "remote-ch-6b".into(),
                    entity_type: "conversation".into(),
                    entity_id: "conv-6".into(),
                    revision: 6,
                    operation: SyncOperation::Upsert,
                    payload: Some(serde_json::json!({"id": "conv-6", "title": "Remote 6b"})),
                    sequence: "61".into(),
                    device_id: "dev-3".into(),
                    changed_at: now + 100,
                }),
                error_code: Some("conflict".into()),
            };
            finalize_resolution_ack_tx(tx, &queued_id, &ack_result)
        })
        .unwrap();

    let updated = scoped_db
        .with_read_tx(&lease, |tx| {
            yilian_backend::modules::cloud::contracts::get_conflict_tx(tx, &conflict_id)
        })
        .unwrap();
    assert_eq!(updated.status, "open");
    assert_eq!(updated.remote_revision, 6);
    assert_eq!(updated.conflict_version, 2);
}

#[test]
fn test_local_edit_during_resolution_protects_local_edit() {
    let (lease, scoped_db, db, _path) = create_lease("https://cloud.example.com", "user1", "gen1");
    let now = chrono::Utc::now().timestamp_millis();

    let conflict_id = scoped_db
        .with_write_tx(&lease, |tx| {
            tx.execute(
                "INSERT INTO cloud_entity_revisions (entity_type, entity_id, fingerprint, revision, authoritative_revision, local_generation, deleted, updated_at)
                 VALUES ('conversation', 'conv-7', '', 1, 0, 1, 0, ?1)",
                [now],
            )?;

            let local = PendingChange {
                change_id: "local-ch-7".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-7".into(),
                base_revision: 1,
                revision: 1,
                operation: SyncOperation::Upsert,
                payload: Some(serde_json::json!({"id": "conv-7", "title": "Local 7 Old"})),
                device_id: "dev-1".into(),
                changed_at: now,
                local_generation: 1,
            };
            let remote = RemoteRecord {
                change_id: "remote-ch-7".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-7".into(),
                revision: 3,
                operation: SyncOperation::Upsert,
                payload: Some(serde_json::json!({"id": "conv-7", "title": "Remote 7"})),
                sequence: "70".into(),
                device_id: "dev-2".into(),
                changed_at: now,
            };
            store_conflict_tx(tx, &local, &remote)
        })
        .unwrap();

    // Concurrently, user edits the entity locally -> local_generation advances to 2!
    {
        let conn = db.conn();
        conn.execute(
            "UPDATE cloud_entity_revisions SET local_generation=2 WHERE entity_type='conversation' AND entity_id='conv-7'",
            [],
        ).unwrap();
    }

    let req = ResolveConflictRequest {
        conflict_id: conflict_id.clone(),
        expected_conflict_version: 1,
        expected_remote_revision: 3,
        choice: ResolutionChoice::UseRemote,
        request_id: "req-7".into(),
    };

    let res = yilian_backend::modules::cloud::conflict_service::resolve_conflict(
        &scoped_db, &lease, &req, "dev-1",
    );

    match res {
        Err(SyncError::ConflictChanged(_)) => {}
        other => panic!(
            "expected ConflictChanged due to concurrent local edit, got: {:?}",
            other
        ),
    }

    let conflict = scoped_db
        .with_read_tx(&lease, |tx| {
            yilian_backend::modules::cloud::contracts::get_conflict_tx(tx, &conflict_id)
        })
        .unwrap();
    assert_eq!(conflict.status, "open");
}

#[test]
fn test_duplicate_remote_conflicts_bounded_count() {
    let (lease, scoped_db, db, _path) = create_lease("https://cloud.example.com", "user1", "gen1");
    let now = chrono::Utc::now().timestamp_millis();

    let local = PendingChange {
        change_id: "local-ch-8".into(),
        entity_type: "conversation".into(),
        entity_id: "conv-8".into(),
        base_revision: 1,
        revision: 2,
        operation: SyncOperation::Upsert,
        payload: Some(serde_json::json!({"id": "conv-8", "title": "Local 8"})),
        device_id: "dev-1".into(),
        changed_at: now,
        local_generation: 1,
    };
    let remote = RemoteRecord {
        change_id: "remote-ch-8".into(),
        entity_type: "conversation".into(),
        entity_id: "conv-8".into(),
        revision: 3,
        operation: SyncOperation::Upsert,
        payload: Some(serde_json::json!({"id": "conv-8", "title": "Remote 8"})),
        sequence: "80".into(),
        device_id: "dev-2".into(),
        changed_at: now,
    };

    let id1 = scoped_db
        .with_write_tx(&lease, |tx| store_conflict_tx(tx, &local, &remote))
        .unwrap();

    let id2 = scoped_db
        .with_write_tx(&lease, |tx| store_conflict_tx(tx, &local, &remote))
        .unwrap();

    let id3 = scoped_db
        .with_write_tx(&lease, |tx| store_conflict_tx(tx, &local, &remote))
        .unwrap();

    assert_eq!(id1, id2);
    assert_eq!(id2, id3);

    let conn = db.conn();
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM cloud_sync_conflicts WHERE entity_type='conversation' AND entity_id='conv-8' AND status='open'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_ensure_remote_snapshot_retrieval_and_404_fail_closed() {
    let (lease, scoped_db, _db, _path) = create_lease("http://127.0.0.1", "user-snap", "gen-snap");
    let now = chrono::Utc::now().timestamp_millis();

    // Router simulating server:
    // GET /sync/entities/conversation/conv-found -> returns RemoteRecord
    // GET /sync/entities/conversation/conv-404 -> returns 404
    let app = Router::new()
        .route(
            "/api/v1/sync/entities/conversation/conv-found",
            get(|| async move {
                Json(serde_json::json!({
                    "change_id": "rem-ch-found",
                    "entity_type": "conversation",
                    "entity_id": "conv-found",
                    "revision": 8,
                    "operation": "upsert",
                    "payload": {"schema_version":1,"title":"Server Snapshot","created_at":1,"updated_at":1},
                    "sequence": "99",
                    "device_id": "dev-svr",
                    "changed_at": 123456789
                }))
            }),
        )
        .route(
            "/api/v1/sync/entities/conversation/conv-404",
            get(|| async move { axum::http::StatusCode::NOT_FOUND }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let base_url = format!("http://127.0.0.1:{}", addr.port());
    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    secret_store
        .put(
            &scoped_cloud_access_token_ref(&lease.scope_id),
            SecretString::from("token-xyz"),
        )
        .await
        .unwrap();
    scoped_db.bind_sync_epoch(&lease, "test-epoch").unwrap();

    let service = ConflictService::new(scoped_db.clone(), secret_store, &base_url).unwrap();

    // 1. Conflict for conv-found with missing payload
    let found_id = scoped_db
        .with_write_tx(&lease, |tx| {
            let local = PendingChange {
                change_id: "loc-ch-f".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-found".into(),
                base_revision: 1,
                revision: 2,
                operation: SyncOperation::Upsert,
                payload: Some(serde_json::json!({"id": "conv-found", "title": "Loc"})),
                device_id: "dev-1".into(),
                changed_at: now,
                local_generation: 1,
            };
            let remote = RemoteRecord {
                change_id: "rem-ch-f".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-found".into(),
                revision: 8,
                operation: SyncOperation::Upsert,
                payload: None, // Missing!
                sequence: "99".into(),
                device_id: "dev-svr".into(),
                changed_at: now,
            };
            store_conflict_tx(tx, &local, &remote)
        })
        .unwrap();

    let snapshot = service
        .ensure_remote_snapshot(&lease, &found_id)
        .await
        .unwrap();
    assert_eq!(snapshot.revision, 8);
    assert!(snapshot.payload.is_some());

    // 2. Conflict for conv-404 with missing payload
    let not_found_id = scoped_db
        .with_write_tx(&lease, |tx| {
            let local = PendingChange {
                change_id: "loc-ch-404".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-404".into(),
                base_revision: 1,
                revision: 2,
                operation: SyncOperation::Upsert,
                payload: Some(serde_json::json!({"id": "conv-404", "title": "Loc"})),
                device_id: "dev-1".into(),
                changed_at: now,
                local_generation: 1,
            };
            let remote = RemoteRecord {
                change_id: "rem-ch-404".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-404".into(),
                revision: 8,
                operation: SyncOperation::Upsert,
                payload: None,
                sequence: "100".into(),
                device_id: "dev-svr".into(),
                changed_at: now,
            };
            store_conflict_tx(tx, &local, &remote)
        })
        .unwrap();

    let not_found_res = service.ensure_remote_snapshot(&lease, &not_found_id).await;
    match not_found_res {
        Err(SyncError::RemotePayloadMissing(_)) => {}
        other => panic!("expected RemotePayloadMissing on 404, got: {:?}", other),
    }

    // Verify status remains open!
    let conflict = scoped_db
        .with_read_tx(&lease, |tx| {
            yilian_backend::modules::cloud::contracts::get_conflict_tx(tx, &not_found_id)
        })
        .unwrap();
    assert_eq!(conflict.status, "open");
}
