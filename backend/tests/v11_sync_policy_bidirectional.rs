use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;
use yilian_backend::db::cloud::capture::SyncPolicy;
use yilian_backend::db::Database;
use yilian_backend::modules::cloud::contracts::{
    canonical_scope, PullPage, RemoteRecord, ScopeLease, ScopedDb,
};
use yilian_backend::modules::cloud::domain::{NewSyncChange, SyncOperation};

struct TestContext {
    db: Database,
    path: PathBuf,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn setup_test_db() -> TestContext {
    let path =
        std::env::temp_dir().join(format!("v11_sync_policy_test_{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).expect("create test database");
    db.initialize_cloud_schema().expect("init cloud schema");
    TestContext { db, path }
}

fn create_test_scope(db: &Database) -> (ScopeLease, ScopedDb) {
    let (_, scope_id) = canonical_scope("https://cloud.example.com", "user-1").unwrap();
    let lease = ScopeLease {
        scope_id,
        generation: uuid::Uuid::new_v4().to_string(),
    };
    let scoped_db = ScopedDb::new(db.clone(), lease.clone());
    (lease, scoped_db)
}

// P3-01: 禁用会话组，pull 页含会话/消息/记忆 -> 记忆落库；会话/消息的业务、mirror、conflict、revision 均未新增
#[test]
fn test_p3_01_disabled_conversations_group_ignores_conv_and_message_but_applies_memory() {
    let ctx = setup_test_db();
    let (lease, scoped_db) = create_test_scope(&ctx.db);

    let mut policy = SyncPolicy::default();
    policy.sync_conversations = false;
    policy.sync_messages = false;
    policy.sync_memories = true;
    ctx.db.save_cloud_sync_policy(&policy).expect("save policy");

    let page = PullPage {
        changes: vec![
            RemoteRecord {
                change_id: "c-1".into(),
                entity_type: "conversation".into(),
                entity_id: "conv-1".into(),
                revision: 1,
                operation: SyncOperation::Upsert,
                payload: Some(json!({
                    "schema_version": 1,
                    "title": "Ignored Conversation",
                    "created_at": 1000,
                    "updated_at": 1000
                })),
                sequence: "1".into(),
                device_id: "dev-2".into(),
                changed_at: 1000,
            },
            RemoteRecord {
                change_id: "c-2".into(),
                entity_type: "message".into(),
                entity_id: "msg-1".into(),
                revision: 1,
                operation: SyncOperation::Upsert,
                payload: Some(json!({
                    "schema_version": 1,
                    "conversation_id": "conv-1",
                    "role": "user",
                    "content": "Ignored Message",
                    "created_at": 1001
                })),
                sequence: "2".into(),
                device_id: "dev-2".into(),
                changed_at: 1001,
            },
            RemoteRecord {
                change_id: "c-3".into(),
                entity_type: "memory".into(),
                entity_id: "mem-1".into(),
                revision: 1,
                operation: SyncOperation::Upsert,
                payload: Some(json!({
                    "schema_version": 1,
                    "content": "User prefers concise answers",
                    "category": "preference",
                    "source": "manual",
                    "source_conversation_id": null,
                    "created_at": 1002,
                    "updated_at": 1002
                })),
                sequence: "3".into(),
                device_id: "dev-2".into(),
                changed_at: 1002,
            },
        ],
        next_cursor: "3".into(),
        has_more: false,
    };

    let report = scoped_db
        .apply_pull_page(&lease, "0", &page)
        .expect("apply pull page");

    assert_eq!(report.skipped_by_policy, 2);
    assert_eq!(report.applied, 1);
    assert_eq!(report.download_cursor, "3");

    // Minimal assertions from plan
    let conn = ctx.db.conn();
    let conv_msg_mirrors: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cloud_synced_entities WHERE entity_type IN ('conversation','message')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(conv_msg_mirrors, 0, "no mirror records for disabled types");

    let conv_msg_conflicts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cloud_sync_conflicts WHERE entity_type IN ('conversation','message')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        conv_msg_conflicts, 0,
        "no conflict records for disabled types"
    );

    let download_cursor: String = conn
        .query_row(
            "SELECT download_cursor FROM cloud_sync_cursor WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        download_cursor, "3",
        "download_cursor must advance to next_cursor"
    );

    // Business tables
    let conv_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(conv_count, 0);

    let msg_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(msg_count, 0);

    let mem_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mem_count, 1);
}

// P3-02: 页内全部是禁用类别，has_more=true -> next_cursor 正常推进，无死循环
#[test]
fn test_p3_02_all_records_disabled_advances_cursor_without_loop() {
    let ctx = setup_test_db();
    let (lease, scoped_db) = create_test_scope(&ctx.db);

    let mut policy = SyncPolicy::default();
    policy.sync_conversations = false;
    policy.sync_messages = false;
    ctx.db.save_cloud_sync_policy(&policy).expect("save policy");

    let page1 = PullPage {
        changes: vec![RemoteRecord {
            change_id: "c-1".into(),
            entity_type: "conversation".into(),
            entity_id: "conv-1".into(),
            revision: 1,
            operation: SyncOperation::Upsert,
            payload: Some(json!({
                "schema_version": 1,
                "title": "Conv 1",
                "created_at": 1000,
                "updated_at": 1000
            })),
            sequence: "1".into(),
            device_id: "dev-2".into(),
            changed_at: 1000,
        }],
        next_cursor: "1".into(),
        has_more: true,
    };

    let report1 = scoped_db
        .apply_pull_page(&lease, "0", &page1)
        .expect("apply page 1");
    assert_eq!(report1.skipped_by_policy, 1);
    assert_eq!(report1.applied, 0);
    assert_eq!(report1.download_cursor, "1");

    let page2 = PullPage {
        changes: vec![RemoteRecord {
            change_id: "c-2".into(),
            entity_type: "conversation".into(),
            entity_id: "conv-2".into(),
            revision: 1,
            operation: SyncOperation::Upsert,
            payload: Some(json!({
                "schema_version": 1,
                "title": "Conv 2",
                "created_at": 2000,
                "updated_at": 2000
            })),
            sequence: "2".into(),
            device_id: "dev-2".into(),
            changed_at: 2000,
        }],
        next_cursor: "2".into(),
        has_more: false,
    };

    let report2 = scoped_db
        .apply_pull_page(&lease, "1", &page2)
        .expect("apply page 2");
    assert_eq!(report2.skipped_by_policy, 1);
    assert_eq!(report2.applied, 0);
    assert_eq!(report2.download_cursor, "2");

    let conn = ctx.db.conn();
    let download_cursor: String = conn
        .query_row(
            "SELECT download_cursor FROM cloud_sync_cursor WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(download_cursor, "2");
}

// P3-03: 禁用类别远端 delete -> 已有本地记录保留；开启后按既有 tombstone 规则处理
#[test]
fn test_p3_03_disabled_category_remote_delete_retains_local_data_until_reopened() {
    let ctx = setup_test_db();
    let (lease, scoped_db) = create_test_scope(&ctx.db);

    // Initial state: synced conversation exists locally
    {
        let conn = ctx.db.conn();
        conn.execute(
            "INSERT INTO conversations (id, title, created_at, updated_at) VALUES ('conv-1', 'My Chat', 1000, 1000)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO cloud_synced_entities (entity_type, entity_id, revision, deleted, payload_json, canonical_digest, server_sequence, updated_at)
             VALUES ('conversation', 'conv-1', 1, 0, '{\"title\":\"My Chat\"}', 'digest-1', 1, 1000)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO cloud_entity_revisions (entity_type, entity_id, fingerprint, revision, authoritative_revision, local_generation, deleted, updated_at)
             VALUES ('conversation', 'conv-1', 'fp-1', 1, 1, 1, 0, 1000)",
            [],
        ).unwrap();
        conn.execute(
            "UPDATE cloud_sync_cursor SET download_cursor='1', cursor='1', updated_at=1000 WHERE singleton=1",
            [],
        ).unwrap();
    }

    // Disable conversations
    let mut policy = SyncPolicy::default();
    policy.sync_conversations = false;
    policy.sync_messages = false;
    ctx.db.save_cloud_sync_policy(&policy).expect("save policy");

    // Remote deletion arrives while disabled
    let delete_page = PullPage {
        changes: vec![RemoteRecord {
            change_id: "c-del".into(),
            entity_type: "conversation".into(),
            entity_id: "conv-1".into(),
            revision: 2,
            operation: SyncOperation::Delete,
            payload: None,
            sequence: "2".into(),
            device_id: "dev-2".into(),
            changed_at: 2000,
        }],
        next_cursor: "2".into(),
        has_more: false,
    };

    let report = scoped_db
        .apply_pull_page(&lease, "1", &delete_page)
        .expect("apply delete page");
    assert_eq!(report.skipped_by_policy, 1);
    assert_eq!(report.download_cursor, "2");

    // Assert local conversation is still preserved!
    {
        let conn = ctx.db.conn();
        let local_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM conversations WHERE id='conv-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            local_exists, 1,
            "local conversation must NOT be deleted while category is disabled"
        );
    }

    // Re-enable conversations
    policy.sync_conversations = true;
    policy.sync_messages = true;
    ctx.db
        .save_cloud_sync_policy(&policy)
        .expect("reopen policy");

    // Checkpoint must have reset to "0"
    {
        let conn = ctx.db.conn();
        let cursor: String = conn
            .query_row(
                "SELECT download_cursor FROM cloud_sync_cursor WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            cursor, "0",
            "download_cursor must reset to 0 upon reopening"
        );
    }

    // Replay delete from cursor 0
    let replay_page = PullPage {
        changes: vec![RemoteRecord {
            change_id: "c-del".into(),
            entity_type: "conversation".into(),
            entity_id: "conv-1".into(),
            revision: 2,
            operation: SyncOperation::Delete,
            payload: None,
            sequence: "2".into(),
            device_id: "dev-2".into(),
            changed_at: 2000,
        }],
        next_cursor: "2".into(),
        has_more: false,
    };

    let replay_report = scoped_db
        .apply_pull_page(&lease, "0", &replay_page)
        .expect("replay delete page");
    assert_eq!(replay_report.applied, 1);

    // Now local conversation is deleted via tombstone
    {
        let conn = ctx.db.conn();
        let local_after_replay: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM conversations WHERE id='conv-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            local_after_replay, 0,
            "local conversation deleted after reopening and replaying"
        );
    }
}

