use yilian_backend::modules::task::proofloop::{
    contracts::{
        Budget, CandidateNode, CandidatePlan, CapabilityKindSpec, CapabilitySnapshot,
        CapabilitySpec, CompileResult, CompileStatus, Criterion, CriterionCheck,
        DeliverableNodeMapping, DeliverableSpec, GoalContract, ProofError,
    },
    enhancement_contracts::{PlanOption, PlanScore},
    plan_selection::{score_candidate, select_recommended, weighted_total},
};

fn contract() -> GoalContract {
    GoalContract {
        schema_version: 1,
        goal_id: "goal-1".into(),
        version: 1,
        workspace_id: "workspace-1".into(),
        user_goal: "Create report".into(),
        deliverables: vec![DeliverableSpec {
            id: "report".into(),
            title: "report.md".into(),
            mime_type: "text/markdown".into(),
        }],
        criteria: vec![
            Criterion {
                id: "exists".into(),
                description: "Report exists".into(),
                required: true,
                check: CriterionCheck::ArtifactExists {
                    deliverable_id: "report".into(),
                },
            },
            Criterion {
                id: "review".into(),
                description: "Human reviews report".into(),
                required: true,
                check: CriterionCheck::HumanReview {
                    deliverable_id: "report".into(),
                    rubric: "Readable".into(),
                },
            },
        ],
        constraints: Vec::new(),
        allowed_capability_ids: vec!["mcp.report".into()],
        budget: Budget::default(),
    }
}

fn candidate() -> CandidatePlan {
    CandidatePlan {
        schema_version: 1,
        nodes: vec![CandidateNode {
            id: "n1".into(),
            title: "Write report".into(),
            instruction: "Write a report".into(),
            capability_id: "mcp.report".into(),
            literal_input: serde_json::Map::new(),
            bindings: Vec::new(),
        }],
        edges: Vec::new(),
        deliverable_nodes: vec![DeliverableNodeMapping {
            deliverable_id: "report".into(),
            node_id: "n1".into(),
        }],
    }
}

fn snapshot() -> CapabilitySnapshot {
    CapabilitySnapshot {
        snapshot_id: "snapshot-1".into(),
        captured_at_ms: 1,
        capabilities: vec![CapabilitySpec {
            capability_id: "mcp.report".into(),
            executor_ref: "capability://mcp.report".into(),
            kind: CapabilityKindSpec::McpTool,
            name: "Report".into(),
            description: "Create report".into(),
            input_schema: None,
            output_schema: None,
            output_path_prefix: vec!["structured".into()],
            output_provenance: "declared".into(),
            risk: "high".into(),
            readiness: "ready".into(),
            unavailable_reason: None,
        }],
    }
}

fn option(id: &str, total: i64, status: CompileStatus, rejected_codes: Vec<String>) -> PlanOption {
    PlanOption {
        candidate_id: id.into(),
        candidate: candidate(),
        compilation: CompileResult {
            status,
            graph: None,
            diagnostics: Vec::<ProofError>::new(),
            pending_inputs: Vec::new(),
        },
        score: PlanScore {
            coverage_bp: 10_000,
            type_bp: 10_000,
            evidence_bp: 5_000,
            risk_bp: 6_667,
            cost_bp: 500,
            cost_known: false,
            total,
            rejected_codes,
        },
    }
}

#[test]
fn frozen_plan_weights_are_reproducible() {
    assert_eq!(
        weighted_total(10_000, 10_000, 10_000, 6_667, 1_000),
        639_995
    );
}

#[test]
fn candidate_score_uses_contract_compile_and_snapshot_inputs() {
    let contract = contract();
    let candidate = candidate();
    let compilation = CompileResult {
        status: CompileStatus::Ready,
        graph: None,
        diagnostics: Vec::new(),
        pending_inputs: Vec::new(),
    };

    let score = score_candidate(&contract, &candidate, &compilation, &snapshot());

    assert_eq!(score.coverage_bp, 10_000);
    assert_eq!(score.type_bp, 10_000);
    assert_eq!(score.evidence_bp, 5_000);
    assert_eq!(score.risk_bp, 6_667);
    assert_eq!(score.cost_bp, 500);
    assert!(!score.cost_known);
    assert!(score.rejected_codes.is_empty());
}

#[test]
fn only_ready_unrejected_candidate_can_be_recommended() {
    let options = vec![
        option("a-invalid", 900_000, CompileStatus::Invalid, Vec::new()),
        option(
            "b-rejected",
            800_000,
            CompileStatus::Ready,
            vec!["denied".into()],
        ),
        option("c-ready", 700_000, CompileStatus::Ready, Vec::new()),
        option("d-ready", 700_000, CompileStatus::Ready, Vec::new()),
    ];

    assert_eq!(select_recommended(&options).as_deref(), Some("c-ready"));
}
