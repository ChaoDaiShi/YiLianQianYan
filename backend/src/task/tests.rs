// ============================================================
// Task domain tests — filled incrementally alongside each domain.
// ============================================================

use super::*;
use crate::db::Database;
use crate::workspace::{Workspace, WorkspaceId};

fn temp_db(label: &str) -> (std::path::PathBuf, Database) {
    let path =
        std::env::temp_dir().join(format!("yilian-task-{label}-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    (path, db)
}

fn workspace(db: &Database, name: &str) -> Workspace {
    let ws = Workspace::new(
        WorkspaceId::generate(),
        name.to_string(),
        String::new(),
        None,
        1,
    )
    .unwrap();
    db.create_workspace(&ws).unwrap();
    ws
}

#[test]
fn task_id_generates_and_serializes() {
    let a = TaskId::generate();
    let b = TaskId::generate();
    assert_ne!(a, b);
    assert!(TaskId::new("").is_err());
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::json!(a.as_str())
    );
}

#[test]
fn task_creates_as_draft_with_bounds() {
    let (path, db) = temp_db("create");
    let ws = workspace(&db, "W");
    let task = Task::new(
        TaskId::generate(),
        ws.id,
        "任务".to_string(),
        "描述".to_string(),
        TaskPriority::High,
        None,
        None,
        10,
    )
    .unwrap();
    assert_eq!(task.status, TaskStatus::Draft);
    assert_eq!(task.priority, TaskPriority::High);
    let _ = std::fs::remove_file(&path);

    // Title empty rejected.
    let (path2, db2) = temp_db("empty");
    let ws2 = workspace(&db2, "W2");
    let error = Task::new(
        TaskId::generate(),
        ws2.id,
        " ".to_string(),
        String::new(),
        TaskPriority::Normal,
        None,
        None,
        10,
    );
    assert_eq!(error, Err(TaskError::EmptyTitle));
    let _ = std::fs::remove_file(&path2);
}

#[test]
fn task_status_transitions_are_validated() {
    assert!(task_transition_allowed(
        TaskStatus::Draft,
        TaskStatus::Ready
    ));
    assert!(task_transition_allowed(
        TaskStatus::Ready,
        TaskStatus::Running
    ));
    assert!(task_transition_allowed(
        TaskStatus::Running,
        TaskStatus::Completed
    ));
    assert!(task_transition_allowed(
        TaskStatus::Running,
        TaskStatus::WaitingApproval
    ));
    assert!(task_transition_allowed(
        TaskStatus::Running,
        TaskStatus::Failed
    ));
    assert!(!task_transition_allowed(
        TaskStatus::Completed,
        TaskStatus::Running
    ));
    assert!(!task_transition_allowed(
        TaskStatus::Failed,
        TaskStatus::Completed
    ));
    assert!(task_transition_allowed(
        TaskStatus::Blocked,
        TaskStatus::Ready
    ));
    assert!(task_transition_allowed(
        TaskStatus::WaitingUser,
        TaskStatus::Running
    ));
}

#[test]
fn execution_status_transitions_are_validated() {
    assert!(execution_transition_allowed(
        TaskExecutionStatus::Created,
        TaskExecutionStatus::Running
    ));
    assert!(execution_transition_allowed(
        TaskExecutionStatus::Running,
        TaskExecutionStatus::Completed
    ));
    assert!(execution_transition_allowed(
        TaskExecutionStatus::Running,
        TaskExecutionStatus::WaitingApproval
    ));
    assert!(execution_transition_allowed(
        TaskExecutionStatus::Running,
        TaskExecutionStatus::Interrupted
    ));
    assert!(execution_transition_allowed(
        TaskExecutionStatus::WaitingApproval,
        TaskExecutionStatus::Running
    ));
    assert!(!execution_transition_allowed(
        TaskExecutionStatus::Completed,
        TaskExecutionStatus::Running
    ));
    assert!(!execution_transition_allowed(
        TaskExecutionStatus::Interrupted,
        TaskExecutionStatus::Running
    ));
}

#[test]
fn task_persists_and_filters() {
    let (path, db) = temp_db("persist");
    let ws = workspace(&db, "W");
    let task = Task::new(
        TaskId::generate(),
        ws.id.clone(),
        "任务".to_string(),
        String::new(),
        TaskPriority::Normal,
        None,
        None,
        10,
    )
    .unwrap();
    db.create_task(&task).unwrap();
    let stored = db.get_task(&task.id).unwrap().unwrap();
    assert_eq!(stored.title, "任务");

    let filtered = db
        .list_tasks(&crate::db::TaskQuery {
            workspace_id: Some(ws.id.clone()),
            status: Some(TaskStatus::Draft),
            limit: Some(10),
            offset: None,
        })
        .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, task.id);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn task_execution_persists_and_attempt_counts() {
    let (path, db) = temp_db("exec");
    let ws = workspace(&db, "W");
    let task = Task::new(
        TaskId::generate(),
        ws.id,
        "T".to_string(),
        String::new(),
        TaskPriority::Normal,
        None,
        None,
        1,
    )
    .unwrap();
    db.create_task(&task).unwrap();

    let execution = TaskExecution::new(
        TaskExecutionId::generate(),
        task.id.clone(),
        crate::execution::ExecutionContext::new(
            crate::execution::ExecutionId::generate(),
            "local-user",
            "task-runner",
            None,
            1,
        ),
        1,
        1,
    );
    db.create_task_execution(&execution).unwrap();
    let list = db.list_task_executions(&task.id).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].attempt, 1);
    assert_eq!(list[0].execution_context.subject_id, "local-user");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn task_plan_validation_checks_bounds_and_uniqueness() {
    let ok = TaskPlan {
        schema_version: 1,
        summary: "s".to_string(),
        steps: vec![TaskPlanStep {
            id: "step-1".to_string(),
            title: "t".to_string(),
            instruction: "i".to_string(),
            executor: TaskPlanExecutor::Subagent {
                name: "r".to_string(),
            },
        }],
    };
    assert!(validate_plan_structure(&ok).is_ok());

    let too_many_steps = TaskPlan {
        schema_version: 1,
        summary: "s".to_string(),
        steps: (0..21)
            .map(|i| TaskPlanStep {
                id: format!("s{i}"),
                title: "t".to_string(),
                instruction: "i".to_string(),
                executor: TaskPlanExecutor::Subagent {
                    name: "r".to_string(),
                },
            })
            .collect(),
    };
    assert!(validate_plan_structure(&too_many_steps).is_err());

    let duplicate = TaskPlan {
        schema_version: 1,
        summary: "s".to_string(),
        steps: vec![
            TaskPlanStep {
                id: "a".to_string(),
                title: "t".to_string(),
                instruction: "i".to_string(),
                executor: TaskPlanExecutor::Subagent {
                    name: "r".to_string(),
                },
            },
            TaskPlanStep {
                id: "a".to_string(),
                title: "t".to_string(),
                instruction: "i".to_string(),
                executor: TaskPlanExecutor::Subagent {
                    name: "r".to_string(),
                },
            },
        ],
    };
    assert!(validate_plan_structure(&duplicate).is_err());

    let bad_id = TaskPlan {
        schema_version: 1,
        summary: "s".to_string(),
        steps: vec![TaskPlanStep {
            id: "has space".to_string(),
            title: "t".to_string(),
            instruction: "i".to_string(),
            executor: TaskPlanExecutor::Subagent {
                name: "r".to_string(),
            },
        }],
    };
    assert!(validate_plan_structure(&bad_id).is_err());
}

