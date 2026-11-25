// ============================================================
// Workspace domain tests.
// ============================================================

use super::*;
use crate::db::Database;

#[test]
fn workspace_id_generates_and_validates() {
    let a = WorkspaceId::generate();
    let b = WorkspaceId::generate();
    assert_ne!(a, b);
    assert!(WorkspaceId::new("").is_err());
    assert!(WorkspaceId::new("abc\n").is_err());
    assert!(WorkspaceId::new("abc-123").is_ok());
    // serde transparent
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::json!(a.as_str())
    );
}

#[test]
fn workspace_requires_non_empty_name() {
    let error = Workspace::new(
        WorkspaceId::generate(),
        "  ".to_string(),
        String::new(),
        None,
        1,
    );
    assert_eq!(error, Err(WorkspaceFieldError::EmptyName));
}

#[test]
fn workspace_rejects_overlong_name() {
    let error = Workspace::new(
        WorkspaceId::generate(),
        "x".repeat(121),
        String::new(),
        None,
        1,
    );
    assert_eq!(error, Err(WorkspaceFieldError::NameTooLong));
}

#[test]
fn workspace_rejects_overlong_description() {
    let error = Workspace::new(
        WorkspaceId::generate(),
        "ok".to_string(),
        "y".repeat(4001),
        None,
        1,
    );
    assert_eq!(error, Err(WorkspaceFieldError::DescriptionTooLong));
}

#[test]
fn workspace_creates_active_and_trims_name() {
    let workspace = Workspace::new(
        WorkspaceId::generate(),
        "  测试项目  ".to_string(),
        "描述".to_string(),
        None,
        10,
    )
    .unwrap();
    assert_eq!(workspace.name, "测试项目");
    assert_eq!(workspace.status, WorkspaceStatus::Active);
    assert_eq!(workspace.created_at, 10);
}

#[test]
fn workspace_status_serde() {
    assert_eq!(
        serde_json::to_value(WorkspaceStatus::Active).unwrap(),
        serde_json::json!("active")
    );
    assert_eq!(
        serde_json::to_value(WorkspaceStatus::Archived).unwrap(),
        serde_json::json!("archived")
    );
}

