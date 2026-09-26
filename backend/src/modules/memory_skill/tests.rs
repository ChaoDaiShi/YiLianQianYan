//! End-to-end review lifecycle tests.
//!
//! These deliberately drive the service the way a caller does — create,
//! review, confirm, roll back — rather than poking at individual layers, so
//! they keep holding after the module is reorganised.

use super::*;
use crate::{
    db::Database,
    modules::{
        memory_skill::store::ManagedSkillStore,
        task::artifact::ArtifactSource,
        task::{TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind, TaskWorldRuntime},
    },
    shared::event::EventHub,
};

fn setup() -> (
    std::path::PathBuf,
    Database,
    MemorySkillService,
    ArtifactSource,
) {
    let root = std::env::temp_dir().join(format!("skill-evidence-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let db = Database::new(&root.join("db.sqlite")).unwrap();
    let runtime = TaskWorldRuntime::new(&db, EventHub::new(4)).unwrap();
    let graph = TaskGraphId::new("lesson-task").unwrap();
    let node = TaskNodeId::new("node").unwrap();
    runtime
        .create_graph(
            graph.clone(),
            vec![TaskNode::new(
                node.clone(),
                TaskNodeKind::Work,
                "Completed evidence",
                serde_json::json!({}),
            )
            .unwrap()],
            vec![],
            1,
        )
        .unwrap();
    let execution = runtime.start_execution(&graph, &node, 1, 2).unwrap();
    runtime
        .complete_execution(
            &graph,
            &execution.id,
            serde_json::json!({"ok":true,"result":"Verified evidence"}),
            crate::modules::task::validation::ValidationPolicy::StructuredResult,
            3,
        )
        .unwrap();
    let source = ArtifactSource::Node {
        graph_id: graph.to_string(),
        node_id: node.to_string(),
        execution_id: execution.id.to_string(),
        workspace_id: String::new(),
    };
    let service = MemorySkillService::new(db.clone(), ManagedSkillStore::new(root.join("skills")));
    (root, db, service, source)
}

#[test]
fn authorized_candidate_is_reviewed_versioned_discoverable_and_reversible() {
    let (root, db, service, source) = setup();
    assert!(service
        .create(vec![source.clone()], "完成后记录验证步骤", false, 4)
        .is_err());
    let draft = service
        .create(vec![source.clone()], "完成后记录验证步骤", true, 4)
        .unwrap();
    assert_eq!(draft.status, "draft");
    assert!(!root.join("skills/reviewed/SKILL.md").exists());
    assert!(service
        .confirm(&draft.id, draft.revision, "reviewed", true, 5)
        .is_err());
    let edited = service
        .edit(
            &draft.id,
            draft.revision,
            "完成后先记录验证步骤，再整理结果",
            5,
        )
        .unwrap();
    assert!(service.validate(&draft.id, draft.revision, 6).is_err());
    let valid = service.validate(&edited.id, edited.revision, 6).unwrap();
    assert!(service
        .confirm(&valid.id, valid.revision, "reviewed", false, 7)
        .is_err());
    let first = service
        .confirm(&valid.id, valid.revision, "reviewed", true, 7)
        .unwrap();
    assert_eq!(first.version, 1);
    let discovery = crate::tools::skill::SkillDiscovery::discover(
        &[root.join("skills").to_string_lossy().into()],
        root.to_str().unwrap(),
    );
    assert!(discovery.all().iter().any(|skill| skill.name == "reviewed"));
    let second = service
        .create(vec![source], "完成后先整理结果，再记录后续问题", true, 8)
        .unwrap();
    let second = service.validate(&second.id, second.revision, 9).unwrap();
    assert_eq!(
        service
            .confirm(&second.id, second.revision, "reviewed", true, 10)
            .unwrap()
            .version,
        2
    );
    service.deactivate("reviewed").unwrap();
    assert!(!root.join("skills/reviewed/SKILL.md").exists());
    assert_eq!(service.rollback("reviewed", 1, 11).unwrap().version, 1);
    assert!(
        std::fs::read_to_string(root.join("skills/reviewed/SKILL.md"))
            .unwrap()
            .contains("先记录验证步骤")
    );
    assert_eq!(service.versions("reviewed").unwrap().len(), 2);
    let memories: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
        .unwrap();
    assert_eq!(memories, 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn confirmation_database_failure_leaves_no_installed_rule() {
    let (root, db, service, source) = setup();
    let draft = service
        .create(vec![source], "记录任务的验证步骤", true, 4)
        .unwrap();
    let valid = service.validate(&draft.id, draft.revision, 5).unwrap();
    db.conn().execute_batch("CREATE TRIGGER fail_skill_version BEFORE INSERT ON managed_skill_versions BEGIN SELECT RAISE(ABORT,'injected version failure'); END;").unwrap();
    assert!(service
        .confirm(&valid.id, valid.revision, "rollback-safe", true, 6)
        .is_err());
    assert!(!root.join("skills/rollback-safe/SKILL.md").exists());
    assert_eq!(service.list().unwrap()[0].status, "validated");
    assert!(service.versions("rollback-safe").unwrap().is_empty());
    let rejected = service.reject(&valid.id, valid.revision, 7).unwrap();
    assert_eq!(rejected.status, "rejected");
    assert!(service
        .confirm(&valid.id, rejected.revision, "rejected", true, 8)
        .is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn sensitivity_is_checked_before_draft_and_again_before_confirmation() {
    let (root, db, service, source) = setup();
    assert!(service
        .create(vec![source.clone()], "password = PLACEHOLDER", true, 4)
        .is_err());
    assert!(service.list().unwrap().is_empty());
    let draft = service
        .create(vec![source], "记录已验证的步骤和限制", true, 5)
        .unwrap();
    let valid = service.validate(&draft.id, draft.revision, 6).unwrap();
    db.conn()
        .execute(
            "UPDATE skill_candidates SET lesson='password = PLACEHOLDER' WHERE id=?1",
            [&valid.id],
        )
        .unwrap();
    assert!(service
        .confirm(&valid.id, valid.revision, "unsafe", true, 7)
        .is_err());
    assert!(!root.join("skills/unsafe/SKILL.md").exists());
    let _ = std::fs::remove_dir_all(root);
}
