use super::{
    effective_request_timeout, parse_ai_repair_reply, validate_failure_context, FailureContext,
    ObservedValue,
};
use crate::modules::task::proofloop::contracts::{
    Budget, CapabilitySnapshot, GoalContract, RepairOperation,
};
use serde_json::json;
use std::time::Duration;

#[test]
fn repair_timeout_uses_the_smaller_activity_and_client_limit() {
    assert_eq!(
        effective_request_timeout(Duration::from_secs(1), Duration::from_secs(10)),
        Duration::from_secs(1),
    );
    assert_eq!(
        effective_request_timeout(Duration::from_secs(10), Duration::from_secs(2)),
        Duration::from_secs(2),
    );
}

fn context() -> FailureContext {
    FailureContext {
        contract: GoalContract {
            schema_version: 1,
            goal_id: "goal-1".into(),
            version: 7,
            workspace_id: "workspace-1".into(),
            user_goal: "生成报告".into(),
            deliverables: Vec::new(),
            criteria: Vec::new(),
            constraints: Vec::new(),
            allowed_capability_ids: vec!["files.write".into()],
            budget: Budget::default(),
        },
        snapshot: CapabilitySnapshot {
            snapshot_id: "snapshot-1".into(),
            captured_at_ms: 10,
            capabilities: Vec::new(),
        },
        graph_revision: 4,
        report_id: "report-1".into(),
        failed_execution_ids: vec!["execution-1".into()],
        failed_node_id: "failed-node".into(),
        failure_code: "invalid_argument".into(),
        error_message: "required value missing".into(),
        observed_values: vec![ObservedValue {
            evidence_id: "evidence-1".into(),
            execution_id: "upstream-execution".into(),
            node_id: Some("source-node".into()),
            path: vec!["structured".into(), "filename".into()],
            value: json!("report.md"),
        }],
        current_round: 0,
    }
}

#[test]
fn proposal_anchors_are_filled_by_server_and_literal_must_be_observed() {
    let response = json!({
        "operations": [{
            "kind": "replace_literal",
            "node_id": "failed-node",
            "target_path": ["filename"],
            "value": "report.md"
        }],
        "rationale": "Use the filename observed in the selected evidence."
    });

    let proposal = parse_ai_repair_reply(&response.to_string(), &context()).unwrap();

    assert_eq!(proposal.goal_id, "goal-1");
    assert_eq!(proposal.goal_version, 7);
    assert_eq!(proposal.expected_graph_revision, 4);
    assert_eq!(proposal.source_report_id, "report-1");
    assert_eq!(proposal.failure_execution_ids, vec!["execution-1"]);
    assert_eq!(proposal.rerun_root_node_id, "failed-node");
    assert!(matches!(
        &proposal.operations[0],
        RepairOperation::ReplaceLiteral { value, .. } if value == "report.md"
    ));
}

#[test]
fn proposal_rejects_literal_values_without_observed_support() {
    let response = json!({
        "operations": [{
            "kind": "replace_literal",
            "node_id": "failed-node",
            "target_path": ["filename"],
            "value": "C:/guessed/output.md"
        }],
        "rationale": "Guessing a plausible path."
    });

    assert_eq!(
        parse_ai_repair_reply(&response.to_string(), &context())
            .unwrap_err()
            .code,
        "repair_needs_input"
    );
}

#[test]
fn proposal_rejects_changes_to_a_non_failed_node() {
    let response = json!({
        "operations": [{
            "kind": "rerun_node",
            "node_id": "successful-node"
        }],
        "rationale": "Retry another branch."
    });

    assert_eq!(
        parse_ai_repair_reply(&response.to_string(), &context())
            .unwrap_err()
            .code,
        "unrelated_branch_mutation_forbidden"
    );
}

#[test]
fn uncertain_external_results_are_rejected_before_model_reservation() {
    let mut context = context();
    context.failure_code = "external_mutation_uncertain".into();
    context.error_message = "provider timeout after submit".into();

    assert_eq!(
        validate_failure_context(&context).unwrap_err().code,
        "external_mutation_uncertain"
    );

    context.failure_code = "provider_failed".into();
    context.error_message = "read-only provider request timed out".into();
    assert!(validate_failure_context(&context).is_ok());
}
