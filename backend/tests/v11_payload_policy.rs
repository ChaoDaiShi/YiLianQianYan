use rusqlite::params;
use serde_json::json;
use std::path::PathBuf;
use yilian_backend::db::cloud::capture::SyncPolicy;
use yilian_backend::db::cloud::usage::{
    list_capability_usage, project_usage_tx, record_capability_usage_tx, UsageRecord,
};
use yilian_backend::modules::cloud::contracts::validate_sync_payload_strictly;
use yilian_backend::modules::cloud::domain::SyncOperation;
use yilian_backend::modules::cloud::payload::{validate_message, SyncCapabilityUsagePayload};
use yilian_backend::safety::grant::model::{
    GrantEffect, GrantResource, GrantSource, ProcessGrantScope, SecurityGrant,
};
use yilian_backend::safety::PermissionId;
use yilian_backend::Database;

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
        std::env::temp_dir().join(format!("payload_policy_test_{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).expect("create test database");
    db.initialize_cloud_schema().expect("init cloud schema");
    TestContext { db, path }
}

#[test]
fn test_message_system_role_or_extra_field_rejected() {
    // 1. Role = system is strictly rejected
    let system_msg = json!({
        "schema_version": 1,
        "conversation_id": "conv-1",
        "role": "system",
        "content": "You are a helpful assistant",
        "created_at": 1000
    });
    let res = validate_sync_payload_strictly("message", SyncOperation::Upsert, Some(&system_msg));
    assert!(res.is_err(), "role=system must be rejected");

    // 2. Extra unapproved field (e.g. api_key) is rejected via deny_unknown_fields
    let extra_field_msg = json!({
        "schema_version": 1,
        "conversation_id": "conv-1",
        "role": "user",
        "content": "Hello",
        "created_at": 1000,
        "api_key": "sk-secret-123"
    });
    let res =
        validate_sync_payload_strictly("message", SyncOperation::Upsert, Some(&extra_field_msg));
    assert!(res.is_err(), "extra api_key field must be rejected");

    // 3. Valid user message succeeds
    let valid_user_msg = json!({
        "schema_version": 1,
        "conversation_id": "conv-1",
        "role": "user",
        "content": "Hello world",
        "created_at": 1000
    });
    let res =
        validate_sync_payload_strictly("message", SyncOperation::Upsert, Some(&valid_user_msg));
    assert!(res.is_ok(), "valid user message must be accepted");

    // 4. Valid assistant message succeeds
    let valid_asst_msg = json!({
        "schema_version": 1,
        "conversation_id": "conv-1",
        "role": "assistant",
        "content": "Hi there!",
        "created_at": 1001
    });
    let res =
        validate_sync_payload_strictly("message", SyncOperation::Upsert, Some(&valid_asst_msg));
    assert!(res.is_ok(), "valid assistant message must be accepted");

    // 5. validate_message helper matches
    assert!(validate_message(&system_msg).is_err());
    assert!(validate_message(&extra_field_msg).is_err());
    assert!(validate_message(&valid_user_msg).is_ok());
}

#[test]
fn test_unknown_fields_in_nested_structures_rejected() {
    // 1. Conversation with injected secret or unknown field
    let conv_extra = json!({
        "schema_version": 1,
        "title": "Secret Chat",
        "created_at": 100,
        "updated_at": 200,
        "secret_token": "token-xyz"
    });
    assert!(validate_sync_payload_strictly(
        "conversation",
        SyncOperation::Upsert,
        Some(&conv_extra)
    )
    .is_err());

    // 2. Agent profile with unknown field
    let profile_extra = json!({
        "schema_version": 1,
        "name": "Custom Bot",
        "description": "desc",
        "instructions": "inst",
        "max_iterations": 10,
        "enabled": true,
        "provider_key": "hidden"
    });
    assert!(validate_sync_payload_strictly(
        "agent_profile",
        SyncOperation::Upsert,
        Some(&profile_extra)
    )
    .is_err());

    // 3. Prohibited settings sections
    let settings_secret = json!({
        "schema_version": 1,
        "section": "api_keys",
        "preferences": { "openai": "sk-12345" }
    });
    assert!(validate_sync_payload_strictly(
        "settings",
        SyncOperation::Upsert,
        Some(&settings_secret)
    )
    .is_err());

    // 4. Allowed settings section
    let settings_ui = json!({
        "schema_version": 1,
        "section": "ui",
        "preferences": { "theme": "dark" }
    });
    assert!(
        validate_sync_payload_strictly("settings", SyncOperation::Upsert, Some(&settings_ui))
            .is_ok()
    );
}

#[test]
fn test_user_disable_message_sync_does_not_create_deletes() {
    let ctx = setup_test_db();
    let db = &ctx.db;

    // Insert conversation and messages in a closed scope so conn lock is dropped
    {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO conversations (id, title, created_at, updated_at) VALUES ('conv-1', 'Sync Test', 1000, 1000)",
            [],
        )
        .expect("insert conv");
        conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES ('msg-user-1', 'conv-1', 'user', 'Hello user', 1000)",
            [],
        )
        .expect("insert user msg");
        conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES ('msg-sys-1', 'conv-1', 'system', 'Internal instruction', 1001)",
            [],
        )
        .expect("insert sys msg");
    }

    // 1. Capture with full policy
    let full_policy = SyncPolicy::default();
    let rep1 = db
        .capture_sync_candidates_with_policy(&full_policy)
        .expect("capture full");
    assert!(rep1.captured >= 2, "conversation + user message captured");

    // Check outbox has no 'delete' changes
    {
        let conn = db.conn();
        let delete_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM cloud_sync_outbox WHERE operation='delete'",
                [],
                |r| r.get(0),
            )
            .expect("count deletes");
        assert_eq!(delete_count, 0, "no deletes should be generated initially");
    }

    // 2. Disable message sync in policy
    let mut no_msg_policy = SyncPolicy::default();
    no_msg_policy.sync_messages = false;

    let rep2 = db
        .capture_sync_candidates_with_policy(&no_msg_policy)
        .expect("capture without messages");
    assert!(
        rep2.skipped_by_policy >= 1,
        "messages must be skipped by policy"
    );

    // Verify outbox STILL has NO 'delete' changes for messages!
    {
        let conn = db.conn();
        let delete_count_after: i64 = conn
            .query_row(
                "SELECT count(*) FROM cloud_sync_outbox WHERE operation='delete'",
                [],
                |r| r.get(0),
            )
            .expect("count deletes after policy change");
        assert_eq!(
            delete_count_after, 0,
            "policy exclusion must NEVER infer or create outbox delete operations"
        );
    }
}

