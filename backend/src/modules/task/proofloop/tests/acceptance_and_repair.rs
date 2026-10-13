use super::*;
use sha2::Digest;

// ── Line C Tests: Acceptance Evaluator ──

#[test]
fn test_line_c_acceptance_aggregation_and_checks() {
    use crate::modules::task::proofloop::acceptance::*;
    use std::collections::HashMap;

    // 1. Aggregation rules
    assert_eq!(aggregate_required(&[]), GoalStatus::Incomplete);
    assert_eq!(
        aggregate_required(&[CriterionStatus::Pass, CriterionStatus::Pass]),
        GoalStatus::Pass
    );
    assert_eq!(
        aggregate_required(&[CriterionStatus::Pass, CriterionStatus::Fail]),
        GoalStatus::Fail
    );
    assert_eq!(
        aggregate_required(&[CriterionStatus::Pass, CriterionStatus::HumanPending]),
        GoalStatus::Incomplete
    );

    // 2. Goal evaluation with deliverables and sections
    let contract = GoalContract {
        schema_version: 1,
        goal_id: "goal_1".into(),
        version: 1,
        workspace_id: "ws_default".into(),
        user_goal: "完成调研报告".into(),
        deliverables: vec![DeliverableSpec {
            id: "report_md".into(),
            title: "调研报告".into(),
            mime_type: "text/markdown".into(),
        }],
        criteria: vec![
            Criterion {
                id: "c_exists".into(),
                description: "产物存在".into(),
                required: true,
                check: CriterionCheck::ArtifactExists {
                    deliverable_id: "report_md".into(),
                },
            },
            Criterion {
                id: "c_sections".into(),
                description: "必要章节".into(),
                required: true,
                check: CriterionCheck::RequiredSections {
                    deliverable_id: "report_md".into(),
                    headings: vec!["背景".into(), "技术对比".into(), "结论".into()],
                },
            },
            Criterion {
                id: "c_review".into(),
                description: "人工确认".into(),
                required: false,
                check: CriterionCheck::HumanReview {
                    deliverable_id: "report_md".into(),
                    rubric: "确认无误".into(),
                },
            },
        ],
        constraints: vec![],
        allowed_capability_ids: vec![],
        budget: Budget::default(),
    };

    let md_text = "# 调研报告\n\n## 背景\n介绍背景。\n\n## 技术对比\n方案A与方案B。\n\n## 结论\n采用方案A。\n";
    let md_bytes = md_text.as_bytes().to_vec();
    let digest = format!("{:x}", sha2::Sha256::digest(&md_bytes));
    let selected_execution = ExecutionRef {
        graph_id: "graph_1".into(),
        graph_revision: 1,
        node_id: "report_node".into(),
        execution_id: "exec_1".into(),
    };
    let snapshot = EvidenceSnapshot {
        snapshot_id: "snap_1".into(),
        goal_id: "goal_1".into(),
        goal_version: 1,
        graph_id: "graph_1".into(),
        graph_revision: 1,
        executions: vec![selected_execution.clone()],
        evidence: vec![EvidenceRef {
            evidence_id: "ev_art_1".into(),
            origin: selected_execution,
            artifact_id: Some("art_1".into()),
            output_path: None,
            digest,
            observed_at_ms: 1000,
        }],
        deliverables: vec![DeliverableArtifactMapping {
            deliverable_id: "report_md".into(),
            artifact_id: "art_1".into(),
        }],
        sources: vec![],
    };

    let mut content_map = HashMap::new();
    content_map.insert("art_1".into(), md_bytes);

    let report = evaluate_goal(&contract, &snapshot, &content_map, &[], 2000).unwrap();
    // Required criteria pass, optional human review is HumanPending -> GoalStatus is Pass!
    assert_eq!(report.status, GoalStatus::Pass);
    assert_eq!(report.results[0].status, CriterionStatus::Pass);
    assert_eq!(report.results[1].status, CriterionStatus::Pass);
    assert_eq!(report.results[2].status, CriterionStatus::HumanPending);

    // Negative case: missing a required section -> Goal fails
    let md_missing = "# 调研报告\n\n## 背景\n介绍。\n";
    content_map.insert("art_1".into(), md_missing.as_bytes().to_vec());
    let report_missing = evaluate_goal(&contract, &snapshot, &content_map, &[], 2000).unwrap();
    assert_eq!(report_missing.status, GoalStatus::Fail);
    assert_eq!(report_missing.results[1].status, CriterionStatus::Fail);
}