fn temp_db(label: &str) -> (std::path::PathBuf, Database) {
    let path = std::env::temp_dir().join(format!("yilian-ws-{label}-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    (path, db)
}

#[test]
fn default_workspace_starts_once_and_returns_to_the_initial_folder() {
    let (path, db) = temp_db("default");
    let root = path.parent().unwrap().to_str().unwrap();
    let initial = db.ensure_default_workspace(root).unwrap();
    assert_eq!(db.ensure_default_workspace(root).unwrap(), initial);
    let workspaces = db.list_workspaces().unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].root_path.as_deref(), Some(root));

    let project = Workspace::new(
        WorkspaceId::generate(),
        "项目".into(),
        String::new(),
        None,
        1,
    )
    .unwrap();
    db.create_workspace(&project).unwrap();
    assert!(db.set_default_workspace(&project.id).unwrap());
    assert_eq!(
        db.ensure_default_workspace(root).unwrap(),
        project.id.as_str()
    );
    let chat = db
        .create_conversation_with_workspace_default("新对话", true)
        .unwrap();
    db.archive_workspace(&project.id, 2, Some(root)).unwrap();
    assert_eq!(
        db.default_workspace_id().unwrap().as_deref(),
        Some(initial.as_str())
    );
    assert_eq!(
        db.conversation_workspace(&chat.id).unwrap().as_deref(),
        Some(project.id.as_str())
    );

    db.archive_workspace(&WorkspaceId::new(&initial).unwrap(), 3, Some(root))
        .unwrap();
    let replacement = db.default_workspace_id().unwrap().unwrap();
    assert_ne!(replacement, initial);
    drop(db);
    let reopened = Database::new(&path).unwrap();
    assert_eq!(
        reopened.ensure_default_workspace(root).unwrap(),
        replacement
    );
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn workspace_persists_roundtrips_archives_and_lists() {
    let (path, db) = temp_db("crud");
    let ws = Workspace::new(
        WorkspaceId::generate(),
        "P1".to_string(),
        "项目一".to_string(),
        None,
        1,
    )
    .unwrap();
    db.create_workspace(&ws).unwrap();
    assert_eq!(db.get_workspace(&ws.id).unwrap().unwrap().name, "P1");

    let mut project_record = ws.clone();
    project_record.project = WorkspaceProject {
        stage: Some(super::model::WorkspaceProjectStage::Testing),
        frontend: "页面联调完成".into(),
        backend: "等待接口验收".into(),
    };
    db.update_workspace_with_project(&project_record, true)
        .unwrap();

    let mut updated = ws.clone();
    updated.name = "P1-改".to_string();
    updated.updated_at = 2;
    db.update_workspace(&updated).unwrap();
    assert_eq!(db.get_workspace(&ws.id).unwrap().unwrap().name, "P1-改");

    db.archive_workspace(&ws.id, 3, None).unwrap();
    let archived = db.get_workspace(&ws.id).unwrap().unwrap();
    assert_eq!(archived.status, WorkspaceStatus::Archived);
    assert_eq!(archived.project, project_record.project);

    let ws2 = Workspace::new(
        WorkspaceId::generate(),
        "P2".to_string(),
        String::new(),
        None,
        4,
    )
    .unwrap();
    db.create_workspace(&ws2).unwrap();
    let list = db.list_workspaces().unwrap();
    assert_eq!(list.len(), 2);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn workspace_persists_root_path() {
    let (path, db) = temp_db("root");
    let ws = Workspace::new(
        WorkspaceId::generate(),
        "P".to_string(),
        String::new(),
        Some("f:/workspaces/p1".to_string()),
        1,
    )
    .unwrap();
    db.create_workspace(&ws).unwrap();
    let stored = db.get_workspace(&ws.id).unwrap().unwrap();
    assert_eq!(stored.root_path.as_deref(), Some("f:/workspaces/p1"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn archived_workspace_can_be_restored_or_removed_without_losing_history() {
    let (path, db) = temp_db("organize");
    let ws = Workspace::new(
        WorkspaceId::generate(),
        "项目".into(),
        String::new(),
        Some("f:/workspaces/project".into()),
        1,
    )
    .unwrap();
    db.create_workspace(&ws).unwrap();
    assert!(!db.remove_archived_workspace(&ws.id, 2).unwrap());

    db.archive_workspace(&ws.id, 3, None).unwrap();
    assert!(db.restore_workspace(&ws.id, 4).unwrap());
    assert_eq!(
        db.get_workspace(&ws.id).unwrap().unwrap().status,
        WorkspaceStatus::Active
    );

    db.archive_workspace(&ws.id, 5, None).unwrap();
    assert!(db.remove_archived_workspace(&ws.id, 6).unwrap());
    db.archive_workspace(&ws.id, 7, None).unwrap();
    db.update_workspace(&ws).unwrap();
    assert!(!db.restore_workspace(&ws.id, 8).unwrap());
    assert!(db.get_workspace(&ws.id).unwrap().is_none());
    assert!(db.list_workspaces().unwrap().is_empty());
    assert_eq!(
        db.get_workspace_for_history(&ws.id)
            .unwrap()
            .unwrap()
            .root_path,
        ws.root_path
    );
    drop(db);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn workspace_active_task_count() {
    let (path, db) = temp_db("count");
    let ws = Workspace::new(
        WorkspaceId::generate(),
        "P".to_string(),
        String::new(),
        None,
        1,
    )
    .unwrap();
    db.create_workspace(&ws).unwrap();
    assert_eq!(db.count_active_tasks(&ws.id).unwrap(), 0);
    let _ = std::fs::remove_file(&path);
}
