use super::RepairProposalHttpResponse;
use crate::modules::task::proofloop::contracts::{PlanningUsage, RepairOperation, RepairProposal};

fn proposal() -> RepairProposal {
    RepairProposal {
        schema_version: 1,
        repair_id: "repair-1".into(),
        goal_id: "goal-1".into(),
        goal_version: 2,
        expected_graph_revision: 3,
        source_report_id: "report-1".into(),
        failure_execution_ids: vec!["execution-1".into()],
        round: 1,
        operations: vec![RepairOperation::RerunNode {
            node_id: "node-1".into(),
        }],
        rerun_root_node_id: "node-1".into(),
        affected_node_ids: vec!["node-1".into()],
        rationale: "最小重试".into(),
    }
}

#[test]
fn response_flattens_proposal_and_omits_usage_for_rule_repairs() {
    let rule = serde_json::to_value(RepairProposalHttpResponse {
        proposal: proposal(),
        ai_repair_usage: None,
    })
    .unwrap();
    assert_eq!(rule["repair_id"], "repair-1");
    assert!(rule.get("proposal").is_none());
    assert!(rule.get("ai_repair_usage").is_none());

    let ai = serde_json::to_value(RepairProposalHttpResponse {
        proposal: proposal(),
        ai_repair_usage: Some(PlanningUsage {
            model_calls: 1,
            total_tokens: Some(37),
            started_at_ms: 10,
            elapsed_ms: Some(12),
        }),
    })
    .unwrap();
    assert_eq!(ai["repair_id"], "repair-1");
    assert!(ai.get("proposal").is_none());
    assert_eq!(ai["ai_repair_usage"]["model_calls"], 1);
    assert_eq!(ai["ai_repair_usage"]["total_tokens"], 37);
    assert_eq!(ai["ai_repair_usage"]["started_at_ms"], 10);
    assert_eq!(ai["ai_repair_usage"]["elapsed_ms"], 12);
}
