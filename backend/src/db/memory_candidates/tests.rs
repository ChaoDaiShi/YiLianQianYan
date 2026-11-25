use super::*;

#[test]
fn saved_text_relationship_is_reviewed_graph_data_pinned_to_a_version() {
    use crate::db::UpdateMemoryRequest;
    let (db, _, _) = fixture();
    let source = db
        .create_memory(&CreateMemoryRequest {
            content: "我偏好番茄图配简体中文。".into(),
            category: "preference".into(),
            source: "manual".into(),
            source_conversation_id: None,
            metadata: None,
        })
        .unwrap();
    let relation: MemoryRelationship = serde_json::from_value(serde_json::json!({
        "kind":"preference","subject":"我","label":"偏好","object":"简体中文","directed":true,"certainty":"confirmed"
    })).unwrap();
    let proposal = (relation.clone(), 0, source.content.chars().count());
    let ids = db
        .stage_saved_text_relationships(&source.id, 1, vec![proposal.clone()])
        .unwrap();
    assert_eq!(ids.len(), 1);
    assert!(Database::extracted_memory_relationships_on(
        &db.conn(),
        std::slice::from_ref(&source.id)
    )
    .unwrap()
    .is_empty());
    let draft = db.memory_candidates(10, 0).unwrap().pop().unwrap();
    assert_eq!(draft.source_memory_id.as_deref(), Some(source.id.as_str()));
    assert_eq!(draft.source_memory_revision, Some(1));
    assert_eq!(
        draft.source_excerpt.as_deref(),
        Some(source.content.as_str())
    );
    assert_eq!(
        draft.relationship.unwrap().certainty,
        RelationshipCertainty::Inferred
    );
    assert!(db
        .stage_saved_text_relationships(&source.id, 1, vec![proposal])
        .unwrap()
        .is_empty());
    let saved = db
        .accept_memory_candidate(
            &ids[0],
            AcceptCandidate {
                relationship: Some(relation.clone()),
                ..review()
            },
        )
        .unwrap();
    assert_eq!(saved.id, source.id);
    assert_eq!(db.get_memory_stats().unwrap().total, 1);
    assert_eq!(db.memory_history(&source.id).unwrap().len(), 1);
    let edges =
        Database::extracted_memory_relationships_on(&db.conn(), std::slice::from_ref(&source.id))
            .unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].source_memory_revision, 1);
    assert_eq!(
        edges[0].relationship.certainty,
        RelationshipCertainty::Confirmed
    );
    assert_eq!(edges[0].evidence_end, source.content.chars().count());

    let edge = &edges[0].id;
    let correction = db.stage_saved_text_correction(&source.id, edge).unwrap();
    assert_eq!(correction.len(), 1);
    assert!(db
        .stage_saved_text_correction(&source.id, edge)
        .unwrap()
        .is_empty());
    assert!(db.dismiss_memory_candidate(&correction[0]).unwrap());
    let retry = db.stage_saved_text_correction(&source.id, edge).unwrap();
    assert_eq!(retry.len(), 1);
    let mut corrected = relation.clone();
    corrected.label = "更喜欢".into();
    db.accept_memory_candidate(
        &retry[0],
        AcceptCandidate {
            relationship: Some(corrected),
            ..review()
        },
    )
    .unwrap();
    let edges =
        Database::extracted_memory_relationships_on(&db.conn(), std::slice::from_ref(&source.id))
            .unwrap();
    assert_eq!(edges[0].id, *edge);
    assert_eq!(edges[0].relationship.label, "更喜欢");
    assert_eq!(db.memory_history(&source.id).unwrap().len(), 1);

    let mut changed_relation = relation;
    changed_relation.label = "常用".into();
    let outstanding = db
        .stage_saved_text_relationships(
            &source.id,
            1,
            vec![(changed_relation, 0, source.content.chars().count())],
        )
        .unwrap();
    assert_eq!(outstanding.len(), 1);
    db.update_memory(
        &source.id,
        &UpdateMemoryRequest {
            content: Some("我现在常用法语。".into()),
            category: None,
            metadata: None,
        },
    )
    .unwrap();
    let invalid = db.memory_candidates(10, 0).unwrap().pop().unwrap();
    assert!(invalid.source_removed && invalid.source_excerpt.is_none());
    assert_eq!(
        db.accept_memory_candidate(&outstanding[0], review())
            .unwrap_err(),
        "memory_candidate_source_changed"
    );
    assert!(Database::extracted_memory_relationships_on(
        &db.conn(),
        std::slice::from_ref(&source.id)
    )
    .unwrap()
    .is_empty());
    db.delete_memory(&source.id).unwrap();
    assert!(db.memory_candidates(10, 0).unwrap().is_empty());
    assert_eq!(
        db.conn()
            .query_row(
                "SELECT count(*) FROM memory_text_relationships",
                [],
                |row| row.get::<_, usize>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn saved_text_relationship_purge_replays_on_restored_snapshot() {
    let directory =
        std::env::temp_dir().join(format!("yilian-relation-purge-{}", uuid::Uuid::new_v4()));
    let db_path = directory.join("profile.db");
    let backup = directory.join("backup.db");
    let copy = |from: &std::path::Path, to: &std::path::Path| {
        let origin = rusqlite::Connection::open(from).unwrap();
        let mut target = rusqlite::Connection::open(to).unwrap();
        rusqlite::backup::Backup::new(&origin, &mut target)
            .unwrap()
            .run_to_completion(5, std::time::Duration::from_millis(1), None)
            .unwrap();
    };
    let (source_id, kept_id) = {
        let db = Database::new(&db_path).unwrap();
        let create = |content: &str| {
            db.create_memory(&CreateMemoryRequest {
                content: content.into(),
                category: "note".into(),
                source: "manual".into(),
                source_conversation_id: None,
                metadata: None,
            })
            .unwrap()
        };
        let source = create("甲项目必须等乙项目交付");
        let kept = create("邻居记录正常保留");
        let relation: MemoryRelationship = serde_json::from_value(serde_json::json!({
            "kind":"project","subject":"甲项目","label":"依赖","object":"乙项目","directed":true,"certainty":"confirmed"
        })).unwrap();
        let ids = db
            .stage_saved_text_relationships(
                &source.id,
                1,
                vec![(relation.clone(), 0, source.content.chars().count())],
            )
            .unwrap();
        db.accept_memory_candidate(
            &ids[0],
            AcceptCandidate {
                relationship: Some(relation),
                ..review()
            },
        )
        .unwrap();
        let graph = Database::extracted_memory_relationships_on(
            &db.conn(),
            std::slice::from_ref(&source.id),
        )
        .unwrap();
        assert_eq!(graph.len(), 1);
        assert_eq!(
            db.stage_saved_text_correction(&source.id, &graph[0].id)
                .unwrap()
                .len(),
            1
        );
        copy(&db_path, &backup);
        db.delete_memory(&source.id).unwrap();
        (source.id, kept.id)
    };
    copy(&backup, &db_path);
    let db = Database::new(&db_path).unwrap();
    assert!(db.find_memory(&source_id).unwrap().is_none());
    assert!(db.find_memory(&kept_id).unwrap().is_some());
    assert!(db.memory_candidates(40, 0).unwrap().is_empty());
    assert_eq!(
        db.conn()
            .query_row(
                "SELECT count(*) FROM memory_text_relationship_history",
                [],
                |row| row.get::<_, usize>(0)
            )
            .unwrap(),
        0
    );
    drop(db);
    // Only our newly created UUID test path is owned by this test.
    std::fs::remove_dir_all(directory).unwrap();
}

fn fixture() -> (Database, String, String) {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let conversation = db.create_conversation("记忆候选来源").unwrap().id;
    let message = uuid::Uuid::new_v4().to_string();
    db.add_message(&super::super::MessageRow {
        id: message.clone(),
        conversation_id: conversation.clone(),
        role: "user".into(),
        content: "我一直用 UTF-8 导出 CSV".into(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_result: None,
        created_at: chrono::Utc::now().timestamp_millis(),
    })
    .unwrap();
    (db, conversation, message)
}

fn draft(conversation: &str, message: &str, content: &str) -> CandidateDraft {
    CandidateDraft {
        category: "preference".into(),
        content: content.into(),
        relationship: None,
        source: CandidateSource::Message {
            conversation_id: conversation.into(),
            message_id: message.into(),
        },
    }
}

fn review() -> AcceptCandidate {
    AcceptCandidate {
        content: "导出 CSV 时使用 UTF-8".into(),
        category: "preference".into(),
        scope: MemoryScope::Conversation,
        relationship: None,
    }
}

#[test]
fn review_is_required_and_acceptance_stores_scoped_user_confirmation() {
    let (db, conversation, message) = fixture();
    let id = db
        .stage_memory_candidates(vec![draft(&conversation, &message, "CSV 用 UTF-8")])
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(db.get_memory_stats().unwrap().total, 0);
    let candidates = db.memory_candidates(20, 0).unwrap();
    assert_eq!(
        candidates[0].source_message_id.as_deref(),
        Some(message.as_str())
    );
    assert_eq!(
        candidates[0].source_excerpt.as_deref(),
        Some("我一直用 UTF-8 导出 CSV")
    );
    let saved = db.accept_memory_candidate(&id, review()).unwrap();
    assert_eq!(saved.source, "manual");
    assert_eq!(saved.content, "导出 CSV 时使用 UTF-8");
    assert_eq!(
        db.memory_governance(&saved.id).unwrap().scope,
        format!("conversation:{conversation}")
    );
    assert_eq!(db.memory_history(&saved.id).unwrap().len(), 1);
    assert!(db.memory_candidates(20, 0).unwrap().is_empty());
    assert_eq!(
        db.accept_memory_candidate(&id, review()).unwrap_err(),
        "memory_candidate_not_pending"
    );
    assert!(db
        .stage_memory_candidates(vec![draft(&conversation, &message, "CSV 用 UTF-8")])
        .unwrap()
        .is_empty());
}

#[test]
fn dismissal_and_activity_purge_clear_proposal_bodies() {
    let (db, conversation, message) = fixture();
    let id = db
        .stage_memory_candidates(vec![draft(&conversation, &message, "CSV 用 UTF-8")])
        .unwrap()
        .pop()
        .unwrap();
    assert!(db.dismiss_memory_candidate(&id).unwrap());
    assert!(!db.dismiss_memory_candidate(&id).unwrap());
    assert!(db
        .stage_memory_candidates(vec![draft(&conversation, &message, "CSV 用 UTF-8")])
        .unwrap()
        .is_empty());
    assert_eq!(
        db.stage_memory_candidates(vec![draft(&conversation, &message, "导出要用 UTF-8")])
            .unwrap()
            .len(),
        1
    );
    assert!(db
        .remove_memory_activity(&format!("message:{message}"))
        .unwrap());
    assert!(db.memory_candidates(20, 0).unwrap().is_empty());
    assert!(db
        .stage_memory_candidates(vec![draft(&conversation, &message, "第三种写法")])
        .unwrap()
        .is_empty());
    assert_eq!(
        db.conn()
            .query_row(
                "SELECT COUNT(*) FROM memory_candidates WHERE content IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert!(db.find_conversation(&conversation).unwrap().is_some());
}

#[test]
fn deleted_source_is_visible_as_removed_without_a_usable_link() {
    let (db, conversation, message) = fixture();
    let id = db
        .stage_memory_candidates(vec![draft(&conversation, &message, "CSV 用 UTF-8")])
        .unwrap()
        .pop()
        .unwrap();
    db.delete_conversation(&conversation).unwrap();
    let candidates = db.memory_candidates(20, 0).unwrap();
    assert!(candidates[0].source_removed);
    assert!(candidates[0].source_conversation_id.is_none());
    assert!(candidates[0].source_excerpt.is_none());
    assert_eq!(
        db.accept_memory_candidate(&id, review()).unwrap_err(),
        "memory_candidate_source_removed"
    );
    assert!(db.dismiss_memory_candidate(&id).unwrap());
}

fn relation_draft(conversation: &str, message: &str) -> CandidateDraft {
    let mut proposal = draft(conversation, message, "");
    proposal.category = "fact".into();
    proposal.relationship = Some(
        serde_json::from_value(serde_json::json!({
            "kind":"concept", "subject":"甲", "label":"组成", "object":"乙", "directed":false
        }))
        .unwrap(),
    );
    proposal
}

#[test]
fn structured_review_keeps_graph_identity_and_uses_existing_purge() {
    let (db, conversation, message) = fixture();
    let first = relation_draft(&conversation, &message);
    let mut duplicate = relation_draft(&conversation, &message);
    let relation = duplicate.relationship.as_mut().unwrap();
    std::mem::swap(&mut relation.subject, &mut relation.object);
    let mut another = relation_draft(&conversation, &message);
    another.relationship.as_mut().unwrap().kind = relationship::RelationshipKind::Project;
    let ids = db
        .stage_memory_candidates(vec![first, duplicate, another])
        .unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(db.get_memory_stats().unwrap().total, 0);
    let candidates = db.memory_candidates(20, 0).unwrap();
    for id in &ids {
        let candidate = candidates.iter().find(|value| &value.id == id).unwrap();
        assert_eq!(
            candidate.source_message_id.as_deref(),
            Some(message.as_str())
        );
        let saved = db
            .accept_memory_candidate(
                id,
                AcceptCandidate {
                    relationship: candidate.relationship.clone(),
                    ..review()
                },
            )
            .unwrap();
        assert_eq!(saved.content, "甲 · 组成 · 乙 · 待确认");
        let metadata: serde_json::Value =
            serde_json::from_str(saved.metadata.as_deref().unwrap()).unwrap();
        assert_eq!(metadata["source_message_id"], message.as_str());
        assert_eq!(metadata["relationship"]["certainty"], "inferred");
    }
    assert_eq!(db.get_memory_stats().unwrap().total, 2);
    assert_eq!(
        db.conn()
            .query_row(
                "SELECT count(*) FROM memory_candidates WHERE relationship_json IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let mut third = relation_draft(&conversation, &message);
    third.relationship.as_mut().unwrap().label = "关联".into();
    assert_eq!(db.stage_memory_candidates(vec![third]).unwrap().len(), 1);
    assert!(db
        .remove_memory_activity(&format!("message:{message}"))
        .unwrap());
    assert!(db
        .removed_memory_candidate_sources(&conversation)
        .unwrap()
        .contains(&message));
    assert!(db.memory_candidates(20, 0).unwrap().is_empty());
    assert_eq!(db.conn().query_row("SELECT count(*) FROM memory_candidates WHERE relationship_json IS NOT NULL OR content IS NOT NULL", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
}
