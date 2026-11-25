use super::*;
use crate::modules::task::{
    proofloop::*, NodeContext, NodeExecution, NodeExecutionId, NodeExecutionStatus, TaskNodeId,
    TaskWorldRuntime, ValidationResult,
};
use crate::shared::event::EventHub;
use crate::{db::MessageRow, integrations::secret::InMemorySecretStore, safety::ControlSession};
use serde_json::{json, Value};

fn server() -> AppServer {
    let root = std::env::temp_dir().join(format!("integration-reconcile-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    AppServer::new_with_control_session_and_store(
        &root.join("test.db"),
        root.to_str().unwrap(),
        ControlSession::new("a".repeat(64)).unwrap(),
        Arc::new(InMemorySecretStore::new()),
    )
    .unwrap()
}
fn accept(s: &AppServer, c: &str, id: &str, at: i64) {
    s.db.accept_conversation_turn(&MessageRow {
        id: id.into(),
        conversation_id: c.into(),
        role: "user".into(),
        content: "controlled input".into(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_result: None,
        created_at: at,
    })
    .unwrap();
}

#[tokio::test]
async fn integration_receipt_repairs_projection_failure_without_running_work() {
    let s = server();
    let c = s.db.create_conversation("test").unwrap();
    accept(&s, &c.id, "turn-user", 10);
    let receipt =
        s.db.record_conversation_outcome(&c.id, ConversationRunStatus::Completed, None)
            .unwrap();
    s.db.conn().execute_batch("CREATE TRIGGER fail_outcome_projection BEFORE UPDATE ON conversations BEGIN SELECT RAISE(ABORT,'controlled projection failure'); END;").unwrap();
    assert!(project_conversation_outcome(&s, &receipt).is_err());
    assert_eq!(s.db.pending_conversation_outcomes(32).unwrap().len(), 1);
    s.db.conn()
        .execute_batch("DROP TRIGGER fail_outcome_projection;")
        .unwrap();
    reconcile_once(&s).await.unwrap();
    reconcile_once(&s).await.unwrap();
    let stored = s.db.get_conversation(&c.id).unwrap();
    assert_eq!(stored.run_status, ConversationRunStatus::Completed);
    assert_eq!(stored.messages.len(), 1);
    assert!(stored.execution_history.is_empty());
    assert!(s.db.pending_conversation_outcomes(32).unwrap().is_empty());
}

#[test]
fn integration_late_receipt_does_not_finish_new_user_turn_or_undo_cancel() {
    let s = server();
    let c = s.db.create_conversation("test").unwrap();
    accept(&s, &c.id, "old-user", 10);
    let old =
        s.db.record_conversation_outcome(&c.id, ConversationRunStatus::Completed, None)
            .unwrap();
    accept(&s, &c.id, "new-user", 11);
    project_conversation_outcome(&s, &old).unwrap();
    assert_eq!(
        s.db.get_conversation(&c.id).unwrap().run_status,
        ConversationRunStatus::Running
    );
    let done =
        s.db.record_conversation_outcome(&c.id, ConversationRunStatus::Completed, None)
            .unwrap();
    s.db.set_conversation_run_status(&c.id, ConversationRunStatus::Cancelled, None)
        .unwrap();
    assert_eq!(
        project_conversation_outcome(&s, &done).unwrap(),
        ConversationRunStatus::Cancelled
    );
}

#[test]
fn integration_repeated_approval_and_question_segments_settle_the_latest_wait() {
    let s = server();
    let c = s.db.create_conversation("multi waiting fixture").unwrap();
    accept(&s, &c.id, "one-user-turn", 10);
    for (segment, status) in [
        ("approval-one", ConversationRunStatus::WaitingApproval),
        ("question-two", ConversationRunStatus::WaitingInput),
        ("approval-three", ConversationRunStatus::WaitingApproval),
    ] {
        s.db.set_conversation_run_status(&c.id, ConversationRunStatus::Running, None)
            .unwrap();
        let receipt =
            s.db.record_conversation_segment_outcome(&c.id, segment, status, None)
                .unwrap();
        assert_eq!(project_conversation_outcome(&s, &receipt).unwrap(), status);
    }
    assert_eq!(
        s.db.get_conversation(&c.id).unwrap().run_status,
        ConversationRunStatus::WaitingApproval
    );
    assert_eq!(
        s.db.conn()
            .query_row(
                "SELECT count(*) FROM execution_outcomes WHERE conversation_id=?1",
                [&c.id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        3
    );
}

#[tokio::test]
async fn integration_busy_receipts_do_not_starve_a_later_projection() {
    let s = server();
    let mut gates = Vec::new();
    let mut final_id = String::new();
    for i in 0..33 {
        let c = s.db.create_conversation("fairness fixture").unwrap();
        accept(&s, &c.id, &format!("user-{i}"), 10 + i);
        s.db.record_conversation_outcome(&c.id, ConversationRunStatus::Completed, None)
            .unwrap();
        if i < 16 {
            s.active_tasks
                .lock()
                .insert(c.id.clone(), tokio_util::sync::CancellationToken::new());
        } else if i < 32 {
            gates.push(crate::api::chat::lifecycle::try_acquire_turn(&s, &c.id).unwrap());
        }
        final_id = c.id;
    }
    reconcile_once(&s).await.unwrap();
    reconcile_once(&s).await.unwrap();
    assert_eq!(
        s.db.get_conversation(&final_id).unwrap().run_status,
        ConversationRunStatus::Completed
    );
    let retries: i64 =
        s.db.conn()
            .query_row("SELECT sum(attempts) FROM execution_outcomes", [], |r| {
                r.get(0)
            })
            .unwrap();
    assert_eq!(retries, 0);
    drop(gates);
}

#[test]
fn integration_atomic_projection_cannot_undo_concurrent_stop_or_new_turn() {
    let s = server();
    for i in 0..16 {
        let c = s.db.create_conversation("stop race").unwrap();
        accept(&s, &c.id, &format!("race-user-{i}"), 10 + i);
        let receipt =
            s.db.record_conversation_outcome(&c.id, ConversationRunStatus::Completed, None)
                .unwrap();
        let gate = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            let reader = &gate;
            let db = s.db.clone();
            let id = c.id.clone();
            let turn = receipt.turn_key.clone();
            let worker = scope.spawn(move || {
                reader.wait();
                db.commit_conversation_outcome_guarded(
                    &id,
                    ConversationRunStatus::Completed,
                    None,
                    Some(&turn),
                )
                .unwrap();
            });
            gate.wait();
            s.db.set_conversation_run_status(&c.id, ConversationRunStatus::Cancelled, None)
                .unwrap();
            worker.join().unwrap();
        });
        assert_eq!(
            s.db.get_conversation(&c.id).unwrap().run_status,
            ConversationRunStatus::Cancelled
        );
        accept(&s, &c.id, &format!("new-race-user-{i}"), 100 + i);
        assert_eq!(
            s.db.commit_conversation_outcome_guarded(
                &c.id,
                ConversationRunStatus::Completed,
                None,
                Some(&receipt.turn_key)
            )
            .unwrap(),
            ConversationRunStatus::Running
        );
    }
}

#[tokio::test]
async fn integration_receipt_repairs_personal_projection_after_conversation_commit() {
    use crate::modules::automation::{
        self,
        model::{AutomationInput, AutomationTarget, Occurrence, Schedule},
    };
    let s = server();
    let ws = crate::workspace::Workspace::new(
        crate::workspace::WorkspaceId::generate(),
        "Personal recovery".into(),
        String::new(),
        Some(s.workspace_root.clone()),
        1,
    )
    .unwrap();
    s.db.create_workspace(&ws).unwrap();
    let a = automation::save(
        &s,
        None,
        AutomationInput {
            expected_revision: None,
            name: "Recovery fixture".into(),
            workflow_graph_id: String::new(),
            target: Some(AutomationTarget::PersonalAssistant {
                goal: "Maintain the authorized local report".into(),
                conversation_id: String::new(),
                root_path: String::new(),
                allow_file_writes: false,
                allow_file_discovery: false,
                max_runs: 5,
            }),
            workspace_id: ws.id.to_string(),
            timezone: "UTC".into(),
            schedule: Schedule::Daily {
                time: "09:00".into(),
            },
            enabled: true,
        },
    )
    .unwrap();
    let c = match a.target.as_ref().unwrap() {
        AutomationTarget::PersonalAssistant {
            conversation_id, ..
        } => conversation_id.clone(),
        _ => unreachable!(),
    };
    accept(&s, &c, "personal-user", 10);
    let mut occurrence:Occurrence=serde_json::from_value(json!({"id":"personal-recovery-occurrence","automation_id":a.id,"automation_revision":a.revision,"timezone":"UTC","occurrence_key":"manual:controlled-recovery","trigger":"manual","scheduled_at":10,"workspace_id":a.workspace_id,"workflow_graph_id":"","run_id":null,"conversation_id":c,"assistant_turn_id":"personal-user","status":"running","error":null,"created_at":10,"updated_at":10})).unwrap();
    assert!(s
        .db
        .claim_automation_occurrence(&a, a.next_run_at, &mut occurrence, true)
        .unwrap());
    s.db.add_message(&MessageRow {
        id: "personal-report".into(),
        conversation_id: c.clone(),
        role: "assistant".into(),
        content: "The controlled report is complete.".into(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_result: None,
        created_at: 11,
    })
    .unwrap();
    let receipt =
        s.db.record_conversation_outcome(&c, ConversationRunStatus::Completed, None)
            .unwrap();
    s.db.conn().execute_batch("CREATE TRIGGER reject_personal_projection BEFORE UPDATE ON automation_occurrences BEGIN SELECT RAISE(ABORT,'controlled occurrence persistence failure'); END;").unwrap();
    assert!(project_conversation_outcome(&s, &receipt).is_err());
    assert_eq!(
        s.db.get_conversation(&c).unwrap().run_status,
        ConversationRunStatus::Completed
    );
    s.db.conn()
        .execute_batch("DROP TRIGGER reject_personal_projection;")
        .unwrap();
    reconcile_once(&s).await.unwrap();
    reconcile_once(&s).await.unwrap();
    let payload: String =
        s.db.conn()
            .query_row(
                "SELECT payload FROM automation_occurrences WHERE id=?1",
                [&occurrence.id],
                |r| r.get(0),
            )
            .unwrap();
    let stored: Occurrence = serde_json::from_str(&payload).unwrap();
    assert_eq!(stored.status, "completed");
    assert_eq!(
        stored.report.as_deref(),
        Some("The controlled report is complete.")
    );
    assert_eq!(s.db.get_conversation(&c).unwrap().messages.len(), 2);
}

fn confirmed_goal(s: &mut AppServer) -> (GoalContract, TaskGraphId, String) {
    let bundle: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/proofloop/declared-two-node.json"
    ))
    .unwrap();
    let mut contract: GoalContract = serde_json::from_value(bundle["contract"].clone()).unwrap();
    contract.criteria.push(Criterion {
        id: "human-quality".into(),
        description: "human quality review".into(),
        required: true,
        check: CriterionCheck::HumanReview {
            deliverable_id: contract.deliverables[0].id.clone(),
            rubric: "Review the produced content".into(),
        },
    });
    let snapshot: CapabilitySnapshot = serde_json::from_value(bundle["snapshot"].clone()).unwrap();
    let candidate: CandidatePlan = serde_json::from_value(bundle["candidate"].clone()).unwrap();
    let graph = TaskGraphId::generate();
    let now = chrono::Utc::now().timestamp_millis();
    let draft = ProofDraft {
        source: None,
        planning_review: None,
        draft_id: "integration-basic-acceptance".into(),
        revision: 1,
        graph_id: graph.to_string(),
        contract: contract.clone(),
        snapshot: snapshot.clone(),
        candidate: Some(candidate.clone()),
        binding_confirmations: vec![],
        compilation: compile_candidate(&contract, &candidate, &snapshot, &graph, &[]),
        status: DraftStatus::ReadyForConfirmation,
        confirmed_graph_revision: None,
        planning_usage: PlanningUsage::default(),
    };
    s.db.save_proof_draft(&draft, now).unwrap();
    DraftService::new(s.db.clone())
        .confirm_draft(&s.task_world, &draft.draft_id, 1, &snapshot, now)
        .unwrap();
    let workspace = crate::workspace::Workspace::new(
        crate::workspace::WorkspaceId::new(&contract.workspace_id).unwrap(),
        "Integration workspace".into(),
        String::new(),
        Some(s.workspace_root.clone()),
        now,
    )
    .unwrap();
    s.db.create_workspace(&workspace).unwrap();
    let mut last = String::new();
    for node in ["collect", "compare"] {
        let mut e = NodeExecution::new(
            NodeExecutionId::generate(),
            graph.clone(),
            TaskNodeId::new(node).unwrap(),
            1,
            None,
            NodeContext::new(
                "controlled goal",
                "controlled evidence",
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
            )
            .unwrap(),
            now,
        )
        .unwrap();
        e.status = NodeExecutionStatus::Succeeded;
        e.output = Some(json!({"structured":{"summary":"controlled evidence"}}));
        e.validation = Some(ValidationResult::accepted());
        e.finished_at = Some(now);
        last = e.id.to_string();
        s.db.save_node_execution(&e).unwrap();
    }
    // The evidence is an explicitly constructed fixture; no model or executor
    // is claimed. Reloading the runtime exercises persisted attempt recovery.
    s.task_world = TaskWorldRuntime::new(&s.db, EventHub::new(16)).unwrap();
    (contract, graph, last)
}

#[tokio::test]
async fn integration_basic_acceptance_runs_without_any_frontend_and_is_idempotent() {
    let mut s = server();
    let (contract, graph, execution) = confirmed_goal(&mut s);
    let source = crate::modules::task::artifact::ArtifactSource::Node {
        graph_id: graph.to_string(),
        node_id: "compare".into(),
        execution_id: execution,
        workspace_id: contract.workspace_id.clone(),
    };
    crate::modules::task::artifact::ArtifactService::new(s.db.clone())
        .materialize(
            &source,
            &contract.deliverables[0].title,
            &s.workspace_root,
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
    s.db.recover_integration_handoffs().unwrap();
    reconcile_once(&s).await.unwrap();
    let reports = s.db.list_acceptance_reports(&contract.goal_id).unwrap();
    assert_eq!(reports.len(), 1);
    let report_id = reports[0].report_id.clone();
    assert!(reports[0]
        .results
        .iter()
        .any(|r| r.criterion_id == "human-quality" && r.status == CriterionStatus::HumanPending));
    s.db.queue_goal_evaluation(graph.as_str()).unwrap();
    reconcile_once(&s).await.unwrap();
    assert_eq!(
        s.db.list_acceptance_reports(&contract.goal_id).unwrap()[0].report_id,
        report_id
    );
    assert_eq!(
        s.db.list_acceptance_reports(&contract.goal_id)
            .unwrap()
            .len(),
        1
    );
    let gate = TaskGraphId::new(graph.to_string()).unwrap();
    s.task_world
        .pause_task(&gate, chrono::Utc::now().timestamp_millis())
        .unwrap();
    s.db.queue_goal_evaluation(graph.as_str()).unwrap();
    assert!(!evaluate_graph_if_ready(&s, graph.as_str()).unwrap());
}

#[test]
fn integration_late_automatic_report_insert_preserves_committed_human_review() {
    let mut s = server();
    let (contract, graph, execution) = confirmed_goal(&mut s);
    let source = crate::modules::task::artifact::ArtifactSource::Node {
        graph_id: graph.to_string(),
        node_id: "compare".into(),
        execution_id: execution,
        workspace_id: contract.workspace_id.clone(),
    };
    crate::modules::task::artifact::ArtifactService::new(s.db.clone())
        .materialize(
            &source,
            &contract.deliverables[0].title,
            &s.workspace_root,
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
    assert!(evaluate_graph_if_ready(&s, graph.as_str()).unwrap());
    let pending =
        s.db.list_acceptance_reports(&contract.goal_id)
            .unwrap()
            .remove(0);
    let (_, snapshot) =
        s.db.get_acceptance_report(&pending.report_id)
            .unwrap()
            .unwrap();
    let mut reviewed = pending.clone();
    reviewed.human_reviews.push(HumanReview {
        criterion_id: "human-quality".into(),
        decision: "pass".into(),
        reason: "Explicit controlled human decision".into(),
        reviewer_subject: "local-user".into(),
        reviewed_at_ms: 100,
    });
    let (ready, after_review) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let db = s.db.clone();
        let original = pending.clone();
        let snap = snapshot.clone();
        let worker = scope.spawn(move || {
            after_review.recv().unwrap();
            db.insert_acceptance_report_if_current(&original, &snap, 200)
                .unwrap();
        });
        s.db.save_acceptance_report_if_current(&reviewed, &snapshot, 100)
            .unwrap();
        ready.send(()).unwrap();
        worker.join().unwrap();
    });
    let (stored, _) =
        s.db.get_acceptance_report(&pending.report_id)
            .unwrap()
            .unwrap();
    assert_eq!(stored.human_reviews, reviewed.human_reviews);
    // A new attempt changes the evidence set inside the write transaction.
    let mut newer = s
        .db
        .load_node_execution(&NodeExecutionId::new(&snapshot.executions[0].execution_id).unwrap())
        .unwrap()
        .unwrap();
    newer.id = NodeExecutionId::generate();
    newer.attempt += 1;
    newer.status = NodeExecutionStatus::Running;
    s.db.save_node_execution(&newer).unwrap();
    assert!(s
        .db
        .insert_acceptance_report_if_current(&pending, &snapshot, 201)
        .is_err());
}

#[tokio::test]
async fn integration_deleted_goal_tombstone_settles_observation_without_false_error() {
    let mut s = server();
    let (contract, graph, _) = confirmed_goal(&mut s);
    let revision = s.task_world.get_graph_detail(&graph).unwrap().revision;
    s.task_world.delete_graph(&graph, revision, 9000).unwrap();
    s.db.recover_integration_handoffs().unwrap();
    assert!(!s.db.pending_goal_evaluations().unwrap().is_empty());
    reconcile_once(&s).await.unwrap();
    assert!(s.db.pending_goal_evaluations().unwrap().is_empty());
    assert!(s.db.integration_projection_issues().unwrap().is_empty());
    assert!(s
        .db
        .list_acceptance_reports(&contract.goal_id)
        .unwrap()
        .is_empty());
}
