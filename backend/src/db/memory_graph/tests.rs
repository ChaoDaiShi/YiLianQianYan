use super::*;
use crate::db::{CreateMemoryRequest, UpdateMemoryRequest};

fn create(db: &Database, content: &str) -> Memory {
    db.create_memory(&CreateMemoryRequest {
        content: content.into(),
        category: "fact".into(),
        source: "manual".into(),
        source_conversation_id: None,
        metadata: None,
    })
    .unwrap()
}

fn snapshot(db: &Database) -> MemoryGraphSnapshot {
    db.memory_graph(
        MemoryQuery {
            category: None,
            source: None,
            q: None,
            limit: None,
            offset: None,
        },
        false,
        None,
    )
    .unwrap()
}

#[test]
fn semantic_clues_only_use_current_same_scope_vectors_and_do_not_create_edges() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let a = create(&db, "发版前检查更新计划");
    let b = create(&db, "上线前核对发布顺序");
    let stale = create(&db, "原有发版顺序");
    let deleted = create(&db, "待清除的发版样本");
    let indexed_zero = create(&db, "没有有效语义方向");
    let corrupt = create(&db, "向量形状异常的响应");
    let unrelated = create(&db, "和发版无关的向量");
    let no_vector = create(&db, "尚未向量化的发版计划");
    let chat = db.create_conversation("不同采用范围").unwrap();
    let scoped = db
        .create_memory_with_scope(
            &CreateMemoryRequest {
                content: "仅本对话的发布顺序".into(),
                category: "fact".into(),
                source: "manual".into(),
                source_conversation_id: Some(chat.id.clone()),
                metadata: None,
            },
            crate::db::MemoryScope::Conversation,
        )
        .unwrap();
    let embed = |memory: &Memory, model: &str, vector: &[f32]| {
        let ticket = db.begin_memory_embedding(memory, model).unwrap();
        db.complete_memory_embedding(&ticket, vector).unwrap();
    };
    embed(&stale, "previous-model", &[1.0, 0.0]);
    for record in [&a, &b, &scoped, &deleted] {
        embed(record, "current-model", &[1.0, 0.0]);
    }
    embed(&indexed_zero, "current-model", &[1.0, 0.0]);
    embed(&corrupt, "current-model", &[1.0, 0.0]);
    db.conn()
        .execute(
            "UPDATE memories SET embedding='[0,0]' WHERE id=?1",
            [&indexed_zero.id],
        )
        .unwrap();
    db.conn()
        .execute(
            "UPDATE memories SET embedding='[1,0,0]' WHERE id=?1",
            [&corrupt.id],
        )
        .unwrap();
    let zero_ticket = db
        .begin_memory_embedding(&indexed_zero, "current-model")
        .unwrap();
    assert!(db
        .complete_memory_embedding(&zero_ticket, &[0.0, 0.0])
        .is_err());
    embed(&unrelated, "current-model", &[0.0, 1.0]);
    db.delete_memory(&deleted.id).unwrap();
    let graph = db
        .memory_graph(
            MemoryQuery {
                category: None,
                source: None,
                q: None,
                limit: None,
                offset: None,
            },
            false,
            Some("current-model"),
        )
        .unwrap();
    assert!(graph.semantic.configured);
    assert_eq!(graph.semantic.similarities.len(), 1);
    let similar = &graph.semantic.similarities[0];
    assert_eq!(
        std::collections::HashSet::from([&similar.source_memory_id, &similar.target_memory_id]),
        std::collections::HashSet::from([&a.id, &b.id])
    );
    assert_eq!(similar.score, 1.0);
    assert!(!graph.semantic.indexed_memory_ids.contains(&scoped.id));
    assert!(graph.semantic.indexed_memory_ids.contains(&unrelated.id));
    for invalid in [&stale, &deleted, &indexed_zero, &corrupt, &no_vector] {
        assert!(!graph.semantic.indexed_memory_ids.contains(&invalid.id));
    }
    assert!(graph.relations.is_empty());
    assert!(db.memory_candidates(20, 0).unwrap().is_empty());
    assert_eq!(graph.histories[&a.id].len(), 1);
    let retry = db.memories_needing_embedding("current-model", 20).unwrap();
    assert_eq!(retry.len(), 4);
    for expected in [&stale, &indexed_zero, &corrupt, &no_vector] {
        assert!(retry.iter().any(|record| record.id == expected.id));
    }
    assert!(!snapshot(&db).semantic.configured);
}

