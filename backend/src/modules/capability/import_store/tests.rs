use super::*;
fn db() -> Database {
    Database::new(std::path::Path::new(":memory:")).unwrap()
}
fn package(content: &str) -> Package {
    markdown_package("demo", "Demo", "1.0.0", content, "markdown").unwrap()
}
#[test]
fn import_preview_confirm_update_disable_uninstall_and_restore_are_transactional() {
    let db = db();
    let first = inspect(
        &db,
        package("# Inert instructions\nDo not run these during inspection."),
        1,
    )
    .unwrap();
    assert!(list(&db).unwrap().is_empty());
    let installed = confirm(&db, &first.preview_id, 2).unwrap();
    assert!(!installed.enabled);
    assert!(confirm(&db, &first.preview_id, 3).is_err());
    let enabled = change(&db, "demo", installed.revision, "enable").unwrap();
    let preview = inspect(&db, package("# Changed"), 4).unwrap();
    assert_eq!(preview.changed, vec!["SKILL.md"]);
    let updated = confirm(&db, &preview.preview_id, 5).unwrap();
    assert!(!updated.enabled);
    assert_eq!(updated.history.len(), 1);
    assert!(change(&db, "demo", enabled.revision, "uninstall").is_err());
    let removed = change(&db, "demo", updated.revision, "uninstall").unwrap();
    assert!(!removed.installed);
    assert!(!removed.enabled);
    let restored = change(&db, "demo", removed.revision, "restore").unwrap();
    assert!(restored.installed);
    let rolled_back = change(&db, "demo", restored.revision, "rollback").unwrap();
    assert!(rolled_back.package.files["SKILL.md"].contains("Inert"));
}
#[test]
fn stale_preview_dedup_and_persistence_failure_leave_inventory_unchanged() {
    let db = db();
    let first = inspect(&db, package("# first"), 1).unwrap();
    let competing = inspect(&db, package("# second"), 1).unwrap();
    let installed = confirm(&db, &first.preview_id, 2).unwrap();
    assert!(confirm(&db, &competing.preview_id, 3).is_err());
    let same = inspect(&db, package("# first"), 4).unwrap();
    assert!(same.unchanged);
    assert_eq!(
        confirm(&db, &same.preview_id, 5).unwrap().revision,
        installed.revision
    );
    let before = serde_json::to_value(list(&db).unwrap()).unwrap();
    db.conn().execute_batch("CREATE TRIGGER fail_import_write BEFORE UPDATE ON settings BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    assert!(change(&db, "demo", installed.revision, "enable").is_err());
    assert_eq!(serde_json::to_value(list(&db).unwrap()).unwrap(), before);
}
