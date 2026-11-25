use super::approvals::approved_execution_stop_reason;
use crate::{
    agent::{
        delegation::ExecutionScope, engine::ExternalToolAccounting, verifier::VerificationResult,
    },
    execution::ExecutionOwner,
    tools::ToolResult,
};
fn scope(personal: bool) -> ExecutionScope {
    ExecutionScope {
        owner: personal.then(|| ExecutionOwner::PersonalDelegation {
            automation_id: "delegation".into(),
            occurrence_id: "occurrence".into(),
            conversation_id: "chat".into(),
        }),
        cancel: tokio_util::sync::CancellationToken::new(),
        budget: None,
        handoff_id: None,
        goal_id: None,
        goal_version: None,
        evidence_ids: vec![],
        deadline: None,
    }
}
#[test]
fn integration_approved_unknown_result_stops_even_without_external_tool_accounting() {
    let result = ToolResult::success_with_structured(
        "late result",
        Some(serde_json::json!({"status":"result_unknown"})),
    );
    assert!(approved_execution_stop_reason(
        &scope(false),
        &result,
        &VerificationResult::success("ok", None),
        &Ok(ExternalToolAccounting::NotExternal)
    )
    .is_some());
}
#[test]
fn integration_approved_personal_owner_preserves_strict_billing_guard() {
    let result = ToolResult::success("done");
    let v = VerificationResult::success("ok", None);
    assert!(approved_execution_stop_reason(
        &scope(true),
        &result,
        &v,
        &Ok(ExternalToolAccounting::BillingUnknown)
    )
    .is_some());
    assert!(approved_execution_stop_reason(
        &scope(false),
        &result,
        &v,
        &Ok(ExternalToolAccounting::BillingUnknown)
    )
    .is_none());
    assert!(approved_execution_stop_reason(
        &scope(true),
        &result,
        &v,
        &Ok(ExternalToolAccounting::Known)
    )
    .is_none());
}

#[tokio::test]
async fn integration_audit_uses_typed_owner_without_reinterpreting_legacy_conversation() {
    use crate::safety::{AuditEventInput, AuditEventType, AuditRecorder};
    let db = crate::db::Database::new(std::path::Path::new(":memory:")).unwrap();
    let recorder = AuditRecorder::new(db);
    let owner = ExecutionOwner::Workflow {
        workflow_graph_id: "same".into(),
        run_id: "run".into(),
    };
    let event = crate::execution::owner::CURRENT_EXECUTION_OWNER
        .scope(owner, async {
            recorder
                .record(AuditEventInput {
                    event_type: AuditEventType::ExecutionStarted,
                    correlation_id: "call".into(),
                    request_id: "call".into(),
                    subject_id: "local-user".into(),
                    role_key: "owner".into(),
                    conversation_id: Some("legacy-execution-id".into()),
                    ..Default::default()
                })
                .unwrap()
        })
        .await;
    assert_eq!(
        event.conversation_id.as_deref(),
        Some("legacy-execution-id")
    );
    assert_eq!(
        event.details["context"]["execution_owner"]["kind"],
        "workflow"
    );
    assert_eq!(
        event.details["context"]["execution_owner"]["workflow_graph_id"],
        "same"
    );
}
#[test]
fn integration_capability_unknown_preserves_non_replayable_failure_code() {
    let result = ToolResult::success_with_structured(
        "unknown effect",
        Some(serde_json::json!({"state":"result_unknown"})),
    );
    let outcome = crate::safety::execution_gateway::SecurityExecutionOutcome::Executed {
        tool_result: result,
        verification: VerificationResult::success("late observation", None),
    };
    let dispatch = crate::modules::task::application::capability_execution::completed_result(
        "fixture", outcome,
    )
    .unwrap();
    let crate::modules::task::ExecutorDispatch::Failed { code, .. } = dispatch else {
        panic!("unknown result must fail")
    };
    assert!(crate::modules::task::execution::external_result_is_uncertain(Some(&code)));
}
