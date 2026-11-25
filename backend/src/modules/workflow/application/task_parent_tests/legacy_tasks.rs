use super::*;

#[tokio::test]
async fn integration_workflow_legacy_task_approval_settles_original_attempt_once() {
    use crate::modules::task::model::{TaskExecutionStatus, TaskStatus};
    let f = Fixture::new();
    let (task, execution, approval) = f.legacy_waiting().await;
    resolve_approval(&f.server, &approval.approval_id, &approval.subject_id, true)
        .await
        .unwrap();
    let actual = f
        .server
        .db
        .get_task_execution(&execution.id)
        .unwrap()
        .unwrap();
    assert_eq!(actual.workflow_run_id, execution.workflow_run_id);
    assert_eq!(actual.status, TaskExecutionStatus::Completed);
    assert_eq!(
        f.server.db.get_task(&task.id).unwrap().unwrap().status,
        TaskStatus::Completed
    );
    assert_eq!(
        std::fs::read_to_string(f.root.join("result.txt")).unwrap(),
        "one controlled write"
    );
    reconcile(&f.server).unwrap();
    assert!(
        resolve_approval(&f.server, &approval.approval_id, &approval.subject_id, true)
            .await
            .is_err()
    );
    assert_eq!(f.server.db.list_task_executions(&task.id).unwrap().len(), 1);
}

struct IntegrationTaskBuilder(Arc<AppServer>);
#[async_trait::async_trait]
impl crate::modules::task::service::OrchestratorBuilder for IntegrationTaskBuilder {
    async fn build(&self) -> crate::modules::task::orchestrator::TaskOrchestrator {
        crate::modules::task::orchestrator::build_task_orchestrator(&self.0).await
    }
}

