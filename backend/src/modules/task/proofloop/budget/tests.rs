use super::*;
use crate::modules::task::proofloop::{
    compile_candidate, CandidatePlan, CapabilitySnapshot, DraftService, DraftStatus,
    RepairOperation,
};
use crate::modules::task::runtime::TaskWorldRuntime;
use crate::shared::event::EventHub;

fn setup() -> (Database, Arc<GoalBudgetMeter>, ProofDraft) {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/proofloop/declared-two-node.json"
    ))
    .unwrap();
    let mut contract: GoalContract = serde_json::from_value(fixture["contract"].clone()).unwrap();
    contract.budget.max_wall_time_ms = 600_000;
    let snapshot: CapabilitySnapshot = serde_json::from_value(fixture["snapshot"].clone()).unwrap();
    let candidate: CandidatePlan = serde_json::from_value(fixture["candidate"].clone()).unwrap();
    let graph = TaskGraphId::generate();
    let draft = ProofDraft {
        planning_review: None,
        draft_id: "budget-draft".into(),
        revision: 1,
        graph_id: graph.to_string(),
        compilation: compile_candidate(&contract, &candidate, &snapshot, &graph, &[]),
        contract,
        snapshot: snapshot.clone(),
        candidate: Some(candidate),
        binding_confirmations: vec![],
        status: DraftStatus::ReadyForConfirmation,
        confirmed_graph_revision: None,
        planning_usage: PlanningUsage {
            model_calls: 1,
            total_tokens: Some(100),
            started_at_ms: now() - 900_000,
            elapsed_ms: Some(30_000),
        },
    };
    db.insert_proof_draft(&draft, now()).unwrap();
    let runtime = TaskWorldRuntime::new(&db, EventHub::new(16)).unwrap();
    DraftService::new(db.clone())
        .confirm_draft(&runtime, &draft.draft_id, 1, &snapshot, now())
        .unwrap();
    let draft = db.load_proof_draft(&draft.draft_id).unwrap().unwrap();
    let meter = Arc::new(GoalBudgetMeter::new(db.clone()).unwrap());
    (db, meter, draft)
}

#[test]
fn active_budget_excludes_review_and_approval_gaps_and_counts_overlap_once() {
    let (_, meter, draft) = setup();
    assert_eq!(
        meter
            .check(&draft.contract, now() + 1_200_000)
            .unwrap()
            .as_millis(),
        570_000
    );
    let first = meter.begin(&draft, &NodeExecutionId::generate()).unwrap();
    let key = (draft.contract.goal_id.clone(), draft.contract.version);
    meter.running.lock().get_mut(&key).unwrap().started = Instant::now() - Duration::from_secs(10);
    let second = meter.begin(&draft, &NodeExecutionId::generate()).unwrap();
    first.finish().unwrap();
    second.finish().unwrap();
    let used = meter.usage(&draft.contract).unwrap().execution_active_ms;
    assert!(
        (10_000..11_000).contains(&used),
        "parallel interval was double charged: {used}"
    );
    let remaining = meter
        .check(&draft.contract, now() + 3_600_000)
        .unwrap()
        .as_millis();
    assert!(
        (559_000..=560_000).contains(&remaining),
        "approval waiting was charged: {remaining}"
    );
}

#[test]
fn new_complete_goal_with_unknown_token_usage_has_no_remaining_budget() {
    let (_, _, draft) = setup();
    let mut planning = draft.planning_usage.clone();
    planning.total_tokens = None;
    let usage = GoalTimeUsage {
        planning_elapsed_ms: Some(1),
        execution_active_ms: 0,
        human_wait_ms: None,
        accounting_state: "complete".into(),
    };

    let error = remaining_budget(&draft.contract, &planning, &usage, now() - 1, now()).unwrap_err();
    assert_eq!(error.code, "proofloop_usage_unavailable");

    let mut legacy = usage.clone();
    legacy.accounting_state = "legacy_unknown".into();
    assert!(remaining_budget(&draft.contract, &planning, &legacy, now() - 1, now(),).is_ok());
}

#[test]
fn repair_transaction_uses_activity_budget_after_long_wait() {
    let (db, meter, draft) = setup();
    let mut proposal = RepairProposal {
        schema_version: 1,
        repair_id: "budget-repair".into(),
        goal_id: draft.contract.goal_id.clone(),
        goal_version: draft.contract.version,
        expected_graph_revision: 1,
        source_report_id: "report".into(),
        failure_execution_ids: vec![],
        round: 1,
        operations: vec![RepairOperation::RerunNode {
            node_id: "compare".into(),
        }],
        rerun_root_node_id: "compare".into(),
        affected_node_ids: vec!["compare".into()],
        rationale: "test".into(),
    };
    meter
        .save_repair(&mut proposal, &draft.contract, now() + 1_200_000)
        .unwrap();
    assert_eq!(
        db.repair_rounds_used(&proposal.goal_id, proposal.goal_version)
            .unwrap(),
        1
    );
}

#[test]
fn active_restart_does_not_reset_usage_or_charge_downtime() {
    let (db, meter, draft) = setup();
    let lease = meter.begin(&draft, &NodeExecutionId::generate()).unwrap();
    let restarted = GoalBudgetMeter::new(db).unwrap();
    assert_eq!(
        restarted.check(&draft.contract, now()).unwrap_err().code,
        "proofloop_usage_unavailable"
    );
    drop(lease);
}

