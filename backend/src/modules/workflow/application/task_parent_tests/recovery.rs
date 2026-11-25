//! Fault-window and committed-result recovery through the original task attempt.
use super::*;

#[tokio::test]
async fn integration_workflow_checkpoint_failure_before_or_after_atomic_write_never_replays() {
    for after_write in [false, true] {
        let f = Fixture::new();
        f.server.config.write().permissions.mode = crate::config::types::PermissionMode::FullAccess;
        let parent = f.save_parent();
        let condition = if after_write {
            "json_extract(NEW.state_json,'$[0].status')='completed'"
        } else {
            "NEW.status='running'"
        };
        f.server.db.conn().execute_batch(&format!("CREATE TRIGGER reject_workflow_checkpoint BEFORE UPDATE ON workflow_runs WHEN {condition} BEGIN SELECT RAISE(ABORT,'controlled workflow checkpoint failure'); END;")).unwrap();
        let failed = crate::modules::task::application::execution_service::dispatch_execution(
            Arc::clone(&f.server),
            parent.graph_id.clone(),
            parent.id.clone(),
        )
        .await
        .unwrap();
        assert_eq!(failed.status, NodeExecutionStatus::Failed);
        assert_eq!(failed.failure_code(), Some("external_mutation_uncertain"));
        assert_eq!(
            f.root.join("result.txt").exists(),
            after_write,
            "test distinguishes known pre-call failure and unknown post-call persistence"
        );
        let executions = || {
            f.server.db.conn().query_row("SELECT COUNT(*) FROM security_audit_events WHERE event_type='execution_started' AND tool_name='write_file'",[],|row|row.get::<_,i64>(0)).unwrap()
        };
        assert_eq!(executions(), if after_write { 1 } else { 0 });
        assert!(f
            .server
            .task_world
            .retry_node(&parent.graph_id, &parent.node_id, 20)
            .is_err());
        f.server
            .db
            .conn()
            .execute_batch("DROP TRIGGER reject_workflow_checkpoint")
            .unwrap();
        reconcile(&f.server).unwrap();
        reconcile(&f.server).unwrap();
        assert_eq!(
            executions(),
            if after_write { 1 } else { 0 },
            "recovery observes and settles without tool calls"
        );
        assert_eq!(
            run_ids(&f.server.db, Some(parent.id.as_str()))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            f.server
                .task_world
                .find_execution(&parent.id)
                .unwrap()
                .1
                .failure_code(),
            Some("external_mutation_uncertain")
        );
    }
}