// P3-04: 关闭 → 其他设备编辑 → 重启/重新开启 -> 有检查点重放，数据完整，不重复，不跳过历史
#[test]
fn test_p3_04_close_reopen_resets_cursor_and_replays_history_completely() {
    let ctx = setup_test_db();
    let (lease, scoped_db) = create_test_scope(&ctx.db);

    let mut policy = SyncPolicy::default();
    policy.sync_memories = false;
    ctx.db
        .save_cloud_sync_policy(&policy)
        .expect("disable memories");

    {
        let conn = ctx.db.conn();
        conn.execute(
            "UPDATE cloud_sync_cursor SET download_cursor='10', cursor='10', updated_at=1000 WHERE singleton=1",
            [],
        ).unwrap();
    }

    // Reopening memories
    policy.sync_memories = true;
    ctx.db
        .save_cloud_sync_policy(&policy)
        .expect("re-enable memories");

    {
        let conn = ctx.db.conn();
        let cursor: String = conn
            .query_row(
                "SELECT download_cursor FROM cloud_sync_cursor WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cursor, "0", "cursor must be 0 for replaying history");
    }

    // Replay history from other device
    let page = PullPage {
        changes: vec![RemoteRecord {
            change_id: "mem-chg-1".into(),
            entity_type: "memory".into(),
            entity_id: "m-1".into(),
            revision: 1,
            operation: SyncOperation::Upsert,
            payload: Some(json!({
                "schema_version": 1,
                "content": "Saved preference",
                "category": "preference",
                "source": "sync",
                "source_conversation_id": null,
                "created_at": 1500,
                "updated_at": 1500
            })),
            sequence: "5".into(),
            device_id: "other-dev".into(),
            changed_at: 1500,
        }],
        next_cursor: "5".into(),
        has_more: false,
    };

    let report = scoped_db
        .apply_pull_page(&lease, "0", &page)
        .expect("replay page");
    assert_eq!(report.applied, 1);
    assert_eq!(report.download_cursor, "5");

    let conn = ctx.db.conn();
    let mem_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM memories WHERE id='m-1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(mem_count, 1);
}

// P3-05: 关闭期间本地编辑，远端也编辑 -> 本地修改登记；进入原有冲突处理，不静默覆盖
#[test]
fn test_p3_05_local_edit_during_disabled_policy_enters_conflict_after_reopen() {
    let ctx = setup_test_db();
    let (lease, scoped_db) = create_test_scope(&ctx.db);

    // Initial synced state at rev 1
    {
        let conn = ctx.db.conn();
        conn.execute(
            "INSERT INTO memories (id, content, category, source, source_conversation_id, created_at, updated_at)
             VALUES ('m-shared', 'Initial Content', 'work', 'manual', NULL, 1000, 1000)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO cloud_synced_entities (entity_type, entity_id, revision, deleted, payload_json, canonical_digest, server_sequence, updated_at)
             VALUES ('memory', 'm-shared', 1, 0, '{\"content\":\"Initial Content\"}', 'digest-init', 1, 1000)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO cloud_entity_revisions (entity_type, entity_id, fingerprint, revision, authoritative_revision, local_generation, deleted, updated_at)
             VALUES ('memory', 'm-shared', 'fp-init', 1, 1, 1, 0, 1000)",
            [],
        ).unwrap();
        conn.execute(
            "UPDATE cloud_sync_cursor SET download_cursor='1', cursor='1', updated_at=1000 WHERE singleton=1",
            [],
        ).unwrap();
    }

    // Disable memories
    let mut policy = SyncPolicy::default();
    policy.sync_memories = false;
    ctx.db
        .save_cloud_sync_policy(&policy)
        .expect("disable memories");

    // Local edit happens while disabled
    {
        let conn = ctx.db.conn();
        conn.execute(
            "UPDATE memories SET content='Local Edit During Disabled', updated_at=2000 WHERE id='m-shared'",
            [],
        ).unwrap();
    }

    // Reopen memories
    policy.sync_memories = true;
    ctx.db
        .save_cloud_sync_policy(&policy)
        .expect("reopen memories");

    // Next capture runs before pull
    let captured = ctx.db.capture_sync_candidates().expect("capture");
    assert!(captured >= 1, "local edit must be captured into outbox");

    // Remote edit arrives with revision 2, different payload
    let remote_page = PullPage {
        changes: vec![RemoteRecord {
            change_id: "rem-c-2".into(),
            entity_type: "memory".into(),
            entity_id: "m-shared".into(),
            revision: 2,
            operation: SyncOperation::Upsert,
            payload: Some(json!({
                "schema_version": 1,
                "content": "Remote Edit During Disabled",
                "category": "work",
                "source": "manual",
                "source_conversation_id": null,
                "created_at": 1000,
                "updated_at": 2000
            })),
            sequence: "2".into(),
            device_id: "other-dev".into(),
            changed_at: 2000,
        }],
        next_cursor: "2".into(),
        has_more: false,
    };

    let report = scoped_db
        .apply_pull_page(&lease, "0", &remote_page)
        .expect("apply remote edit");
    assert_eq!(
        report.conflicts, 1,
        "must store conflict rather than silently overwriting"
    );

    let conn = ctx.db.conn();
    let conflict_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cloud_sync_conflicts WHERE entity_type='memory' AND entity_id='m-shared'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(conflict_count, 1, "conflict record must be preserved");
}

fn setup_test_server(ctx: &TestContext) -> (Arc<yilian_backend::AppServer>, String, axum::Router) {
    let root = ctx.path.parent().unwrap();
    let token = "v".repeat(64);
    let server = Arc::new(
        yilian_backend::AppServer::new_with_control_session_and_store(
            &ctx.path,
            root.to_str().unwrap(),
            yilian_backend::safety::ControlSession::new(token.clone()).unwrap(),
            Arc::new(yilian_backend::integrations::secret::InMemorySecretStore::new()),
        )
        .unwrap(),
    );
    let app = yilian_backend::api::build_router(Arc::clone(&server));
    (server, token, app)
}

// P3-06: 同步请求挂起时 PUT policy -> 409
#[tokio::test]
async fn test_p3_06_operation_lock_returns_409_when_sync_in_progress() {
    let ctx = setup_test_db();
    let (server, token, app) = setup_test_server(&ctx);

    // Acquire lock simulating in-flight sync
    let lock_guard = server.cloud_operation_lock.try_lock();
    assert!(lock_guard.is_ok(), "should acquire lock");

    let policy_payload = json!({
        "sync_conversations": true,
        "sync_messages": true,
        "sync_memories": true,
        "sync_agent_profiles": true,
        "sync_workflow_metadata": true,
        "sync_artifact_metadata": true,
        "sync_task_graphs": true,
        "sync_canvas_views": true,
        "sync_settings": true,
        "sync_capability_usage": true
    });

    let req = Request::builder()
        .method("PUT")
        .uri("/api/cloud/sync/policy")
        .header("content-type", "application/json")
        .header(
            yilian_backend::safety::CONTROL_SESSION_HEADER,
            token.as_str(),
        )
        .body(Body::from(serde_json::to_vec(&policy_payload).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 16)
        .await
        .unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["code"], "cloud_operation_in_progress");
    assert_eq!(body_json["error"], "当前正在同步，请完成后重试");

    // Release lock and try again
    drop(lock_guard);

    let req2 = Request::builder()
        .method("PUT")
        .uri("/api/cloud/sync/policy")
        .header("content-type", "application/json")
        .header(
            yilian_backend::safety::CONTROL_SESSION_HEADER,
            token.as_str(),
        )
        .body(Body::from(serde_json::to_vec(&policy_payload).unwrap()))
        .unwrap();

    let response2 = app.oneshot(req2).await.unwrap();
    assert_eq!(response2.status(), StatusCode::OK);
}

// P3-07: 混合类别上传服务端已接收但响应丢失 → 禁用其中一类 → 重试其余
// 禁用条目不发送，enabled 条目不重复落云；重新开启后剩余条目仍可去重
#[test]
fn test_p3_07_load_or_create_push_batch_skips_disabled_items_and_retains_idempotency() {
    let ctx = setup_test_db();
    let (lease, scoped_db) = create_test_scope(&ctx.db);

    // Enqueue two outbox items
    let chg1 = ctx
        .db
        .enqueue_sync_change(NewSyncChange {
            entity_type: "conversation".into(),
            entity_id: "conv-push-1".into(),
            base_revision: 0,
            revision: 1,
            operation: SyncOperation::Upsert,
            payload: Some(json!({
                "schema_version": 1,
                "title": "Outbox Conv",
                "created_at": 1000,
                "updated_at": 1000
            })),
        })
        .unwrap();

    let chg2 = ctx
        .db
        .enqueue_sync_change(NewSyncChange {
            entity_type: "memory".into(),
            entity_id: "mem-push-1".into(),
            base_revision: 0,
            revision: 1,
            operation: SyncOperation::Upsert,
            payload: Some(json!({
                "schema_version": 1,
                "content": "Outbox Memory",
                "category": "work",
                "source": "manual",
                "source_conversation_id": null,
                "created_at": 1000,
                "updated_at": 1000
            })),
        })
        .unwrap();

    // Disable conversations
    let mut policy = SyncPolicy::default();
    policy.sync_conversations = false;
    policy.sync_messages = false;
    policy.sync_memories = true;
    ctx.db.save_cloud_sync_policy(&policy).expect("save policy");

    // Load push batch: must ONLY contain memory
    let batch = scoped_db.load_or_create_push_batch(&lease, 100).unwrap();
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0].entity_type, "memory");
    assert_eq!(batch[0].change_id, chg2.change_id);

    // The conversation change is untouched in pending state
    {
        let conn = ctx.db.conn();
        let conv_state: String = conn
            .query_row(
                "SELECT state FROM cloud_sync_outbox WHERE change_id=?1",
                [&chg1.change_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            conv_state, "pending",
            "disabled type outbox item must remain pending"
        );

        // Memory change is marked in_flight
        let mem_state: String = conn
            .query_row(
                "SELECT state FROM cloud_sync_outbox WHERE change_id=?1",
                [&chg2.change_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(mem_state, "in_flight");
    }

    // Reopen conversations
    policy.sync_conversations = true;
    policy.sync_messages = true;
    ctx.db
        .save_cloud_sync_policy(&policy)
        .expect("reopen policy");

    // Next load batch includes the conversation with original change_id and body
    let batch2 = scoped_db.load_or_create_push_batch(&lease, 100).unwrap();
    assert_eq!(batch2.len(), 1);
    assert_eq!(batch2[0].entity_type, "conversation");
    assert_eq!(batch2[0].change_id, chg1.change_id);
}

// P3-08: 组合开关字段不一致 -> 400 sync_policy_group_mismatch；政策与 checkpoint 都不发生部分提交
#[tokio::test]
async fn test_p3_08_group_mismatch_returns_400_and_does_not_mutate_state() {
    let ctx = setup_test_db();
    let (_server, token, app) = setup_test_server(&ctx);

    let conn = ctx.db.conn();
    conn.execute(
        "UPDATE cloud_sync_cursor SET download_cursor='77', cursor='77', updated_at=1000 WHERE singleton=1",
        [],
    ).unwrap();

    // Mismatched conversations vs messages
    let mismatched_payload = json!({
        "sync_conversations": true,
        "sync_messages": false, // mismatch!
        "sync_memories": true,
        "sync_agent_profiles": true,
        "sync_workflow_metadata": true,
        "sync_artifact_metadata": true,
        "sync_task_graphs": true,
        "sync_canvas_views": true,
        "sync_settings": true,
        "sync_capability_usage": true
    });

    let req = Request::builder()
        .method("PUT")
        .uri("/api/cloud/sync/policy")
        .header("content-type", "application/json")
        .header(
            yilian_backend::safety::CONTROL_SESSION_HEADER,
            token.as_str(),
        )
        .body(Body::from(serde_json::to_vec(&mismatched_payload).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 16)
        .await
        .unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["code"], "sync_policy_group_mismatch");

    // Checkpoint must NOT be changed
    let cursor: String = conn
        .query_row(
            "SELECT download_cursor FROM cloud_sync_cursor WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        cursor, "77",
        "cursor must not be reset on validation failure"
    );

    // Mismatched task_graphs vs canvas_views
    let mismatched_task_payload = json!({
        "sync_conversations": true,
        "sync_messages": true,
        "sync_memories": true,
        "sync_agent_profiles": true,
        "sync_workflow_metadata": true,
        "sync_artifact_metadata": true,
        "sync_task_graphs": false,
        "sync_canvas_views": true, // mismatch!
        "sync_settings": true,
        "sync_capability_usage": true
    });

    let req2 = Request::builder()
        .method("PUT")
        .uri("/api/cloud/sync/policy")
        .header("content-type", "application/json")
        .header(
            yilian_backend::safety::CONTROL_SESSION_HEADER,
            token.as_str(),
        )
        .body(Body::from(
            serde_json::to_vec(&mismatched_task_payload).unwrap(),
        ))
        .unwrap();

    let response2 = app.oneshot(req2).await.unwrap();
    assert_eq!(response2.status(), StatusCode::BAD_REQUEST);
}
