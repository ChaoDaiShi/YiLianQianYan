use std::collections::HashMap;

use serde_json::Value;
use yilian_backend::modules::task::{
    proofloop::{
        compile_candidate, evaluate_goal, CandidatePlan, CapabilitySnapshot, CompileStatus,
        Criterion, CriterionCheck, CriterionStatus, EvidenceSnapshot, EvidenceSource, GoalContract,
    },
    TaskGraphId,
};

fn fixture(min_sources: u32) -> (GoalContract, CandidatePlan, CapabilitySnapshot) {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/proofloop/declared-two-node.json")).unwrap();
    let mut contract: GoalContract = serde_json::from_value(fixture["contract"].clone()).unwrap();
    contract.criteria = vec![Criterion {
        id: "sources".into(),
        description: "可追溯来源".into(),
        required: true,
        check: CriterionCheck::SourceTraceability {
            deliverable_id: contract.deliverables[0].id.clone(),
            min_sources,
        },
    }];
    (
        contract,
        serde_json::from_value(fixture["candidate"].clone()).unwrap(),
        serde_json::from_value(fixture["snapshot"].clone()).unwrap(),
    )
}

fn evidence(contract: &GoalContract) -> EvidenceSnapshot {
    EvidenceSnapshot {
        snapshot_id: "source-threshold-snapshot".into(),
        goal_id: contract.goal_id.clone(),
        goal_version: contract.version,
        graph_id: "source-threshold-graph".into(),
        graph_revision: 1,
        executions: vec![],
        evidence: vec![],
        deliverables: vec![],
        sources: vec![],
    }
}

#[test]
fn zero_source_threshold_cannot_compile_a_confirmable_plan() {
    let (contract, candidate, snapshot) = fixture(0);
    let graph_id = TaskGraphId::generate();
    let compilation = compile_candidate(&contract, &candidate, &snapshot, &graph_id, &[]);
    assert_eq!(compilation.status, CompileStatus::Invalid);
    assert!(compilation
        .diagnostics
        .iter()
        .any(|error| error.code == "proofloop_invalid_source_threshold"));

    let (positive, candidate, snapshot) = fixture(1);
    assert_eq!(
        compile_candidate(&positive, &candidate, &snapshot, &graph_id, &[]).status,
        CompileStatus::Ready
    );
}

#[test]
fn zero_source_threshold_in_a_stored_contract_requires_correction() {
    let (contract, _, _) = fixture(0);
    let error = evaluate_goal(&contract, &evidence(&contract), &HashMap::new(), &[], 1000)
        .expect_err("zero threshold must never certify absent sources");
    assert_eq!(error.code, "proofloop_invalid_source_threshold");
}

#[test]
fn positive_source_threshold_requires_the_declared_number_of_distinct_sources() {
    let (contract, _, _) = fixture(2);
    let mut snapshot = evidence(&contract);
    assert_eq!(
        evaluate_goal(&contract, &snapshot, &HashMap::new(), &[], 1000)
            .unwrap()
            .results[0]
            .status,
        CriterionStatus::Fail
    );
    let source = EvidenceSource {
        source_id: "source-1".into(),
        evidence_id: "evidence-1".into(),
        uri: "https://example.com/one".into(),
        retrieved_at_ms: 900,
        cited_by_deliverable_ids: vec![contract.deliverables[0].id.clone()],
    };
    snapshot.sources = vec![source.clone(), source.clone()];
    assert_eq!(
        evaluate_goal(&contract, &snapshot, &HashMap::new(), &[], 1000)
            .unwrap()
            .results[0]
            .status,
        CriterionStatus::Fail
    );
    snapshot.sources.push(EvidenceSource {
        source_id: "source-2".into(),
        evidence_id: "evidence-2".into(),
        uri: "https://example.com/two".into(),
        ..source
    });
    assert_eq!(
        evaluate_goal(&contract, &snapshot, &HashMap::new(), &[], 1000)
            .unwrap()
            .results[0]
            .status,
        CriterionStatus::Pass
    );
}

#[tokio::test]
async fn planning_never_silently_drops_an_explicitly_selected_capability() {
    use tokio_util::sync::CancellationToken;
    use yilian_backend::{
        modules::task::proofloop::{CreateProofDraftRequest, DraftService},
        Database,
    };

    let (contract, _, mut snapshot) = fixture(1);
    let omitted_id = snapshot.capabilities.remove(0).capability_id;
    let service = DraftService::new(Database::new(std::path::Path::new(":memory:")).unwrap());
    let error = service
        .create_draft(
            CreateProofDraftRequest {
                planning: Default::default(),
                workspace_id: contract.workspace_id,
                user_goal: contract.user_goal,
                allowed_capability_ids: Some(vec![omitted_id]),
            },
            snapshot,
            None,
            &CancellationToken::new(),
            1000,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "proofloop_capability_selection_incomplete");
}
