use super::*;

#[test]
fn personal_management_history_pages_equal_timestamps_past_one_hundred_and_scopes_cursors() {
    let f = Fixture::new();
    let a = f.save(20);
    for index in 0..127 {
        f.occurrence(
            &a,
            if index % 10 == 0 {
                "failed"
            } else {
                "completed"
            },
        );
    }
    let mut cursor = None;
    let mut ids = std::collections::HashSet::new();
    loop {
        let page = history(&f.server, &a.id, cursor.as_deref(), None).unwrap();
        assert_eq!(page["total"], 127);
        let values = page["occurrences"].as_array().unwrap();
        assert!(values.len() <= PAGE_SIZE);
        for item in values {
            assert!(ids.insert(item["id"].as_str().unwrap().to_owned()));
        }
        if page["has_more"] == false {
            break;
        }
        cursor = page["next_cursor"].as_str().map(str::to_owned);
    }
    assert_eq!(ids.len(), 127);
    assert_eq!(
        history(&f.server, &a.id, None, Some("failed")).unwrap()["total"],
        13
    );
    assert!(history(&f.server, &a.id, None, Some("nonsense")).is_err());
    let other = f.save(20);
    let foreign = f.occurrence(&other, "completed");
    assert!(history(&f.server, &a.id, Some(&foreign.id), None).is_err());
}

#[test]
fn personal_management_actions_represent_wait_budget_and_archive_without_execution() {
    let f = Fixture::new();
    let mut a = f.save(1);
    assert_eq!(
        detail(&f.server, &a.id).unwrap()["actions"]["can_start"],
        true
    );
    let mut item = f.occurrence(&a, "waiting_input");
    let state = detail(&f.server, &a.id).unwrap();
    assert_eq!(state["actions"]["can_start"], false);
    assert_eq!(state["actions"]["can_edit"], false);
    assert_eq!(state["actions"]["can_stop"], true);
    item.status = "failed".into();
    f.server.db.update_automation_occurrence(&item).unwrap();
    let state = detail(&f.server, &a.id).unwrap();
    assert_eq!(state["actions"]["requires_review"], true);
    assert_eq!(state["actions"]["review_occurrence_id"], item.id);
    assert_eq!(state["actions"]["can_start"], false);
    a.archived = true;
    a.enabled = false;
    a.revision += 1;
    f.server.db.save_automation(&a, false).unwrap();
    let state = detail(&f.server, &a.id).unwrap();
    assert_eq!(state["actions"]["can_edit"], false);
    assert_eq!(state["actions"]["can_stop"], false);
}

#[test]
fn personal_management_preview_is_record_bound_rechecks_grants_and_detects_modification() {
    let f = Fixture::new();
    let a = f.save(5);
    let path = f.root.join("report.md");
    std::fs::write(&path, "# Controlled report\n").unwrap();
    f.record(&a, "created-report", &path, "# Controlled report\n", true);
    f.grant(&f.root, GrantEffect::Allow);
    let document = preview(&f.server, &a.id, "created-report").unwrap();
    assert_eq!(document["content"], "# Controlled report\n");
    assert_eq!(document["truncated"], false);
    let other = f.save(5);
    assert!(preview(&f.server, &other.id, "created-report").is_err());
    let deny = f.grant(&path, GrantEffect::Deny);
    assert!(preview(&f.server, &a.id, "created-report")
        .unwrap_err()
        .contains("权限"));
    f.server.db.delete_grant(&deny).unwrap();
    std::fs::write(&path, "modified by another operation").unwrap();
    assert!(preview(&f.server, &a.id, "created-report")
        .unwrap_err()
        .contains("修改"));
    std::fs::remove_file(&path).unwrap();
    assert!(preview(&f.server, &a.id, "created-report").is_err());
    f.record(
        &a,
        "pending",
        &f.root.join("not-created.md"),
        "pending",
        false,
    );
    assert!(preview(&f.server, &a.id, "pending")
        .unwrap_err()
        .contains("没有成功完成"));
}

#[test]
fn personal_management_projection_is_bounded_and_omits_write_bodies() {
    let f = Fixture::new();
    let a = f.save(5);
    for index in 0..61 {
        f.record(
            &a,
            &format!("call-{index}"),
            &f.root.join(format!("{index}.md")),
            &"sensitive body ".repeat(1000),
            true,
        );
    }
    let state = detail(&f.server, &a.id).unwrap();
    assert_eq!(state["executions"].as_array().unwrap().len(), 50);
    assert!(state["executions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|value| !value["arguments_preview"]
            .as_str()
            .unwrap()
            .contains("sensitive body")));
    assert_eq!(state["files"].as_array().unwrap().len(), 61);
}
