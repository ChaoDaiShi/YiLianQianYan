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
