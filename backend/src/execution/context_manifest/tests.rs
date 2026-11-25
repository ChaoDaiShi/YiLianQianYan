use super::*;
#[test]
fn context_manifest_bounds_metadata_without_input_bodies() {
    let mut m = ContextManifest::default();
    m.record(
        "memory",
        "m1",
        Some("2".into()),
        ContextStatus::Truncated,
        10,
        Some("bounded input"),
    );
    assert!(m.is_bounded());
    assert_eq!(m.total_chars, 10);
    let text = serde_json::to_string(&m).unwrap();
    assert!(!text.contains("content"));
    assert_eq!(serde_json::from_str::<ContextManifest>(&text).unwrap(), m);
}
#[test]
fn workflow_context_consumes_constraints_and_marks_reference_limits() {
    let c = crate::modules::task::NodeContext::new(
        "Goal",
        "Do work",
        vec![],
        vec![],
        vec!["memory:1".into()],
        vec!["mcp.a".into()],
        vec!["local files only".into()],
        vec!["read back output".into()],
    )
    .unwrap();
    let rendered = render_node_context(&c, 2000);
    assert!(rendered.contains("local files only"));
    assert!(rendered.contains("read back output"));
    assert!(rendered.contains("not resolved"));
    assert!(rendered.contains("do not grant"));
    assert!(render_node_context(&c, 80).chars().count() <= 80);
    let (text, consumed) = render_node_context_with_manifest(&c, 80);
    assert_eq!(text.chars().count(), consumed.total_chars);
    assert!(consumed.entries.iter().any(|entry| matches!(
        entry.status,
        ContextStatus::Truncated | ContextStatus::Excluded
    )));
    assert!(node_manifest(&c)
        .entries
        .iter()
        .any(|e| e.kind == "memory_reference" && e.status == ContextStatus::Unsupported));
}

#[test]
fn context_manifest_persistence_is_idempotent_and_old_turn_cannot_replace_new() {
    let path = std::env::temp_dir().join(format!("context-manifest-{}.db", uuid::Uuid::new_v4()));
    let db = crate::db::Database::new(&path).unwrap();
    let c = db.create_conversation("manifest fixture").unwrap();
    let accept = |id: &str, now: i64| {
        db.accept_conversation_turn(&crate::db::MessageRow {
            id: id.into(),
            conversation_id: c.id.clone(),
            role: "user".into(),
            content: "local fixture".into(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at: now,
        })
        .unwrap()
    };
    let first = accept("manifest-user-1", 1);
    let mut manifest = ContextManifest::default();
    manifest.record(
        "memory",
        "weekly-report",
        None,
        ContextStatus::Resolved,
        12,
        None,
    );
    let timeline = ConversationContextTimeline::new(0, 1, &manifest);
    save_conversation_manifest(&db, &c.id, &first, &manifest, Some(timeline)).unwrap();
    save_current_conversation_manifest(&db, &c.id, &ContextManifest::default()).unwrap();
    let saved = load_conversation_manifest(&db, &c.id).unwrap().unwrap();
    assert!(saved.manifest.entries.is_empty());
    let timeline = saved.timeline.unwrap();
    assert_eq!(timeline.request_received_at, 0);
    assert_eq!(timeline.memory_resolved_at, 1);
    assert_eq!(timeline.memory_sources[0].source_id, "weekly-report");
    assert_eq!(
        load_conversation_manifest(&db, &c.id)
            .unwrap()
            .unwrap()
            .turn_id,
        first
    );
    db.finish_conversation_turn(&c.id, "completed").unwrap();
    let second = accept("manifest-user-2", 2);
    assert!(load_conversation_manifest(&db, &c.id).unwrap().is_none());
    assert!(save_conversation_manifest(&db, &c.id, &first, &manifest, None).is_err());
    save_conversation_manifest(&db, &c.id, &second, &manifest, None).unwrap();
    let saved = load_conversation_manifest(&db, &c.id).unwrap().unwrap();
    assert_eq!(saved.turn_id, second);
    assert!(saved.timeline.is_none());
    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn context_manifest_owner_storage_does_not_merge_equal_ids() {
    let path = std::env::temp_dir().join(format!("owner-manifest-{}.db", uuid::Uuid::new_v4()));
    let db = crate::db::Database::new(&path).unwrap();
    let workflow = crate::execution::ExecutionOwner::Workflow {
        workflow_graph_id: "same".into(),
        run_id: "same".into(),
    };
    let task = crate::execution::ExecutionOwner::LegacyTask {
        task_id: "same".into(),
        execution_id: "same".into(),
    };
    let mut manifest = ContextManifest::default();
    manifest.record(
        "constraint",
        "node:constraint:0",
        None,
        ContextStatus::Resolved,
        5,
        None,
    );
    record_manifest(&db, &workflow, &manifest).unwrap();
    record_manifest(&db, &workflow, &manifest).unwrap();
    assert_eq!(load_manifest(&db, &workflow).unwrap(), Some(manifest));
    assert!(load_manifest(&db, &task).unwrap().is_none());
    drop(db);
    let _ = std::fs::remove_file(path);
}
