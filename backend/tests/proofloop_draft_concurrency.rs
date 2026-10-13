use serde_json::Value;
use std::path::Path;
use yilian_backend::db::{Database, DraftWriteConflict, ProofLoopDbError};
use yilian_backend::modules::task::proofloop::{
    compile_candidate, CandidatePlan, CapabilitySnapshot, CompileStatus, DraftService, DraftStatus,
    GoalContract, PlanningUsage, ProofDraft,
};
use yilian_backend::modules::task::runtime::TaskWorldRuntime;
use yilian_backend::modules::task::TaskGraphId;
use yilian_backend::shared::event::EventHub;

const NOW: i64 = 1_797_000_000_000;

fn fixture() -> (GoalContract, CapabilitySnapshot, CandidatePlan) {
    let raw: Value =
        serde_json::from_str(include_str!("fixtures/proofloop/declared-two-node.json")).unwrap();
    (
        serde_json::from_value(raw["contract"].clone()).unwrap(),
        serde_json::from_value(raw["snapshot"].clone()).unwrap(),
        serde_json::from_value(raw["candidate"].clone()).unwrap(),
    )
}

fn draft(
    draft_id: &str,
    contract: &GoalContract,
    snapshot: &CapabilitySnapshot,
    candidate: &CandidatePlan,
) -> ProofDraft {
    let graph_id = TaskGraphId::generate();
    let compilation = compile_candidate(contract, candidate, snapshot, &graph_id, &[]);
    assert_eq!(compilation.status, CompileStatus::Ready);
    ProofDraft {
        draft_id: draft_id.into(),
        revision: 1,
        graph_id: graph_id.to_string(),
        contract: contract.clone(),
        snapshot: snapshot.clone(),
        candidate: Some(candidate.clone()),
        binding_confirmations: Vec::new(),
        compilation,
        status: DraftStatus::ReadyForConfirmation,
        confirmed_graph_revision: None,
        planning_usage: PlanningUsage::default(),
    }
}

#[test]
fn stale_compare_and_swap_cannot_overwrite_a_confirmed_draft() {
    let db = Database::new(Path::new(":memory:")).unwrap();
    let (contract, snapshot, candidate) = fixture();
    let stale_draft = draft(
        "draft-concurrency-confirmed",
        &contract,
        &snapshot,
        &candidate,
    );
    db.insert_proof_draft(&stale_draft, NOW).unwrap();

    let runtime = TaskWorldRuntime::new(&db, EventHub::new(16)).unwrap();
    let confirmed = DraftService::new(db.clone())
        .confirm_draft(&runtime, &stale_draft.draft_id, 1, &snapshot, NOW)
        .unwrap();

    let mut stale_update = stale_draft.clone();
    stale_update.revision = 2;
    stale_update.contract.user_goal = "stale edit".into();
    let stale_cas = db.update_proof_draft_if_current(1, &stale_update, NOW + 2);

    assert!(matches!(
        stale_cas,
        Err(ProofLoopDbError::DraftConflict(
            DraftWriteConflict::AlreadyConfirmed
        ))
    ));
    let persisted = db.load_proof_draft(&stale_draft.draft_id).unwrap().unwrap();
    assert_eq!(persisted.status, DraftStatus::Confirmed);
    assert_eq!(
        persisted.confirmed_graph_revision,
        Some(confirmed.graph_revision)
    );
    assert!(runtime
        .get_graph(&TaskGraphId::new(&stale_draft.graph_id).unwrap())
        .is_some());
}

#[test]
fn compare_and_swap_rejects_the_second_writer_at_the_database_boundary() {
    let db = Database::new(Path::new(":memory:")).unwrap();
    let (contract, snapshot, candidate) = fixture();
    let original = draft("draft-concurrency-stale", &contract, &snapshot, &candidate);
    db.insert_proof_draft(&original, NOW).unwrap();

    let mut winner = original.clone();
    winner.revision = 2;
    winner.contract.user_goal = "winning revision".into();
    db.update_proof_draft_if_current(1, &winner, NOW + 1)
        .unwrap();

    let mut stale = original.clone();
    stale.revision = 2;
    stale.contract.user_goal = "stale revision".into();
    let error = db
        .update_proof_draft_if_current(1, &stale, NOW + 2)
        .unwrap_err();

    assert!(matches!(
        error,
        ProofLoopDbError::DraftConflict(DraftWriteConflict::StaleRevision {
            expected: 1,
            actual: 2,
        })
    ));
    let persisted = db.load_proof_draft(&original.draft_id).unwrap().unwrap();
    assert_eq!(persisted.revision, 2);
    assert_eq!(persisted.contract.user_goal, "winning revision");
}

#[test]
fn failed_confirmation_write_error_is_returned_without_advancing_the_draft() {
    let db = Database::new(Path::new(":memory:")).unwrap();
    let (contract, snapshot, candidate) = fixture();
    let initial = draft(
        "draft-confirmation-write-failure",
        &contract,
        &snapshot,
        &candidate,
    );
    db.insert_proof_draft(&initial, NOW).unwrap();
    db.conn()
        .execute_batch(
            "CREATE TRIGGER reject_draft_revision_two
             BEFORE UPDATE ON proofloop_drafts WHEN NEW.revision=2
             BEGIN SELECT RAISE(ABORT, 'injected draft write failure'); END;",
        )
        .unwrap();

    let runtime = TaskWorldRuntime::new(&db, EventHub::new(16)).unwrap();
    let unavailable_snapshot = CapabilitySnapshot {
        snapshot_id: "empty-current-capabilities".into(),
        captured_at_ms: NOW,
        capabilities: Vec::new(),
    };
    let error = DraftService::new(db.clone())
        .confirm_draft(
            &runtime,
            &initial.draft_id,
            1,
            &unavailable_snapshot,
            NOW + 1,
        )
        .unwrap_err();

    assert_eq!(error.code, "draft_save_failed");
    let persisted = db.load_proof_draft(&initial.draft_id).unwrap().unwrap();
    assert_eq!(persisted.revision, 1);
    assert_eq!(persisted.status, DraftStatus::ReadyForConfirmation);
}