#[test]
fn test_local_delete_memory_records_tombstone() {
    let ctx = setup_test_db();
    let db = &ctx.db;
    let memory_id = "mem-1";

    {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO memories (id, content, category, source, created_at, updated_at) VALUES ('mem-1', 'I prefer dark mode', 'preference', 'user', 1000, 1000)",
            [],
        )
        .expect("insert memory");
    }

    // Capture initial upsert
    let report = db
        .capture_sync_candidates_with_policy(&SyncPolicy::default())
        .expect("capture memory");
    assert!(report.captured >= 1);

    {
        let conn = db.conn();
        let pending_upserts: i64 = conn
            .query_row(
                "SELECT count(*) FROM cloud_sync_outbox WHERE entity_type='memory' AND operation='upsert'",
                [],
                |r| r.get(0),
            )
            .expect("query upsert");
        assert_eq!(pending_upserts, 1);
    }

    // Delete memory locally
    db.delete_memory(memory_id).expect("delete memory");

    // Verify tombstone is recorded in cloud_sync_outbox
    {
        let conn = db.conn();
        let (op, payload): (String, Option<String>) = conn
            .query_row(
                "SELECT operation, payload_json FROM cloud_sync_outbox WHERE entity_type='memory' AND entity_id=?1 AND operation='delete'",
                params![memory_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("query tombstone in outbox");
        assert_eq!(op, "delete");
        assert!(payload.is_none(), "tombstone payload must be NULL");

        // Verify cloud_entity_revisions shows deleted=1
        let deleted_flag: i64 = conn
            .query_row(
                "SELECT deleted FROM cloud_entity_revisions WHERE entity_type='memory' AND entity_id=?1",
                params![memory_id],
                |r| r.get(0),
            )
            .expect("query revision");
        assert_eq!(deleted_flag, 1);
    }
}

#[test]
fn test_capability_usage_idempotency_and_recording() {
    let ctx = setup_test_db();
    let db = &ctx.db;

    let record = UsageRecord {
        execution_id: "exec-unique-001".to_string(),
        capability_id: "calculator".to_string(),
        plugin_id: "math_tools".to_string(),
        version: "1.0.0".to_string(),
        used_at: 1700000000000,
        execution_status: "succeeded".to_string(),
        latency_ms: 45,
        approval_required: false,
        risk_level: "low".to_string(),
        result_digest: None,
    };

    {
        let mut conn = db.conn();
        let tx = conn.transaction().expect("begin tx");
        record_capability_usage_tx(&tx, &record).expect("record usage 1");
        // Replay same execution_id
        record_capability_usage_tx(&tx, &record).expect("record usage 2 (replay)");
        tx.commit().expect("commit tx");
    }

    // Verify idempotency: exactly 1 record in table
    let list = list_capability_usage(db, 10).expect("list usage");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].execution_id, "exec-unique-001");
    assert_eq!(list[0].execution_status, "succeeded");

    // Test remote projection idempotency
    let proj = SyncCapabilityUsagePayload {
        schema_version: 1,
        execution_id: "exec-remote-002".to_string(),
        capability_id: "file_reader".to_string(),
        plugin_id: "core".to_string(),
        version: "1.0.0".to_string(),
        used_at: 1700000005000,
        execution_status: "denied".to_string(),
        latency_ms: 12,
        approval_required: true,
        risk_level: "high".to_string(),
        result_digest: None,
    };

    {
        let mut conn2 = db.conn();
        let tx2 = conn2.transaction().expect("begin tx2");
        project_usage_tx(&tx2, &proj).expect("project usage");
        project_usage_tx(&tx2, &proj).expect("project usage duplicate");
        tx2.commit().expect("commit tx2");
    }

    let list2 = list_capability_usage(db, 10).expect("list usage 2");
    assert_eq!(list2.len(), 2);
}

