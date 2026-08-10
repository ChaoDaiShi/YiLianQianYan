use crate::safety::{
    describe_builtin_tool, BuiltInRole, PermissionId, PolicyDecision, PolicyEngine,
    ResourceDescriptor, ResourceScope, ToolSecurityDescriptor,
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
        PolicyEngine::evaluate(
            BuiltInRole::Standard,
            "write_file",
            &descriptor,
            RiskLevel::Medium,
        ),
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
        PolicyEngine::evaluate(
            BuiltInRole::Restricted,
            "write_file",
            &descriptor,
            RiskLevel::Medium,
        ),
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
        PolicyEngine::evaluate(BuiltInRole::Owner, "bash", &descriptor, RiskLevel::High),
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
        PolicyEngine::evaluate(
            BuiltInRole::Standard,
            "mouse",
            &descriptor,
            descriptor.default_risk,
        ),
        PolicyDecision::RequireApproval(_)
    ));
}

#[test]
fn any_denied_permission_denies_a_multi_permission_tool() {
    let descriptor =
        describe_builtin_tool("upscale_image", &serde_json::json!({"path": "image.png"})).unwrap();

    assert!(matches!(
        PolicyEngine::evaluate(
            BuiltInRole::Restricted,
            "upscale_image",
            &descriptor,
            RiskLevel::Medium,
        ),
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
        PolicyEngine::evaluate(BuiltInRole::Owner, "broken", &descriptor, RiskLevel::Low),
        PolicyDecision::Deny(_)
    ));
}

#[test]
fn evaluated_risk_cannot_drop_below_the_descriptor_default() {
    let descriptor = describe_builtin_tool(
        "mouse",
        &serde_json::json!({"action": "click", "x": 10, "y": 20}),
    )
    .unwrap();

    let decision = PolicyEngine::evaluate(BuiltInRole::Owner, "mouse", &descriptor, RiskLevel::Low);

    assert!(matches!(decision, PolicyDecision::RequireApproval(_)));
    assert_eq!(decision.context().risk_level, RiskLevel::High);
}

#[test]
fn unknown_but_nonempty_descriptor_fails_closed() {
    let descriptor = ToolSecurityDescriptor {
        tool_name: "third_party_tool".to_string(),
        requested_permissions: vec![PermissionId::FilesystemRead.in_scope(ResourceScope::Workspace)],
        resources: vec![ResourceDescriptor::File {
            path: "README.md".to_string(),
        }],
        default_risk: RiskLevel::Low,
        side_effects: vec![],
    };

    assert!(matches!(
        PolicyEngine::evaluate(
            BuiltInRole::Owner,
            "third_party_tool",
            &descriptor,
            RiskLevel::Low,
        ),
        PolicyDecision::Deny(_)
    ));
}

#[test]
fn policy_engine_rejects_descriptor_for_a_different_tool() {
    let descriptor = describe_builtin_tool(
        "write_file",
        &serde_json::json!({"path": "notes.txt", "content": "sample"}),
    )
    .unwrap();

    assert!(matches!(
        PolicyEngine::evaluate(
            BuiltInRole::Owner,
            "read_file",
            &descriptor,
            RiskLevel::Medium,
        ),
        PolicyDecision::Deny(_)
    ));
}
