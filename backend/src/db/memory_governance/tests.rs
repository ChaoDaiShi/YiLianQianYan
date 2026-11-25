use super::*;
use crate::db::{CreateMemoryRequest, MemoryQuery, RetrieveQuery};

fn database() -> Database {
    Database::new(std::path::Path::new(":memory:")).unwrap()
}
fn create(db: &Database, content: &str) -> Memory {
    db.create_memory(&CreateMemoryRequest {
        content: content.into(),
        category: "preference".into(),
        source: "manual".into(),
        source_conversation_id: None,
        metadata: None,
    })
    .unwrap()
}
fn correct(db: &Database, memory: &Memory, content: &str, revision: i64) -> Result<Memory, String> {
    db.correct_memory(
        &memory.id,
        &CorrectMemoryRequest {
            content: content.into(),
            expected_revision: revision,
            reason: "user correction".into(),
            scope: None,
            source_ref: None,
        },
    )
}

#[test]
fn correction_preserves_versions_and_rejects_stale_revision() {
    let db = database();
    let memory = create(&db, "喜欢喝咖啡");
    correct(&db, &memory, "不喜欢喝咖啡", 1).unwrap();
    assert_eq!(db.memory_governance(&memory.id).unwrap().revision, 2);
    assert_eq!(
        db.memory_history(&memory.id).unwrap()[1].content,
        "喜欢喝咖啡"
    );
    assert_eq!(
        correct(&db, &memory, "喜欢喝茶", 1).unwrap_err(),
        "memory_revision_conflict"
    );
    assert_eq!(db.get_memory(&memory.id).unwrap().content, "不喜欢喝咖啡");
}

#[test]
fn repeated_confirmation_does_not_create_an_extra_revision_or_change_reason() {
    let db = database();
    let memory = create(&db, "无变化的记忆");
    let unchanged = correct(&db, &memory, "  无变化的记忆  ", 1).unwrap();
    assert_eq!(unchanged.updated_at, memory.updated_at);
    assert_eq!(db.memory_governance(&memory.id).unwrap().revision, 1);
    assert_eq!(db.memory_governance(&memory.id).unwrap().reason, "");
    assert_eq!(db.memory_history(&memory.id).unwrap().len(), 1);
}