#[tokio::test]
async fn integration_workflow_legacy_task_stop_cancels_claimed_token_and_child() {
    use crate::modules::task::model::{TaskExecutionStatus, TaskStatus};
    let f = Fixture::new();
    let (task, execution, approval) = f.legacy_waiting().await;
    let lease = claim_approval(&f.server, &approval).unwrap();
    assert!(f
        .server
        .active_task_executions
        .lock()
        .contains_key(execution.id.as_str()));
    let runner = crate::modules::task::service::TaskRunner::new(
        f.server.db.clone_connection(),
        Arc::new(IntegrationTaskBuilder(Arc::clone(&f.server))),
        Arc::clone(&f.server.active_task_executions),
    );
    runner.cancel(&task.id).await.unwrap();
    assert!(lease.token.is_cancelled());
    drop(lease);
    reconcile(&f.server).unwrap();
    let child = f
        .server
        .db
        .get_workflow_run(execution.workflow_run_id.as_ref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(child.run.status, WorkflowRunStatus::Cancelled);
    assert_eq!(
        f.server
            .db
            .get_task_execution(&execution.id)
            .unwrap()
            .unwrap()
            .status,
        TaskExecutionStatus::Cancelled
    );
    assert_eq!(
        f.server.db.get_task(&task.id).unwrap().unwrap().status,
        TaskStatus::Cancelled
    );
    assert!(
        resolve_approval(&f.server, &approval.approval_id, &approval.subject_id, true)
            .await
            .is_err()
    );
    assert!(!f.root.join("result.txt").exists());
}

#[tokio::test]
async fn integration_workflow_legacy_task_reject_and_late_child_cannot_resurrect_parent() {
    use crate::modules::task::model::{TaskExecutionStatus, TaskStatus};
    let f = Fixture::new();
    let (task, execution, approval) = f.legacy_waiting().await;
    resolve_approval(
        &f.server,
        &approval.approval_id,
        &approval.subject_id,
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        f.server
            .db
            .get_task_execution(&execution.id)
            .unwrap()
            .unwrap()
            .status,
        TaskExecutionStatus::Failed
    );
    assert_eq!(
        f.server.db.get_task(&task.id).unwrap().unwrap().status,
        TaskStatus::Failed
    );
    // A late/malformed child status never supersedes a settled parent.
    f.server
        .db
        .conn()
        .execute(
            "UPDATE workflow_runs SET status='completed' WHERE run_id=?1",
            [execution.workflow_run_id.as_ref().unwrap().as_str()],
        )
        .unwrap();
    sync_parent(
        &f.server,
        execution.workflow_run_id.as_ref().unwrap().as_str(),
    )
    .unwrap();
    assert_eq!(
        f.server.db.get_task(&task.id).unwrap().unwrap().status,
        TaskStatus::Failed
    );
    assert!(!f.root.join("result.txt").exists());
}

#[tokio::test]
async fn integration_workflow_public_low_level_resume_rechecks_legacy_cancel_and_newer_attempt() {
    use crate::modules::task::model::{
        TaskExecution, TaskExecutionId, TaskExecutionStatus, TaskStatus,
    };
    for newer in [false, true] {
        let f = Fixture::new();
        let (mut task, mut execution, approval) = f.legacy_waiting().await;
        if newer {
            let mut next = TaskExecution::new(
                TaskExecutionId::generate(),
                task.id.clone(),
                crate::execution::ExecutionContext::new(
                    crate::execution::ExecutionId::generate(),
                    "local-user",
                    "task-runner",
                    None,
                    10,
                ),
                2,
                10,
            );
            next.status = TaskExecutionStatus::Running;
            f.server.db.create_task_execution(&next).unwrap();
        } else {
            execution.status = TaskExecutionStatus::Cancelled;
            task.status = TaskStatus::Cancelled;
            f.server.db.update_task_execution(&execution).unwrap();
            f.server.db.update_task(&task).unwrap();
        }
        let result =
            crate::modules::workflow::application::approval_service::resolve_workflow_approval(
                &crate::modules::workflow::application::run_service::build_gateway(&f.server).await,
                &f.server.approval_store,
                &f.server.db,
                &approval.approval_id,
                &approval.subject_id,
                true,
                &CancellationToken::new(),
                crate::modules::workflow::application::run_service::build_agent_executor(&f.server),
            )
            .await;
        assert!(
            result.is_err(),
            "old public resolver may not bypass the parent guard"
        );
        assert_eq!(
            f.server
                .approval_store
                .get(&approval.approval_id)
                .unwrap()
                .status,
            crate::safety::ApprovalStatus::Pending,
            "a rejected stale parent does not consume approval"
        );
        assert!(!f.root.join("result.txt").exists());
    }
}

#[tokio::test]
async fn integration_workflow_disabled_plugin_blocks_low_level_resume_without_consuming_decision() {
    let f = Fixture::new();
    let (task, _execution, approval) = f.legacy_waiting().await;
    // Declarative inventory fixture only: no plugin code is loaded/installed.
    let manifest=json!({"schema_version":1,"id":"disabled-fixture","name":"Disabled fixture","version":"1.0.0","contributes":{}}).to_string();
    let inventory = json!({"records":{"disabled-fixture":{"package":{"id":"disabled-fixture","name":"Disabled fixture","version":"1.0.0","kind":"plugin","source":"controlled-metadata-fixture","permissions":[],"content_hash":"fixture","files":{"plugin.json":manifest}},"revision":1,"enabled":false,"installed":true,"history":[],"resources":{"workflow.json":"controlled-workflow"},"owned_resource_ids":["controlled-workflow"]}},"previews":[]});
    f.server.db.conn().execute("INSERT INTO settings(key,value) VALUES('v1.managed_imports.v1',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[inventory.to_string()]).unwrap();
    let result =
        crate::modules::workflow::application::approval_service::resolve_workflow_approval(
            &crate::modules::workflow::application::run_service::build_gateway(&f.server).await,
            &f.server.approval_store,
            &f.server.db,
            &approval.approval_id,
            &approval.subject_id,
            true,
            &CancellationToken::new(),
            crate::modules::workflow::application::run_service::build_agent_executor(&f.server),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(
        f.server
            .approval_store
            .get(&approval.approval_id)
            .unwrap()
            .status,
        crate::safety::ApprovalStatus::Pending
    );
    assert!(!f.root.join("result.txt").exists());
    // Revocation blocks execution, while rejecting the old action is allowed.
    resolve_approval(
        &f.server,
        &approval.approval_id,
        &approval.subject_id,
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        f.server.db.get_task(&task.id).unwrap().unwrap().status,
        crate::modules::task::model::TaskStatus::Failed
    );
    assert!(!f.root.join("result.txt").exists());
}

#[tokio::test]
async fn integration_workflow_legacy_child_has_independent_execution_identity_and_typed_owner() {
    let f = Fixture::new();
    let (_task, parent, approval) = f.legacy_waiting().await;
    let child = f
        .server
        .db
        .get_workflow_run(&WorkflowRunId::new(approval.workflow_run_id.unwrap()).unwrap())
        .unwrap()
        .unwrap();
    assert_ne!(
        child.run.execution_context.execution_id,
        parent.execution_context.execution_id
    );
    assert_eq!(
        child.run.execution_context.parent_execution_id.as_ref(),
        Some(&parent.execution_context.execution_id)
    );
    assert_eq!(
        child.run.execution_context.owner,
        Some(crate::execution::ExecutionOwner::Workflow {
            workflow_graph_id: "controlled-workflow".into(),
            run_id: child.run.run_id.to_string()
        })
    );
    assert_eq!(
        legacy::parent(&f.server.db, &child.run)
            .unwrap()
            .unwrap()
            .id,
        parent.id
    );
}