// ── Line D Tests: Controlled Repair ──

#[test]
fn test_line_d_controlled_repair_validation_and_branch_isolation() {
    use crate::modules::task::proofloop::repair::*;
    use crate::modules::task::task_graph::{
        GraphRevision, TaskEdge, TaskGraph, TaskNode, TaskNodeId, TaskNodeKind,
    };

    // Build a two-node graph: node_a -> node_b, plus unrelated independent node_c
    let node_a = TaskNode::new(
        TaskNodeId::new("node_a").unwrap(),
        TaskNodeKind::Work,
        "A",
        serde_json::json!({
            "capability_input": { "text": "hello" },
            "capability_bindings": []
        }),
    )
    .unwrap();

    let node_b = TaskNode::new(
        TaskNodeId::new("node_b").unwrap(),
        TaskNodeKind::Work,
        "B",
        serde_json::json!({
            "capability_input": {},
            "capability_bindings": []
        }),
    )
    .unwrap();

    let node_c = TaskNode::new(
        TaskNodeId::new("node_c").unwrap(),
        TaskNodeKind::Work,
        "C",
        serde_json::json!({
            "capability_input": {},
            "capability_bindings": []
        }),
    )
    .unwrap();

    let graph = TaskGraph::new(
        TaskGraphId::new("test_graph").unwrap(),
        GraphRevision::initial(),
        vec![node_a, node_b, node_c],
        vec![TaskEdge::new(
            TaskNodeId::new("node_a").unwrap(),
            TaskNodeId::new("node_b").unwrap(),
        )],
    )
    .unwrap();

    let contract = GoalContract {
        schema_version: 1,
        goal_id: "test_goal".into(),
        version: 1,
        workspace_id: "ws".into(),
        user_goal: "test".into(),
        deliverables: vec![],
        criteria: vec![],
        constraints: vec![],
        allowed_capability_ids: vec![],
        budget: Budget::default(),
    };

    let snapshot = CapabilitySnapshot {
        snapshot_id: "s1".into(),
        captured_at_ms: 1000,
        capabilities: vec![],
    };

    // Valid repair targeting node_a (downstream branch includes node_a and node_b)
    let valid_proposal = RepairProposal {
        schema_version: 1,
        repair_id: "r1".into(),
        goal_id: "test_goal".into(),
        goal_version: 1,
        expected_graph_revision: 1,
        source_report_id: "rep1".into(),
        failure_execution_ids: vec!["exec_a".into()],
        round: 1,
        operations: vec![RepairOperation::ReplaceLiteral {
            node_id: "node_a".into(),
            target_path: vec!["text".into()],
            value: serde_json::json!("new_text"),
        }],
        rerun_root_node_id: "node_a".into(),
        affected_node_ids: vec!["node_a".into(), "node_b".into()],
        rationale: "Fix literal".into(),
    };

    let validated = validate_repair(&valid_proposal, &graph, 1, &snapshot, &contract).unwrap();
    assert_eq!(validated.affected_node_ids, vec!["node_a", "node_b"]);

    let (applied_graph, new_rev, affected) = apply_repair(validated, 1).unwrap();
    assert_eq!(new_rev, 2);
    assert_eq!(applied_graph.revision.value(), 2);
    assert_eq!(affected, vec!["node_a", "node_b"]);

    // Invalid repair: attempts to mutate unrelated node_c when root is node_a
    let invalid_proposal = RepairProposal {
        operations: vec![RepairOperation::ReplaceLiteral {
            node_id: "node_c".into(), // UNRELATED BRANCH
            target_path: vec!["text".into()],
            value: serde_json::json!("hack"),
        }],
        ..valid_proposal.clone()
    };

    let err_unrelated =
        validate_repair(&invalid_proposal, &graph, 1, &snapshot, &contract).unwrap_err();
    assert_eq!(err_unrelated.code, "unrelated_branch_mutation_forbidden");

    // Invalid repair: stale revision
    let err_stale = validate_repair(&valid_proposal, &graph, 2, &snapshot, &contract).unwrap_err();
    assert_eq!(err_stale.code, "proofloop_stale_revision");

    // Invalid repair: budget exhausted
    let mut exhausted_proposal = valid_proposal;
    exhausted_proposal.round = 3; // default max is 2
    let err_budget =
        validate_repair(&exhausted_proposal, &graph, 1, &snapshot, &contract).unwrap_err();
    assert_eq!(err_budget.code, "proofloop_budget_exhausted");
}
