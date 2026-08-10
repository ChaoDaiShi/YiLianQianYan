use crate::safety::{
    describe_builtin_tool, BuiltInRole, PolicyDecision, PolicyEngine, ToolSecurityDescriptor,
};
use crate::tools::trait_def::RiskLevel;

#[test]
fn standard_workspace_write_is_allowed_at_medium_risk() {
    let descriptor = describe_builtin_tool(
        "write_file",
        &serde_json::json!({"path": "notes.txt", "content": "hello"}),
    )
    .unwrap();

    assert!(matches!(
        PolicyEngine::evaluate(BuiltInRole::Standard, &descriptor, RiskLevel::Medium),
        PolicyDecision::Allow(_)
    ));
}

#[test]
fn restricted_workspace_write_is_denied() {
    let descriptor = describe_builtin_tool(
        "write_file",
        &serde_json::json!({"path": "notes.txt", "content": "hello"}),
    )
    .unwrap();

    assert!(matches!(
        PolicyEngine::evaluate(BuiltInRole::Restricted, &descriptor, RiskLevel::Medium),
        PolicyDecision::Deny(_)
    ));
}

#[test]
fn owner_still_requires_approval_for_high_risk() {
    let descriptor = describe_builtin_tool(
        "bash",
        &serde_json::json!({"command": "git push origin develop"}),
    )
    .unwrap();

    assert!(matches!(
        PolicyEngine::evaluate(BuiltInRole::Owner, &descriptor, RiskLevel::High),
        PolicyDecision::RequireApproval(_)
    ));
}

#[test]
fn standard_desktop_interaction_requires_approval_even_at_medium_risk() {
    let descriptor = describe_builtin_tool(
        "mouse",
        &serde_json::json!({"action": "click", "x": 10, "y": 20}),
    )
    .unwrap();

    assert!(matches!(
        PolicyEngine::evaluate(BuiltInRole::Standard, &descriptor, descriptor.default_risk,),
        PolicyDecision::RequireApproval(_)
    ));
}

#[test]
fn any_denied_permission_denies_a_multi_permission_tool() {
    let descriptor =
        describe_builtin_tool("upscale_image", &serde_json::json!({"path": "image.png"})).unwrap();

    assert!(matches!(
        PolicyEngine::evaluate(BuiltInRole::Restricted, &descriptor, RiskLevel::Medium),
        PolicyDecision::Deny(_)
    ));
}

#[test]
fn invalid_empty_descriptor_fails_closed() {
    let descriptor = ToolSecurityDescriptor {
        tool_name: "broken".to_string(),
        requested_permissions: vec![],
        resources: vec![],
        default_risk: RiskLevel::Low,
        side_effects: vec![],
    };

    assert!(matches!(
        PolicyEngine::evaluate(BuiltInRole::Owner, &descriptor, RiskLevel::Low),
        PolicyDecision::Deny(_)
    ));
}