#[test]
fn edge_change_does_not_touch_endpoints_and_correction_requires_review() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let a = create(&db, "甲项目原始安排");
    let b = create(&db, "乙项目原始安排");
    let link = db
        .create_memory_link(CreateMemoryLink {
            source_memory_id: a.id.clone(),
            target_memory_id: b.id.clone(),
            label: "检查安排互相对应".into(),
            status: None,
        })
        .unwrap();
    db.update_memory(
        &a.id,
        &UpdateMemoryRequest {
            content: Some("甲项目新的安排".into()),
            category: None,
            metadata: None,
        },
    )
    .unwrap();
    let changed = db
        .change_memory_link(
            &link.id,
            ChangeMemoryLink {
                expected_revision: 1,
                label: Some("对应同一批安排".into()),
                status: None,
            },
        )
        .unwrap();
    assert_eq!(changed.revision, 2);
    assert_eq!(snapshot(&db).histories[&a.id].len(), 2);
    assert_eq!(snapshot(&db).histories[&b.id].len(), 1);
    let stage = |content: &str, revision| StageLinkCorrection {
        target_memory_id: b.id.clone(),
        content: content.into(),
        reason: "新安排已人工核实".into(),
        expected_revision: revision,
        source_version_id: Some(format!("{}:2", a.id)),
    };
    assert_eq!(
        db.stage_link_correction(&link.id, stage(&b.content, 1))
            .unwrap(),
        None
    );
    let candidate = db
        .stage_link_correction(&link.id, stage("乙项目继续复核", 1))
        .unwrap()
        .unwrap();
    assert_eq!(snapshot(&db).histories[&b.id].len(), 1);
    let review = || {
        serde_json::from_value::<crate::db::memory_candidates::AcceptCandidate>(serde_json::json!({
            "content":"乙项目继续复核","category":"fact","scope":"profile"
        }))
        .unwrap()
    };
    assert_eq!(
        db.accept_memory_candidate(&candidate, review()).unwrap().id,
        b.id
    );
    let saved = snapshot(&db);
    assert!(db.accept_memory_candidate(&candidate, review()).is_err());
    assert_eq!(saved.histories[&a.id].len(), 2);
    assert_eq!(saved.histories[&b.id].len(), 2);
    assert_eq!(saved.histories[&b.id][0].reason, "新安排已人工核实");
    let event = saved
        .change_events
        .iter()
        .find(|event| event.memory_id == b.id && event.kind == "reviewed_correction")
        .unwrap();
    assert_eq!(event.version_id, format!("{}:2", b.id));
    assert_eq!(event.related_version_id, Some(format!("{}:2", a.id)));
    assert_eq!(
        event.related_event_id,
        Some(format!("memory-version:{}:2", a.id))
    );
    assert_eq!(event.relation_id, Some(link.id.clone()));
    assert_eq!(saved.relation_versions.len(), 2);
    assert_eq!(
        saved.relation_versions[1].source_version_id,
        format!("{}:2", a.id)
    );

    let pending = db
        .stage_link_correction(&link.id, stage("待重新核对", 2))
        .unwrap()
        .unwrap();
    db.delete_memory(&a.id).unwrap();
    let after = snapshot(&db);
    assert!(!after.histories.contains_key(&a.id));
    assert_eq!(after.histories[&b.id].len(), 2);
    assert!(after.relations.is_empty() && after.relation_versions.is_empty());
    assert!(after
        .change_events
        .iter()
        .all(|event| event.related_event_id.is_none()));
    assert!(db.memory_candidates(10, 0).unwrap().is_empty());
    assert!(db.accept_memory_candidate(&pending, review()).is_err());
    assert_eq!(
        db.conn()
            .query_row(
                "SELECT count(*) FROM memory_graph_link_versions",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

fn unfiltered() -> MemoryQuery {
    MemoryQuery {
        category: None,
        source: None,
        q: None,
        limit: None,
        offset: None,
    }
}

#[test]
fn effective_head_can_predate_revision_and_expired_governing_head_does_not_resurrect() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let first = create(&db, "可见初版");
    let second = create(&db, "旁边的一条记忆");
    db.update_memory(
        &first.id,
        &UpdateMemoryRequest {
            content: Some("未来安排".to_owned()),
            category: None,
            metadata: None,
        },
    )
    .unwrap();
    let future_edge = db
        .create_memory_link(CreateMemoryLink {
            source_memory_id: first.id.clone(),
            target_memory_id: second.id,
            label: "未来安排依据".into(),
            status: None,
        })
        .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    db.conn()
        .execute(
            "UPDATE memory_governance SET valid_from=?2 WHERE memory_id=?1",
            params![first.id, now + 60_000],
        )
        .unwrap();
    let graph = snapshot(&db);
    assert_eq!(
        graph.current_version_ids[&first.id],
        format!("{}:1", first.id)
    );
    assert_eq!(graph.histories[&first.id][0].revision, 2);
    assert_eq!(
        graph
            .memories
            .iter()
            .find(|memory| memory.id == first.id)
            .unwrap()
            .content,
        "可见初版"
    );
    assert!(graph.relations.iter().all(|edge| edge.id != future_edge.id));
    let mut query = unfiltered();
    query.q = Some("未来安排".into());
    assert!(!db
        .memory_graph(query, false, None)
        .unwrap()
        .memories
        .iter()
        .any(|record| record.id == first.id));
    db.conn()
        .execute(
            "UPDATE memory_governance SET valid_from=NULL,valid_until=?2 WHERE memory_id=?1",
            params![first.id, now - 1],
        )
        .unwrap();
    assert!(!snapshot(&db).current_version_ids.contains_key(&first.id));
}

#[test]
fn scope_is_validated_and_filters_hidden_version_content_links_events_and_counts() {
    use crate::db::memory_governance::CorrectMemoryRequest;
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let c = db.create_conversation("私有对话").unwrap();
    let a = create(&db, "首个公开版本");
    let b = create(&db, "公开邻居");
    let revise = |revision, scope: &str, content: &str| {
        db.correct_memory(
            &a.id,
            &CorrectMemoryRequest {
                content: content.into(),
                expected_revision: revision,
                scope: Some(scope.into()),
                source_ref: None,
                reason: "用户调整范围".into(),
            },
        )
        .unwrap();
    };
    revise(1, &format!("conversation:{}", c.id), "这版只在对话可读");
    let hidden_link = db
        .create_memory_link(CreateMemoryLink {
            source_memory_id: a.id.clone(),
            target_memory_id: b.id.clone(),
            label: "仅在对话时产生的依据".into(),
            status: None,
        })
        .unwrap();
    assert!(!snapshot(&db).current_version_ids.contains_key(&a.id));
    let context = GraphScope {
        workspace_id: None,
        conversation_id: Some(c.id.clone()),
    };
    let scoped = db
        .scoped_memory_graph(unfiltered(), false, None, &context)
        .unwrap();
    assert_eq!(scoped.histories[&a.id].len(), 2);
    assert_eq!(scoped.relations[0].id, hidden_link.id);
    revise(2, "profile", "再次公开的版本");
    let public = snapshot(&db);
    assert_eq!(public.current_version_ids[&a.id], format!("{}:3", a.id));
    assert_eq!(
        public.histories[&a.id]
            .iter()
            .map(|version| version.layer)
            .collect::<Vec<_>>(),
        vec![1, 0]
    );
    assert!(public.histories[&a.id]
        .iter()
        .all(|version| !version.content.contains("这版只在对话可读")));
    assert!(public.relations.is_empty() && public.relation_versions.is_empty());
    assert!(public
        .change_events
        .iter()
        .all(|event| event.version_id != format!("{}:2", a.id)));
    let missing = GraphScope {
        workspace_id: None,
        conversation_id: Some("missing".into()),
    };
    assert!(
        matches!(db.scoped_memory_graph(unfiltered(),false,None,&missing), Err(reason) if reason=="memory_graph_scope_unavailable")
    );
}
