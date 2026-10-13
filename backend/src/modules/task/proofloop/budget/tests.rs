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
fn explicit_wall_clock_still_includes_waits() {
    let (_, meter, mut draft) = setup();
    draft.contract.budget.time_basis = BudgetTimeBasis::WallClock;
    assert_eq!(
        meter.check(&draft.contract, now()).unwrap_err().code,
        "proofloop_budget_exhausted"
    );
}
