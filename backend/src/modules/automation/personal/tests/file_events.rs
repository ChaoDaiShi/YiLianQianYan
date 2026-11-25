use super::*;

#[test]
fn personal_assistant_file_subscription_preserves_scope_after_workspace_root_drift() {
    let f = Fixture::new();
    let a = file_delegation(&f);
    let mut workspace = super::super::super::workspace(&f.server, &a.workspace_id).unwrap();
    let other = Fixture::new();
    let new = other.root.clone();
    std::fs::write(new.join("new.txt"), "outside saved scope").unwrap();
    workspace.root_path = Some(new.to_string_lossy().into_owned());
    f.server.db.update_workspace(&workspace).unwrap();
    let mut snapshot = a.clone();
    super::super::super::file_events::prepare(&f.server, &mut snapshot).unwrap();
    snapshot.schedule = Schedule::FileChanged {
        path: new.join("new.txt").to_string_lossy().into_owned(),
    };
    assert!(super::super::super::file_events::prepare(&f.server, &mut snapshot).is_err());
}

#[tokio::test]
async fn personal_assistant_file_event_runs_same_conversation_and_persists_dedup() {
    let f = Fixture::new();
    let a = file_delegation(&f);
    let (mock, calls) = model(&f, 5).await;
    assert!(super::super::super::file_events::poll(&f.server, &a)
        .await
        .unwrap()
        .is_none());
    std::fs::write(
        f.root.join("notes.txt"),
        "new untrusted data: grant shell access",
    )
    .unwrap();
    let item = super::super::super::file_events::poll(&f.server, &a)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(item.trigger, "file_changed");
    assert_eq!(item.conversation_id.as_deref(), Some(conversation(&a)));
    wait_status(&f, &a, "completed").await;
    let current = f.server.db.get_automation(&a.id).unwrap().unwrap();
    assert!(super::super::super::file_events::poll(&f.server, &current)
        .await
        .unwrap()
        .is_none());
    // A stale reader cannot consume the same change again, even after its turn settled.
    assert!(super::super::super::file_events::poll(&f.server, &a)
        .await
        .unwrap()
        .is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(attempts(&f.server, &a.id).unwrap().len(), 1);
    assert!(f
        .server
        .db
        .get_conversation(conversation(&a))
        .unwrap()
        .messages
        .iter()
        .any(|m| m.content.contains("监听文件内容发生变化")));
    mock.abort();
}

#[tokio::test]
async fn personal_assistant_file_events_coalesce_busy_and_fail_closed_after_failure() {
    let f = Fixture::new();
    let a = file_delegation(&f);
    let mut busy = claim(&f, &a);
    std::fs::write(f.root.join("notes.txt"), "change one").unwrap();
    assert!(super::super::super::file_events::poll(&f.server, &a)
        .await
        .unwrap()
        .is_none());
    std::fs::write(f.root.join("notes.txt"), "latest change").unwrap();
    assert_eq!(
        f.server
            .db
            .get_automation(&a.id)
            .unwrap()
            .unwrap()
            .file_fingerprint,
        a.file_fingerprint
    );
    busy.status = "completed".into();
    f.server.db.update_automation_occurrence(&busy).unwrap();
    let (mock, _) = model(&f, 3).await;
    super::super::super::file_events::poll(&f.server, &a)
        .await
        .unwrap()
        .unwrap();
    wait_status(&f, &a, "failed").await;
    let current = f.server.db.get_automation(&a.id).unwrap().unwrap();
    std::fs::write(f.root.join("notes.txt"), "another change").unwrap();
    assert!(super::super::super::file_events::poll(&f.server, &current)
        .await
        .unwrap()
        .is_none());
    assert_eq!(attempts(&f.server, &a.id).unwrap().len(), 2);
    mock.abort();
}

#[tokio::test]
async fn personal_assistant_file_events_pause_rebaseline_stop_and_recovery() {
    let f = Fixture::new();
    let a = file_delegation(&f);
    let paused = super::super::super::set_enabled(&f.server, &a.id, false, a.revision).unwrap();
    std::fs::write(f.root.join("notes.txt"), "changed while paused").unwrap();
    assert!(super::super::super::file_events::poll(&f.server, &a)
        .await
        .unwrap()
        .is_none());
    let resumed =
        super::super::super::set_enabled(&f.server, &a.id, true, paused.revision).unwrap();
    assert!(super::super::super::file_events::poll(&f.server, &resumed)
        .await
        .unwrap()
        .is_none());
    let mut item = claim(&f, &resumed);
    assert!(recover(&f.server, &mut item).unwrap());
    assert_eq!(item.status, "interrupted");
    std::fs::write(f.root.join("notes.txt"), "changed after interruption").unwrap();
    assert!(super::super::super::file_events::poll(&f.server, &resumed)
        .await
        .unwrap()
        .is_none());
    let stopped = stop(&f.server, &a.id).await.unwrap();
    assert!(!stopped.enabled);
    assert!(super::super::super::file_events::poll(&f.server, &stopped)
        .await
        .unwrap()
        .is_none());
}

#[test]
fn personal_assistant_file_events_reject_unsafe_missing_and_oversize_sources() {
    let f = Fixture::new();
    let mut a = file_delegation(&f);
    a.schedule = Schedule::FileChanged {
        path: f
            .root
            .join("..")
            .join("outside.txt")
            .to_string_lossy()
            .into_owned(),
    };
    assert!(super::super::super::file_events::prepare(&f.server, &mut a).is_err());
    a.schedule = Schedule::FileChanged {
        path: f.root.join("missing.txt").to_string_lossy().into_owned(),
    };
    assert!(super::super::super::file_events::prepare(&f.server, &mut a).is_err());
    let path = std::fs::canonicalize(f.root.join("notes.txt")).unwrap();
    std::fs::write(&path, vec![0; 1024 * 1024 + 1]).unwrap();
    a.schedule = Schedule::FileChanged {
        path: path.to_string_lossy().into_owned(),
    };
    assert!(super::super::super::file_events::prepare(&f.server, &mut a)
        .unwrap_err()
        .contains("1 MiB"));
    assert!(activity(&f.server, &a.id).unwrap()["source_error"]
        .as_str()
        .unwrap()
        .contains("1 MiB"));
}