#[test]
fn test_corrupt_grant_fields_fail_closed() {
    let ctx = setup_test_db();
    let db = &ctx.db;

    // 1. Corrupt effect
    {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO security_grants (grant_id, subject_id, effect, permission_id, resource_json, source, created_at, expires_at)
             VALUES ('g-corrupt-effect', 'user-1', 'invalid_effect_enum', 'filesystem.read', '{}', 'user', 100, NULL)",
            [],
        )
        .expect("insert corrupt effect");
    }

    assert!(
        db.list_grants("user-1").is_err(),
        "corrupt effect must fail list_grants closed"
    );
    assert!(
        db.get_grant("g-corrupt-effect").is_err(),
        "corrupt effect must fail get_grant closed"
    );

    // Clean up
    {
        let conn = db.conn();
        conn.execute(
            "DELETE FROM security_grants WHERE grant_id='g-corrupt-effect'",
            [],
        )
        .expect("cleanup");
    }

    // 2. Corrupt permission
    {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO security_grants (grant_id, subject_id, effect, permission_id, resource_json, source, created_at, expires_at)
             VALUES ('g-corrupt-perm', 'user-1', 'allow', 'unknown_permission_abc', '{}', 'user', 100, NULL)",
            [],
        )
        .expect("insert corrupt permission");
    }

    assert!(
        db.list_grants("user-1").is_err(),
        "corrupt permission must fail closed"
    );
    assert!(
        db.get_grant("g-corrupt-perm").is_err(),
        "corrupt permission must fail closed"
    );

    // Clean up
    {
        let conn = db.conn();
        conn.execute(
            "DELETE FROM security_grants WHERE grant_id='g-corrupt-perm'",
            [],
        )
        .expect("cleanup");
    }

    // 3. Corrupt resource_json
    {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO security_grants (grant_id, subject_id, effect, permission_id, resource_json, source, created_at, expires_at)
             VALUES ('g-corrupt-res', 'user-1', 'allow', 'filesystem.read', 'not-valid-json{', 'user', 100, NULL)",
            [],
        )
        .expect("insert corrupt resource");
    }

    assert!(
        db.list_grants("user-1").is_err(),
        "corrupt resource_json must fail closed"
    );
    assert!(
        db.get_grant("g-corrupt-res").is_err(),
        "corrupt resource_json must fail closed"
    );

    // Clean up
    {
        let conn = db.conn();
        conn.execute(
            "DELETE FROM security_grants WHERE grant_id='g-corrupt-res'",
            [],
        )
        .expect("cleanup");
    }

    // 4. Corrupt source
    {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO security_grants (grant_id, subject_id, effect, permission_id, resource_json, source, created_at, expires_at)
             VALUES ('g-corrupt-src', 'user-1', 'allow', 'filesystem.read', '{}', 'unknown_source_xyz', 100, NULL)",
            [],
        )
        .expect("insert corrupt source");
    }

    assert!(
        db.list_grants("user-1").is_err(),
        "corrupt source must fail closed"
    );
    assert!(
        db.get_grant("g-corrupt-src").is_err(),
        "corrupt source must fail closed"
    );
}

#[test]
fn test_valid_allow_deny_expired_grants() {
    let ctx = setup_test_db();
    let db = &ctx.db;

    let grant1 = SecurityGrant {
        id: "grant-allow-1".to_string(),
        subject_id: "subject-test".to_string(),
        effect: GrantEffect::Allow,
        permission: PermissionId::FilesystemRead,
        resource: GrantResource::Filesystem {
            root: "F:/safe".to_string(),
            recursive: true,
        },
        source: GrantSource::User,
        created_at: 1000,
        expires_at: Some(2000),
    };

    let grant2 = SecurityGrant {
        id: "grant-deny-2".to_string(),
        subject_id: "subject-test".to_string(),
        effect: GrantEffect::Deny,
        permission: PermissionId::ProcessControl,
        resource: GrantResource::Process {
            scope: ProcessGrantScope::ManagedChildren,
        },
        source: GrantSource::System,
        created_at: 1050,
        expires_at: None,
    };

    db.create_grant(&grant1).expect("create grant 1");
    db.create_grant(&grant2).expect("create grant 2");

    let list = db.list_grants("subject-test").expect("list grants");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, "grant-allow-1");
    assert_eq!(list[0].effect, GrantEffect::Allow);
    assert_eq!(list[1].id, "grant-deny-2");
    assert_eq!(list[1].effect, GrantEffect::Deny);

    let fetched = db
        .get_grant("grant-allow-1")
        .expect("get grant 1")
        .expect("found");
    assert_eq!(fetched.id, "grant-allow-1");
    assert_eq!(fetched.expires_at, Some(2000));
}