async fn committed_child_before_parent_checkpoint() -> (Fixture, NodeExecution, String) {
    let fixture = Fixture::new();
    fixture.server.config.write().permissions.mode =
        crate::config::types::PermissionMode::FullAccess;
    let parent = fixture.save_parent();
    fixture
        .server
        .db
        .conn()
        .execute_batch(
            "CREATE TRIGGER reject_parent_receipt BEFORE UPDATE ON task_node_executions
         WHEN NEW.status='succeeded'
         BEGIN SELECT RAISE(ABORT,'controlled parent receipt failure'); END;",
        )
        .unwrap();
    let result = crate::modules::task::application::execution_service::dispatch_execution(
        Arc::clone(&fixture.server),
        parent.graph_id.clone(),
        parent.id.clone(),
    )
    .await;
    assert!(
        result.is_err(),
        "parent receipt storage failure must remain observable"
    );
    let child_ids = run_ids(&fixture.server.db, Some(parent.id.as_str())).unwrap();
    assert_eq!(child_ids.len(), 1);
    let child_id = child_ids[0].clone();
    let child = fixture
        .server
        .db
        .get_workflow_run(&WorkflowRunId::new(child_id.as_str()).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(child.run.status, WorkflowRunStatus::Completed);
    assert!(fixture
        .server
        .db
        .load_node_execution(&parent.id)
        .unwrap()
        .unwrap()
        .status
        .is_active());
    fixture
        .server
        .db
        .conn()
        .execute_batch("DROP TRIGGER reject_parent_receipt")
        .unwrap();
    (fixture, parent, child_id)
}

#[tokio::test]
async fn integration_workflow_restart_recovers_committed_child_without_replaying() {
    let (fixture, parent, child_id) = committed_child_before_parent_checkpoint().await;
    let root = fixture.root.clone();
    drop(fixture);
    for _ in 0..2 {
        let server = AppServer::new_with_control_session_and_store(
            &root.join("state.db"),
            root.to_str().unwrap(),
            ControlSession::generate(),
            Arc::new(InMemorySecretStore::new()),
        )
        .unwrap();
        reconcile(&server).unwrap();
        let recovered = server.task_world.find_execution(&parent.id).unwrap().1;
        assert_eq!(recovered.status, NodeExecutionStatus::Succeeded);
        assert_eq!(
            recovered.output.as_ref().unwrap()["workflow_run_id"],
            child_id
        );
        assert_eq!(recovered.failure_code(), None);
        assert_eq!(
            server
                .task_world
                .list_node_executions(&parent.graph_id, &parent.node_id)
                .unwrap()
                .len(),
            1
        );
        let starts: i64 = server.db.conn().query_row(
            "SELECT COUNT(*) FROM security_audit_events WHERE event_type='execution_started' AND tool_name='write_file'",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(
            starts, 1,
            "recovery must consume the committed result, not repeat its write"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("result.txt")).unwrap(),
            "one controlled write"
        );
    }
}

#[tokio::test]
async fn integration_workflow_committed_child_does_not_replace_cancelled_parent() {
    let (fixture, mut parent, _) = committed_child_before_parent_checkpoint().await;
    parent
        .transition(NodeExecutionStatus::Cancelled, 50)
        .unwrap();
    fixture.server.db.update_node_execution(&parent).unwrap();
    let root = fixture.root.clone();
    drop(fixture);
    let server = AppServer::new_with_control_session_and_store(
        &root.join("state.db"),
        root.to_str().unwrap(),
        ControlSession::generate(),
        Arc::new(InMemorySecretStore::new()),
    )
    .unwrap();
    reconcile(&server).unwrap();
    assert_eq!(
        server
            .task_world
            .find_execution(&parent.id)
            .unwrap()
            .1
            .status,
        NodeExecutionStatus::Cancelled
    );
    let starts: i64 = server.db.conn().query_row(
        "SELECT COUNT(*) FROM security_audit_events WHERE event_type='execution_started' AND tool_name='write_file'",
        [], |row| row.get(0),
    ).unwrap();
    assert_eq!(starts, 1);
}

#[tokio::test]
async fn integration_workflow_committed_receipt_rejects_conflicting_user_review() {
    let (fixture, parent, _) = committed_child_before_parent_checkpoint().await;
    let root = fixture.root.clone();
    drop(fixture);
    let server = AppServer::new_with_control_session_and_store(
        &root.join("state.db"),
        root.to_str().unwrap(),
        ControlSession::generate(),
        Arc::new(InMemorySecretStore::new()),
    )
    .unwrap();
    let before = server.db.load_node_execution(&parent.id).unwrap().unwrap();
    assert_eq!(before.failure_code(), Some("execution_interrupted"));
    assert!(matches!(
        server.task_world.record_no_effect_review(
            &parent.graph_id,
            &parent.id,
            1,
            "用户报告没有外部效果",
            chrono::Utc::now().timestamp_millis(),
        ),
        Err(crate::modules::task::TaskWorldRuntimeError::Harness(
            crate::modules::task::TaskHarnessError::ExternalResultConflict(_)
        ))
    ));
    assert_eq!(
        server.db.load_node_execution(&parent.id).unwrap().unwrap(),
        before
    );
    reconcile(&server).unwrap();
    let settled = server.task_world.find_execution(&parent.id).unwrap().1;
    assert_eq!(settled.status, NodeExecutionStatus::Succeeded);
    assert!(settled.external_review.is_none());
    let starts: i64 = server.db.conn().query_row(
        "SELECT COUNT(*) FROM security_audit_events WHERE event_type='execution_started' AND tool_name='write_file'",
        [], |row| row.get(0),
    ).unwrap();
    assert_eq!(starts, 1);
}
