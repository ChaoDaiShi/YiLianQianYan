use yilian_backend::modules::task::{
    proofloop::{
        compile_candidate, CandidatePlan, CapabilitySnapshot, CreateProofDraftRequest,
        GoalContract, PlanningStrategy,
    },
    TaskGraphId,
};

#[test]
fn frozen_v12_fixtures_use_the_real_compiler_and_legacy_request_defaults() {
    for bytes in [
        include_str!("fixtures/proofloop/v12/planning-existing.json"),
        include_str!("fixtures/proofloop/v12/planning-ranked.json"),
        include_str!("fixtures/proofloop/v12/planning-all-invalid.json"),
        include_str!("fixtures/proofloop/v12/planning-needs-input.json"),
    ] {
        let fixture: serde_json::Value = serde_json::from_str(bytes).unwrap();
        let request: CreateProofDraftRequest =
            serde_json::from_value(fixture["request"].clone()).unwrap();
        request.planning.validate().unwrap();
        let contract: GoalContract = serde_json::from_value(fixture["contract"].clone()).unwrap();
        let snapshot: CapabilitySnapshot =
            serde_json::from_value(fixture["snapshot"].clone()).unwrap();
        let graph = TaskGraphId::generate();
        for (index, candidate) in fixture["candidates"].as_array().unwrap().iter().enumerate() {
            let candidate: CandidatePlan = serde_json::from_value(candidate.clone()).unwrap();
            let compiled = compile_candidate(&contract, &candidate, &snapshot, &graph, &[]);
            assert_eq!(
                serde_json::to_value(compiled.status).unwrap(),
                fixture["expected_compile_statuses"][index]
            );
        }
    }
    let mut fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/proofloop/v12/planning-existing.json"
    ))
    .unwrap();
    let request = fixture["request"].as_object_mut().unwrap();
    request.remove("planning");
    request.remove("requirements");
    let request: CreateProofDraftRequest =
        serde_json::from_value(serde_json::Value::Object(request.clone())).unwrap();
    assert_eq!(request.planning.strategy, PlanningStrategy::Existing);
    assert_eq!(request.planning.candidate_count, 1);
    assert!(request.requirements.is_none());
}