#[test]
fn layers_are_local_to_each_memory_and_unchanged_edits_leave_no_trace() {
    let db = database();
    let a = create(&db, "A 原稿");
    let b = create(&db, "B 原稿");
    correct(&db, &a, "A 修订一", 1).unwrap();
    correct(&db, &a, "A 修订二", 2).unwrap();
    let ids = [a.id.clone(), b.id.clone()];
    let histories = db.memory_histories(&ids).unwrap();
    let layers = |id: &str| {
        histories[id]
            .iter()
            .map(|version| (version.layer, version.content.as_str()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        layers(&a.id),
        [(2, "A 修订二"), (1, "A 修订一"), (0, "A 原稿")]
    );
    assert_eq!(layers(&b.id), [(0, "B 原稿")]);
    assert!(!histories[&b.id][0].baseline);
    assert_eq!(histories[&a.id][2].version_id, format!("{}:1", a.id));
    correct(&db, &b, "B 原稿", 1).unwrap();
    correct(&db, &b, "B 修订", 1).unwrap();
    assert_eq!(db.memory_history(&b.id).unwrap()[0].layer, 1);
    assert_eq!(db.memory_history(&a.id).unwrap().len(), 3);
}

#[test]
fn relationship_snapshot_tracks_edits_and_clears_with_the_memory() {
    let db = database();
    let m = db
        .create_memory(&CreateMemoryRequest {
            content: "接口验收 · 依赖 · 材料整理".into(),
            category: "fact".into(),
            source: "manual".into(),
            source_conversation_id: None,
            metadata: Some(
                serde_json::json!({
                    "relationship": {"kind":"project","subject":"接口验收","object":"材料整理",
                        "label":"依赖","directed":true}
                })
                .to_string(),
            ),
        })
        .unwrap();
    let update = |directed: bool| {
        db.update_memory(&m.id, &crate::db::UpdateMemoryRequest {
            content: Some(format!(" {} ", m.content)),
            category: Some(m.category.clone()),
            metadata: Some(format!(
                r#"{{"relationship":{{ "label":"依赖", "directed":{directed}, "subject":"接口验收", "object":"材料整理", "kind":"project" }} }}"#
            )),
        }).unwrap()
    };
    assert_eq!(update(true).updated_at, m.updated_at);
    update(false);
    let history = db.memory_history(&m.id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(db.memory_governance(&m.id).unwrap().revision, 2);
    assert_eq!(history[0].relationship.as_ref().unwrap()["directed"], false);
    assert_eq!(history[1].relationship.as_ref().unwrap()["directed"], true);
    assert!(history.iter().all(|version| version.relationship_known));
    db.delete_memory(&m.id).unwrap();
    assert!(db.memory_history(&m.id).unwrap().is_empty());
    assert_eq!(
        db.conn()
            .query_row(
                "SELECT count(*) FROM memory_version_details WHERE memory_id=?1",
                [&m.id],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
        0,
    );
}

#[test]
fn deleting_the_source_removes_history_links_without_increasing_layers() {
    let db = database();
    let conversation = db.create_conversation("版本来源").unwrap();
    let message_id = uuid::Uuid::new_v4().to_string();
    db.add_message(&crate::db::MessageRow {
        id: message_id.clone(),
        conversation_id: conversation.id.clone(),
        role: "user".into(),
        content: "为版本历史提供来源".into(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_result: None,
        created_at: 1,
    })
    .unwrap();
    let m = db
        .create_memory(&CreateMemoryRequest {
            content: "可以独立保留的记忆".into(),
            category: "fact".into(),
            source: "manual".into(),
            source_conversation_id: Some(conversation.id.clone()),
            metadata: Some(serde_json::json!({"source_message_id":message_id}).to_string()),
        })
        .unwrap();
    let history = db.memory_history(&m.id).unwrap();
    assert_eq!(
        history[0].source_conversation_id.as_deref(),
        Some(conversation.id.as_str())
    );
    assert_eq!(
        history[0].source_message_id.as_deref(),
        Some(message_id.as_str())
    );
    assert!(!history[0].source_removed);
    db.delete_conversation(&conversation.id).unwrap();
    let history = db.memory_history(&m.id).unwrap();
    assert_eq!(history.len(), 1);
    assert!(history[0].source_removed);
    assert!(history[0].source_conversation_id.is_none());
    assert!(history[0].source_message_id.is_none());
    assert_eq!(db.memory_governance(&m.id).unwrap().revision, 1);
    assert_eq!(db.get_memory(&m.id).unwrap().content, m.content);
}

#[test]
fn first_remote_snapshot_is_a_baseline_even_when_no_edits_are_visible() {
    use crate::modules::cloud::domain::SyncOperation;
    let db = database();
    let payload = |content: &str| {
        serde_json::json!({
            "schema_version":1, "content":content, "category":"note", "source":"manual",
            "created_at": 1, "updated_at": 1
        })
    };
    db.apply_remote_sync_change(
        "memory",
        "incoming",
        1,
        SyncOperation::Upsert,
        Some(&payload("设备间的当前快照")),
        1,
    )
    .unwrap();
    let first = db.memory_history("incoming").unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].layer, 0);
    assert!(first[0].baseline);
    assert!(!first[0].relationship_known);
    db.apply_remote_sync_change(
        "memory",
        "incoming",
        2,
        SyncOperation::Upsert,
        Some(&payload("设备间的下一次实际变化")),
        2,
    )
    .unwrap();
    let history = db.memory_history("incoming").unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].layer, 1);
    assert!(!history[0].baseline);
    assert!(history[1].baseline);
}

#[test]
fn negation_is_not_duplicate_and_normalized_equal_is_idempotent() {
    let db = database();
    let original = create(&db, "likes coffee");
    assert_eq!(create(&db, "  LIKES   coffee ").id, original.id);
    assert_ne!(create(&db, "dislikes coffee").id, original.id);
    assert_ne!(create(&db, "no longer likes coffee").id, original.id);
}

#[test]
fn embedding_cas_rejects_content_and_model_generation_changes() {
    let db = database();
    let original = create(&db, "咖啡偏好");
    let old = db.begin_memory_embedding(&original, "model-a").unwrap();
    let updated = correct(&db, &original, "茶饮偏好", 1).unwrap();
    assert_eq!(
        db.complete_memory_embedding(&old, &[1.0, 0.0]).unwrap_err(),
        "stale_embedding_result"
    );
    let a = db.begin_memory_embedding(&updated, "model-a").unwrap();
    let b = db.begin_memory_embedding(&updated, "model-b").unwrap();
    assert!(db.complete_memory_embedding(&a, &[1.0, 0.0]).is_err());
    db.complete_memory_embedding(&b, &[1.0, 0.0]).unwrap();
    let query = RetrieveQuery {
        q: "unrelated".into(),
        top_k: 10,
        category: None,
    };
    assert!(db
        .retrieve_scoped_memories(&query, Some(&[1.0, 0.0]), Some("model-a"), None, true)
        .unwrap()
        .is_empty());
    assert_eq!(
        db.retrieve_scoped_memories(&query, Some(&[1.0, 0.0]), Some("model-b"), None, true)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn withdrawal_blocks_retrieval_and_inflight_embeddings_delete_erases_history() {
    let db = database();
    let original = create(&db, "长期语言偏好");
    let ticket = db.begin_memory_embedding(&original, "model").unwrap();
    db.withdraw_memory(
        &original.id,
        &WithdrawMemoryRequest {
            expected_revision: 1,
            reason: "no longer valid".into(),
        },
    )
    .unwrap();
    assert!(db.complete_memory_embedding(&ticket, &[1.0]).is_err());
    assert_eq!(db.memory_history(&original.id).unwrap().len(), 2);
    assert!(db
        .retrieve_memories(&RetrieveQuery {
            q: "长期语言偏好".into(),
            top_k: 10,
            category: None
        })
        .unwrap()
        .is_empty());
    assert!(db
        .list_memories(&MemoryQuery {
            category: None,
            source: None,
            q: None,
            limit: None,
            offset: None
        })
        .unwrap()
        .is_empty());
    assert_eq!(db.withdrawn_memories().unwrap().len(), 1);
    db.delete_memory(&original.id).unwrap();
    assert!(db.memory_history(&original.id).unwrap().is_empty());
}

#[test]
fn retrieval_includes_matching_memory_older_than_500_candidates() {
    let db = database();
    let old = create(&db, "独特远古记忆信标");
    db.conn()
        .execute("UPDATE memories SET updated_at=1 WHERE id=?1", [&old.id])
        .unwrap();
    for index in 0..505 {
        create(&db, &format!("ordinary entry {index}"));
    }
    let matches = db
        .retrieve_memories(&RetrieveQuery {
            q: "独特远古记忆信标".into(),
            top_k: 1,
            category: None,
        })
        .unwrap();
    assert_eq!(matches[0].memory.id, old.id);
}

#[test]
fn profile_databases_and_conversation_scopes_are_separate() {
    let db = database();
    let other = database();
    let memory = create(&db, "档案私有偏好");
    assert!(other.find_memory(&memory.id).unwrap().is_none());
    db.conn()
        .execute(
            "UPDATE memory_governance SET scope='conversation:owner' WHERE memory_id=?1",
            [&memory.id],
        )
        .unwrap();
    let query = RetrieveQuery {
        q: "档案私有偏好".into(),
        top_k: 10,
        category: None,
    };
    assert!(db
        .retrieve_scoped_memories(&query, None, None, None, true)
        .unwrap()
        .is_empty());
    assert!(db
        .retrieve_scoped_memories(&query, None, None, Some("other"), true)
        .unwrap()
        .is_empty());
    assert_eq!(
        db.retrieve_scoped_memories(&query, None, None, Some("owner"), true)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn bound_agent_cannot_reintroduce_withdrawn_or_other_conversation_memory() {
    let db = database();
    let memory = create(&db, "绑定的长期偏好");
    db.conn().execute("INSERT INTO agent_definitions(id,name,description,instructions,allowed_tools,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES('memory-test-agent','Agent','','','[]','[]',10,1,'user',1,1)",[]).unwrap();
    db.conn().execute("INSERT INTO connector_bindings(id,agent_id,kind,source_id,target_id) VALUES('memory-test-binding','memory-test-agent','agent_memory','agent:memory-test-agent',?1)",[format!("memory:{}",memory.id)]).unwrap();
    assert_eq!(
        db.connector_runtime_bindings("memory-test-agent")
            .unwrap()
            .memories
            .len(),
        1
    );
    db.conn()
        .execute(
            "UPDATE memory_governance SET scope='conversation:owner' WHERE memory_id=?1",
            [&memory.id],
        )
        .unwrap();
    assert!(db
        .connector_runtime_bindings("memory-test-agent")
        .unwrap()
        .memories
        .is_empty());
    assert_eq!(
        db.connector_runtime_bindings_for_conversation("memory-test-agent", Some("owner"))
            .unwrap()
            .memories
            .len(),
        1
    );
    db.withdraw_memory(
        &memory.id,
        &WithdrawMemoryRequest {
            expected_revision: 1,
            reason: "withdrawn".into(),
        },
    )
    .unwrap();
    assert!(db
        .connector_runtime_bindings_for_conversation("memory-test-agent", Some("owner"))
        .unwrap()
        .memories
        .is_empty());
}

#[test]
fn workspace_scope_is_adopted_only_inside_its_real_project() {
    let db = database();
    let memory = create(&db, "项目专用偏好");
    db.conn().execute("INSERT INTO workspaces(id,name,status,created_at,updated_at) VALUES('project-a','Project A','active',1,1)",[]).unwrap();
    db.correct_memory(
        &memory.id,
        &CorrectMemoryRequest {
            content: memory.content.clone(),
            expected_revision: 1,
            reason: "project-only".into(),
            scope: Some("workspace:project-a".into()),
            source_ref: None,
        },
    )
    .unwrap();
    let query = RetrieveQuery {
        q: "项目专用偏好".into(),
        top_k: 5,
        category: None,
    };
    assert!(db
        .retrieve_scoped_memories(&query, None, None, None, true)
        .unwrap()
        .is_empty());
    assert!(db
        .retrieve_workspace_memories(&query, None, None, "project-b")
        .unwrap()
        .is_empty());
    assert_eq!(
        db.retrieve_workspace_memories(&query, None, None, "project-a")
            .unwrap()
            .len(),
        1
    );
    assert!(!db
        .memory_is_available_in_workspace(&memory.id, None, Some("project-b"))
        .unwrap());
}

#[test]
fn chat_memory_changes_require_the_records_conversation_scope_and_revision() {
    let db = database();
    let owner = db.create_conversation("owner").unwrap();
    let other = db.create_conversation("other").unwrap();
    let memory = create(&db, "导出 CSV 使用 GBK 编码");
    db.correct_memory(
        &memory.id,
        &CorrectMemoryRequest {
            content: memory.content.clone(),
            expected_revision: 1,
            reason: "scope change".into(),
            scope: Some(format!("conversation:{}", owner.id)),
            source_ref: None,
        },
    )
    .unwrap();
    let correction = CorrectMemoryRequest {
        content: "导出 CSV 使用 UTF-8 编码".into(),
        expected_revision: 2,
        reason: "user correction".into(),
        scope: None,
        source_ref: None,
    };
    assert_eq!(
        db.correct_memory_scoped(&memory.id, &correction, Some(&other.id))
            .unwrap_err(),
        "memory_not_in_scope"
    );
    db.correct_memory_scoped(&memory.id, &correction, Some(&owner.id))
        .unwrap();
    let withdrawal = WithdrawMemoryRequest {
        expected_revision: 2,
        reason: "user withdrawal".into(),
    };
    assert_eq!(
        db.withdraw_memory_scoped(&memory.id, &withdrawal, Some(&owner.id))
            .unwrap_err(),
        "memory_revision_conflict"
    );
    let current = db.memory_governance(&memory.id).unwrap();
    assert_eq!(current.scope, format!("conversation:{}", owner.id));
    assert_eq!((current.status.as_str(), current.revision), ("active", 3));
}