#[test]
fn failed_parallel_finish_can_be_explicitly_retried_without_resetting_usage() {
    let (db, meter, draft) = setup();
    let key = (draft.contract.goal_id.clone(), draft.contract.version);
    let first = meter.begin(&draft, &NodeExecutionId::generate()).unwrap();
    let second = meter.begin(&draft, &NodeExecutionId::generate()).unwrap();
    let started = meter.running.lock().get(&key).unwrap().started;
    let persisted_started = db
        .load_proofloop_time(&key.0, key.1)
        .unwrap()
        .unwrap()
        .active_started_at_ms;
    db.conn()
        .execute_batch(
            "CREATE TRIGGER fail_parallel_budget_finish
             BEFORE UPDATE ON proofloop_time_usage
             WHEN NEW.active_count = 1 AND NEW.accounting_state != 'interrupted_unknown'
             BEGIN SELECT RAISE(ABORT, 'injected budget save failure'); END;",
        )
        .unwrap();

    assert_eq!(first.finish().unwrap_err().code, "db_error");
    {
        let running = meter.running.lock();
        let activity = running.get(&key).unwrap();
        assert_eq!(activity.count, 2);
        assert_eq!(activity.unpersisted_finishes, 1);
        assert_eq!(activity.started, started);
    }
    let persisted = db.load_proofloop_time(&key.0, key.1).unwrap().unwrap();
    assert_eq!(persisted.active_count, 2);
    assert_eq!(persisted.active_started_at_ms, persisted_started);
    assert_eq!(
        meter.usage(&draft.contract).unwrap_err().code,
        "proofloop_usage_unavailable"
    );
    db.conn()
        .execute_batch("DROP TRIGGER fail_parallel_budget_finish;")
        .unwrap();
    second.finish().unwrap();
    assert!(!meter.running.lock().contains_key(&key));
    let persisted = db.load_proofloop_time(&key.0, key.1).unwrap().unwrap();
    assert_eq!(persisted.active_count, 0);
    assert_eq!(persisted.usage.accounting_state, "complete");
    assert!(meter.usage(&draft.contract).is_ok());

    let restarted = GoalBudgetMeter::new(db.clone()).unwrap();
    assert_eq!(
        restarted
            .usage(&draft.contract)
            .unwrap()
            .execution_active_ms,
        persisted.usage.execution_active_ms
    );
}

#[test]
fn failed_last_lease_drop_keeps_activity_unknown_after_restart() {
    let (db, meter, draft) = setup();
    let key = (draft.contract.goal_id.clone(), draft.contract.version);
    let lease = meter.begin(&draft, &NodeExecutionId::generate()).unwrap();
    let started = meter.running.lock().get(&key).unwrap().started;
    let persisted_started = db
        .load_proofloop_time(&key.0, key.1)
        .unwrap()
        .unwrap()
        .active_started_at_ms;
    db.conn()
        .execute_batch(
            "CREATE TRIGGER fail_last_budget_finish
             BEFORE UPDATE ON proofloop_time_usage
             WHEN NEW.active_count = 0 AND NEW.accounting_state != 'interrupted_unknown'
             BEGIN SELECT RAISE(ABORT, 'injected budget save failure'); END;",
        )
        .unwrap();

    drop(lease);
    {
        let running = meter.running.lock();
        let activity = running.get(&key).unwrap();
        assert_eq!(activity.count, 1);
        assert_eq!(activity.unpersisted_finishes, 1);
        assert_eq!(activity.started, started);
    }
    let persisted = db.load_proofloop_time(&key.0, key.1).unwrap().unwrap();
    assert_eq!(persisted.active_count, 1);
    assert_eq!(persisted.active_started_at_ms, persisted_started);
    assert_eq!(persisted.usage.execution_active_ms, 0);
    assert_eq!(
        meter.usage(&draft.contract).unwrap_err().code,
        "proofloop_usage_unavailable"
    );

    let restarted = GoalBudgetMeter::new(db.clone()).unwrap();
    let recovered = db.load_proofloop_time(&key.0, key.1).unwrap().unwrap();
    assert_eq!(recovered.active_count, 0);
    assert_eq!(recovered.usage.accounting_state, "interrupted_unknown");
    assert_eq!(
        restarted.usage(&draft.contract).unwrap().accounting_state,
        "interrupted_unknown"
    );
    assert_eq!(
        restarted.check(&draft.contract, now()).unwrap_err().code,
        "proofloop_usage_unavailable"
    );
}

#[test]
fn completed_last_lease_restarts_with_known_usage() {
    let (db, meter, draft) = setup();
    let key = (draft.contract.goal_id.clone(), draft.contract.version);
    meter
        .begin(&draft, &NodeExecutionId::generate())
        .unwrap()
        .finish()
        .unwrap();
    let persisted = db.load_proofloop_time(&key.0, key.1).unwrap().unwrap();
    assert_eq!(persisted.active_count, 0);
    assert_eq!(persisted.usage.accounting_state, "complete");

    let restarted = GoalBudgetMeter::new(db).unwrap();
    assert_eq!(
        restarted
            .usage(&draft.contract)
            .unwrap()
            .execution_active_ms,
        persisted.usage.execution_active_ms
    );
}

#[test]
fn explicit_wall_clock_still_includes_waits() {
    let (_, meter, mut draft) = setup();
    draft.contract.budget.time_basis = BudgetTimeBasis::WallClock;
    assert_eq!(
        meter.check(&draft.contract, now()).unwrap_err().code,
        "proofloop_budget_exhausted"
    );
}