#[test]
fn artifact_service_validates_path_containment() {
    // A root and a path inside it pass; a traversal escape is rejected.
    let (path, _db) = temp_db("artifact");
    let root = std::env::temp_dir().join("yilian-artifact-root");
    std::fs::create_dir_all(&root.join("sub")).unwrap();
    let inside = root.join("sub").join("notes.txt");
    std::fs::write(&inside, "x").unwrap();
    assert!(
        ArtifactService::validate_path(inside.to_str().unwrap(), Some(root.to_str().unwrap()))
            .is_ok()
    );

    let outside = std::env::temp_dir().join("yilian-outside-other.txt");
    std::fs::write(&outside, "x").unwrap();
    assert_eq!(
        ArtifactService::validate_path(outside.to_str().unwrap(), Some(root.to_str().unwrap())),
        Err(ArtifactError::PathOutsideRoot)
    );
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&outside);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn delegation_policy_clamps_to_hard_limits() {
    let policy = DelegationPolicy {
        max_depth: 99,
        max_agent_executions: 99,
        max_total_iterations: 9999,
    }
    .clamped();
    assert_eq!(policy.max_depth, MAX_DELEGATION_DEPTH);
    assert_eq!(policy.max_agent_executions, MAX_AGENT_EXECUTIONS_PER_TASK);
    assert_eq!(policy.max_total_iterations, MAX_TOTAL_AGENT_ITERATIONS);
}

#[test]
fn task_decision_models_options_and_status() {
    let decision = TaskDecision {
        id: TaskDecisionId::generate(),
        task_id: TaskId::generate(),
        task_execution_id: TaskExecutionId::generate(),
        prompt: "下一步？".to_string(),
        options: vec![
            TaskDecisionOption {
                id: "a".to_string(),
                label: "方案 A".to_string(),
                description: String::new(),
            },
            TaskDecisionOption {
                id: "b".to_string(),
                label: "方案 B".to_string(),
                description: String::new(),
            },
        ],
        status: TaskDecisionStatus::Pending,
        created_at: 1,
        resolved_at: None,
    };
    assert_eq!(decision.options.len(), 2);
    assert_eq!(decision.status.to_string(), "pending");
    assert_eq!(
        serde_json::to_value(TaskDecisionStatus::Resolved).unwrap(),
        serde_json::json!("resolved")
    );
}
