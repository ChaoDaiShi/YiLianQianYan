use super::*;

#[tokio::test]
async fn integration_workflow_child_approval_resumes_exact_original_attempt_once() {
    let f = Fixture::new();
    let (parent, approval) = f.waiting().await;
    let run_id = approval.workflow_run_id.clone().unwrap();
    let child = f
        .server
        .db
        .get_workflow_run(&WorkflowRunId::new(&*run_id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        child
            .run
            .execution_context
            .parent_execution_id
            .as_ref()
            .map(|id| id.as_str()),
        Some(parent.id.as_str())
    );
    resolve_approval(&f.server, &approval.approval_id, &approval.subject_id, true)
        .await
        .unwrap();
    assert_eq!(
        f.server
            .task_world
            .find_execution(&parent.id)
            .unwrap()
            .1
            .status,
        NodeExecutionStatus::Succeeded
    );
    assert_eq!(
        std::fs::read_to_string(f.root.join("result.txt")).unwrap(),
        "one controlled write"
    );
    assert_eq!(
        run_ids(&f.server.db, Some(parent.id.as_str())).unwrap(),
        vec![run_id]
    );
    assert!(
        resolve_approval(&f.server, &approval.approval_id, &approval.subject_id, true)
            .await
            .is_err()
    );
    assert_eq!(
        f.server
            .task_world
            .list_node_executions(&parent.graph_id, &parent.node_id)
            .unwrap()
            .len(),
        1
    );
    assert!(f.server.active_workflow_runs.lock().is_empty());
}

#[tokio::test]
async fn integration_workflow_rejection_fails_original_parent_without_write() {
    let f = Fixture::new();
    let (parent, approval) = f.waiting().await;
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
            .task_world
            .find_execution(&parent.id)
            .unwrap()
            .1
            .status,
        NodeExecutionStatus::Failed
    );
    assert!(!f.root.join("result.txt").exists());
    assert!(
        resolve_approval(&f.server, &approval.approval_id, &approval.subject_id, true)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn integration_workflow_parent_stop_cancels_saved_child_and_forbids_approval() {
    let f = Fixture::new();
    let (parent, approval) = f.waiting().await;
    f.server
        .task_world
        .cancel_execution(&parent.graph_id, &parent.id, 1, 10)
        .unwrap();
    reconcile(&f.server).unwrap();
    let child = f
        .server
        .db
        .get_workflow_run(&WorkflowRunId::new(approval.workflow_run_id.clone().unwrap()).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(child.run.status, WorkflowRunStatus::Cancelled);
    assert!(
        resolve_approval(&f.server, &approval.approval_id, &approval.subject_id, true)
            .await
            .is_err()
    );
    assert_eq!(
        f.server
            .task_world
            .find_execution(&parent.id)
            .unwrap()
            .1
            .status,
        NodeExecutionStatus::Cancelled
    );
    assert!(f.server.approval_store.list_pending().is_empty());
    assert!(!f.root.join("result.txt").exists());
}

#[tokio::test]
async fn integration_graph_cancel_settles_waiting_child_and_removes_pending_approval() {
    for pause_first in [false, true] {
        let f = Fixture::new();
        let (parent, approval) = f.waiting().await;
        let child_id = WorkflowRunId::new(approval.workflow_run_id.as_deref().unwrap()).unwrap();
        if pause_first {
            f.server
                .task_world
                .pause_task(&parent.graph_id, 10)
                .unwrap();
            assert_eq!(
                f.server
                    .task_world
                    .find_execution(&parent.id)
                    .unwrap()
                    .1
                    .status,
                NodeExecutionStatus::WaitingApproval
            );
        }

        f.server
            .task_world
            .cancel_task(&parent.graph_id, 11)
            .unwrap();
        assert_eq!(
            f.server
                .task_world
                .find_execution(&parent.id)
                .unwrap()
                .1
                .status,
            NodeExecutionStatus::Cancelled
        );
        assert_eq!(
            f.server
                .db
                .get_workflow_run(&child_id)
                .unwrap()
                .unwrap()
                .run
                .status,
            WorkflowRunStatus::Cancelled
        );
        reconcile(&f.server).unwrap();
        assert!(f.server.approval_store.list_pending().is_empty());
        assert!(
            resolve_approval(&f.server, &approval.approval_id, &approval.subject_id, true)
                .await
                .is_err()
        );
        assert!(!f.root.join("result.txt").exists());
    }
}

#[tokio::test]
async fn integration_workflow_restart_reconciles_orphan_without_replaying_write() {
    let f = Fixture::new();
    let (parent, approval) = f.waiting().await;
    let root = f.root.clone();
    let run_id = approval.workflow_run_id.clone().unwrap();
    drop(f);
    let server = AppServer::new_with_control_session_and_store(
        &root.join("state.db"),
        root.to_str().unwrap(),
        ControlSession::generate(),
        Arc::new(InMemorySecretStore::new()),
    )
    .unwrap();
    reconcile(&server).unwrap();
    reconcile(&server).unwrap();
    assert_eq!(
        server
            .task_world
            .find_execution(&parent.id)
            .unwrap()
            .1
            .status,
        NodeExecutionStatus::Failed
    );
    let child = server
        .db
        .get_workflow_run(&WorkflowRunId::new(&*run_id).unwrap())
        .unwrap()
        .unwrap();
    assert!(child.run.status.is_terminal());
    assert!(server.approval_store.list_pending().is_empty());
    assert!(!root.join("result.txt").exists());
}

#[tokio::test]
async fn integration_workflow_cancelled_child_discards_late_completed_snapshot() {
    let f = Fixture::new();
    let (parent, approval) = f.waiting().await;
    let mut child = f
        .server
        .db
        .get_workflow_run(&WorkflowRunId::new(approval.workflow_run_id.unwrap()).unwrap())
        .unwrap()
        .unwrap();
    let mut late = child.run.clone();
    child.run.cancel(10);
    f.server
        .db
        .update_workflow_run(&child.workflow_graph_id, &child.run)
        .unwrap();
    late.status = WorkflowRunStatus::Completed;
    assert!(f
        .server
        .db
        .update_workflow_run(&child.workflow_graph_id, &late)
        .is_err());
    sync_parent(&f.server, child.run.run_id.as_str()).unwrap();
    assert_eq!(
        f.server
            .task_world
            .find_execution(&parent.id)
            .unwrap()
            .1
            .status,
        NodeExecutionStatus::Cancelled
    );
    assert!(!f.root.join("result.txt").exists());
}

#[tokio::test]
async fn integration_workflow_unknown_child_result_retains_non_replayable_code() {
    let f = Fixture::new();
    let (parent, approval) = f.waiting().await;
    let mut child = f
        .server
        .db
        .get_workflow_run(&WorkflowRunId::new(approval.workflow_run_id.unwrap()).unwrap())
        .unwrap()
        .unwrap();
    child.run.status = WorkflowRunStatus::Failed;
    child.run.node_states[0].status = crate::modules::workflow::NodeRunStatus::Failed;
    child.run.node_states[0].error =
        Some("external_mutation_uncertain: atomic operation result is unknown".into());
    f.server
        .db
        .update_workflow_run(&child.workflow_graph_id, &child.run)
        .unwrap();
    sync_parent(&f.server, child.run.run_id.as_str()).unwrap();
    let original = f.server.task_world.find_execution(&parent.id).unwrap().1;
    assert_eq!(original.failure_code(), Some("external_mutation_uncertain"));
    assert!(f
        .server
        .task_world
        .retry_node(&parent.graph_id, &parent.node_id, 20)
        .is_err());
    assert!(!f.root.join("result.txt").exists());
}

#[tokio::test]
async fn integration_workflow_sequential_approvals_keep_original_child_and_parent() {
    let f = Fixture::new();
    let parent = f.save_parent();
    let mut graph = f
        .server
        .db
        .get_workflow_graph("controlled-workflow")
        .unwrap()
        .unwrap();
    graph.definition.nodes.insert(1,WorkflowNodeDefinition{id:WorkflowNodeId::new("second").unwrap(),kind:WorkflowNodeKind::Tool,config:WorkflowNodeConfig::Tool{tool_name:"write_file".into(),arguments:json!({"path":f.root.join("second.txt").to_str().unwrap(),"content":"second controlled write"})}});
    graph.definition.edges = vec![
        WorkflowEdgeDefinition {
            from: WorkflowNodeId::new("write").unwrap(),
            to: WorkflowNodeId::new("second").unwrap(),
        },
        WorkflowEdgeDefinition {
            from: WorkflowNodeId::new("second").unwrap(),
            to: WorkflowNodeId::new("done").unwrap(),
        },
    ];
    f.server
        .db
        .update_workflow_graph("controlled-workflow", &graph)
        .unwrap();
    crate::modules::task::application::execution_service::dispatch_execution(
        Arc::clone(&f.server),
        parent.graph_id.clone(),
        parent.id.clone(),
    )
    .await
    .unwrap();
    let first = f
        .server
        .approval_store
        .list_pending()
        .into_iter()
        .next()
        .unwrap();
    resolve_approval(&f.server, &first.approval_id, &first.subject_id, true)
        .await
        .unwrap();
    let second = f
        .server
        .approval_store
        .list_pending()
        .into_iter()
        .next()
        .unwrap();
    assert_ne!(first.approval_id, second.approval_id);
    assert_eq!(first.workflow_run_id, second.workflow_run_id);
    let waiting = f.server.task_world.find_execution(&parent.id).unwrap().1;
    assert_eq!(waiting.status, NodeExecutionStatus::WaitingApproval);
    assert_eq!(
        waiting.approval_ref.as_deref(),
        Some(second.approval_id.as_str())
    );
    reconcile(&f.server).unwrap();
    reconcile(&f.server).unwrap();
    assert!(!f.root.join("second.txt").exists());
    resolve_approval(&f.server, &second.approval_id, &second.subject_id, true)
        .await
        .unwrap();
    assert_eq!(
        f.server
            .task_world
            .find_execution(&parent.id)
            .unwrap()
            .1
            .status,
        NodeExecutionStatus::Succeeded
    );
    assert_eq!(
        run_ids(&f.server.db, Some(parent.id.as_str()))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        std::fs::read_to_string(f.root.join("second.txt")).unwrap(),
        "second controlled write"
    );
}

#[tokio::test]
async fn integration_workflow_deleted_completed_parent_and_corrupt_neighbor_do_not_block_reconciliation(
) {
    let f = Fixture::new();
    let (parent, approval) = f.waiting().await;
    resolve_approval(&f.server, &approval.approval_id, &approval.subject_id, true)
        .await
        .unwrap();
    let settled_run = approval.workflow_run_id.unwrap();
    f.server
        .task_world
        .delete_graph(&parent.graph_id, 1, 30)
        .unwrap();
    // Direct sync is safe too, even when no runtime harness remains.
    sync_parent(&f.server, &settled_run).unwrap();
    let graph = TaskGraphId::new("unrelated-live-graph").unwrap();
    let node = TaskNodeId::new("work").unwrap();
    f.server
        .task_world
        .create_graph(
            graph.clone(),
            vec![TaskNode::new(
                node.clone(),
                TaskNodeKind::Work,
                "Other work",
                json!({"executor_ref":"workflow://controlled-workflow"}),
            )
            .unwrap()],
            vec![],
            31,
        )
        .unwrap();
    let next = f
        .server
        .task_world
        .start_execution_with_resolver(
            &graph,
            &node,
            1,
            ExecutorResolver::new().with_workflow("controlled-workflow"),
            32,
        )
        .unwrap();
    crate::modules::task::application::execution_service::dispatch_execution(
        Arc::clone(&f.server),
        graph.clone(),
        next.id.clone(),
    )
    .await
    .unwrap();
    let valid = f
        .server
        .approval_store
        .list_pending()
        .into_iter()
        .next()
        .unwrap();
    // An unrelated malformed saved row is a fixture for per-owner isolation.
    f.server.db.conn().execute("INSERT INTO workflow_runs(run_id,workflow_graph_id,execution_id,subject_id,agent_name,parent_execution_id,status,definition_snapshot_json,state_json,created_at,updated_at) SELECT 'corrupt-neighbor',workflow_graph_id,'corrupt-execution',subject_id,agent_name,'missing-parent','waiting_approval','invalid','invalid',created_at,updated_at FROM workflow_runs WHERE run_id=?1",[valid.workflow_run_id.as_deref().unwrap()]).unwrap();
    reconcile(&f.server).unwrap();
    reconcile(&f.server).unwrap();
    assert_eq!(
        f.server
            .task_world
            .find_execution(&next.id)
            .unwrap()
            .1
            .status,
        NodeExecutionStatus::WaitingApproval
    );
    assert!(f.server.task_world.find_execution(&parent.id).is_none());
    let manifest = crate::execution::context_manifest::load_manifest(
        &f.server.db,
        &crate::execution::ExecutionOwner::Workflow {
            workflow_graph_id: "controlled-workflow".into(),
            run_id: valid.workflow_run_id.unwrap(),
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        manifest.total_chars, 0,
        "tool/output workflows have no context prompt consumer"
    );
    let harness = manifest
        .entries
        .iter()
        .find(|entry| entry.kind == "harness")
        .unwrap();
    assert_eq!(
        harness.source_id,
        crate::modules::harness_profiles::DEFAULT_ID
    );
    assert_eq!(harness.chars, 0);
    assert!(manifest
        .entries
        .iter()
        .filter(|entry| entry.kind != "harness")
        .all(
            |entry| entry.status == crate::execution::context_manifest::ContextStatus::Unsupported
        ));
}

#[tokio::test]
async fn integration_workflow_reconciliation_preserves_claimed_approval_until_its_future_settles() {
    let f = Fixture::new();
    let (parent, approval) = f.waiting().await;
    let lease = claim_approval(&f.server, &approval).unwrap();
    f.server
        .approval_store
        .consume_for_approval(&approval.approval_id, &approval.conversation_id)
        .unwrap();
    reconcile(&f.server).unwrap();
    assert_eq!(
        f.server
            .task_world
            .find_execution(&parent.id)
            .unwrap()
            .1
            .status,
        NodeExecutionStatus::WaitingApproval
    );
    assert!(!f.root.join("result.txt").exists());
    drop(lease);
    // A dropped future with a consumed approval and no durable outcome is
    // genuinely uncertain, and may not be automatically dispatched again.
    reconcile(&f.server).unwrap();
    let unknown = f.server.task_world.find_execution(&parent.id).unwrap().1;
    assert_eq!(unknown.status, NodeExecutionStatus::Failed);
    assert_eq!(unknown.failure_code(), Some("external_mutation_uncertain"));
    assert!(!f.root.join("result.txt").exists());
}
