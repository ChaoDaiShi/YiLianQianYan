use crate::safety::{
    describe_builtin_tool, BuiltInRole, DescriptorError, GrantMode, PermissionId, PolicyDecision,
    PolicyEngine, ResourceDescriptor, ResourceScope, RolePolicy, SideEffectKind,
    ToolSecurityDescriptor,
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

// ── MCP security descriptor ──

fn mcp_descriptor(
    tool_name: &str,
    server_id: &str,
    remote_tool: &str,
    permission: PermissionId,
    scope: ResourceScope,
    risk: RiskLevel,
    side_effects: Vec<SideEffectKind>,
) -> ToolSecurityDescriptor {
    ToolSecurityDescriptor {
        tool_name: tool_name.to_string(),
        requested_permissions: vec![permission.in_scope(scope)],
        resources: vec![ResourceDescriptor::Mcp {
            server_id: server_id.to_string(),
            tool_name: remote_tool.to_string(),
        }],
        default_risk: risk,
        side_effects,
    }
}

fn valid_mcp_descriptor() -> ToolSecurityDescriptor {
    mcp_descriptor(
        "mcp_550e8400_read_file",
        "550e8400-e29b-41d4-a716-446655440000",
        "read-file",
        PermissionId::McpInvoke,
        ResourceScope::McpServer,
        RiskLevel::High,
        vec![SideEffectKind::ExternalService],
    )
}

#[test]
fn mcp_descriptor_validates() {
    valid_mcp_descriptor()
        .validate_for_tool("mcp_550e8400_read_file")
        .unwrap();
}

#[test]
fn mcp_descriptor_rejects_empty_server_id() {
    let desc = mcp_descriptor(
        "mcp_550e8400_read_file",
        "",
        "read-file",
        PermissionId::McpInvoke,
        ResourceScope::McpServer,
        RiskLevel::High,
        vec![SideEffectKind::ExternalService],
    );
    assert!(matches!(
        desc.validate(),
        Err(DescriptorError::InvalidDescriptor(_))
    ));
}

#[test]
fn mcp_descriptor_rejects_empty_remote_tool_name() {
    let desc = mcp_descriptor(
        "mcp_550e8400_read_file",
        "550e8400-e29b-41d4-a716-446655440000",
        "",
        PermissionId::McpInvoke,
        ResourceScope::McpServer,
        RiskLevel::High,
        vec![SideEffectKind::ExternalService],
    );
    assert!(matches!(
        desc.validate(),
        Err(DescriptorError::InvalidDescriptor(_))
    ));
}

#[test]
fn mcp_descriptor_rejects_wrong_permission() {
    let desc = mcp_descriptor(
        "mcp_550e8400_read_file",
        "550e8400-e29b-41d4-a716-446655440000",
        "read-file",
        PermissionId::FilesystemRead,
        ResourceScope::McpServer,
        RiskLevel::High,
        vec![SideEffectKind::ExternalService],
    );
    assert!(matches!(
        desc.validate(),
        Err(DescriptorError::InvalidDescriptor(_))
    ));
}

#[test]
fn mcp_descriptor_rejects_low_risk() {
    let desc = mcp_descriptor(
        "mcp_550e8400_read_file",
        "550e8400-e29b-41d4-a716-446655440000",
        "read-file",
        PermissionId::McpInvoke,
        ResourceScope::McpServer,
        RiskLevel::Low,
        vec![SideEffectKind::ExternalService],
    );
    assert!(matches!(
        desc.validate(),
        Err(DescriptorError::InvalidDescriptor(_))
    ));
}

#[test]
fn mcp_descriptor_requires_external_service_side_effect() {
    let desc = mcp_descriptor(
        "mcp_550e8400_read_file",
        "550e8400-e29b-41d4-a716-446655440000",
        "read-file",
        PermissionId::McpInvoke,
        ResourceScope::McpServer,
        RiskLevel::High,
        vec![],
    );
    assert!(matches!(
        desc.validate(),
        Err(DescriptorError::InvalidDescriptor(_))
    ));
}

#[test]
fn owner_and_standard_mcp_require_approval_restricted_denies() {
    use GrantMode::{Deny, RequireApproval};
    assert_eq!(
        RolePolicy::grant(BuiltInRole::Owner, PermissionId::McpInvoke),
        RequireApproval
    );
    assert_eq!(
        RolePolicy::grant(BuiltInRole::Standard, PermissionId::McpInvoke),
        RequireApproval
    );
    assert_eq!(
        RolePolicy::grant(BuiltInRole::Restricted, PermissionId::McpInvoke),
        Deny
    );
}

#[test]
fn policy_engine_mcp_owner_requires_approval() {
    let descriptor = valid_mcp_descriptor();
    let decision = PolicyEngine::evaluate(
        BuiltInRole::Owner,
        "mcp_550e8400_read_file",
        &descriptor,
        RiskLevel::High,
    );
    assert!(matches!(decision, PolicyDecision::RequireApproval(_)));
}

#[test]
fn policy_engine_mcp_standard_requires_approval() {
    let descriptor = valid_mcp_descriptor();
    let decision = PolicyEngine::evaluate(
        BuiltInRole::Standard,
        "mcp_550e8400_read_file",
        &descriptor,
        RiskLevel::High,
    );
    assert!(matches!(decision, PolicyDecision::RequireApproval(_)));
}

#[test]
fn policy_engine_mcp_restricted_denies() {
    let descriptor = valid_mcp_descriptor();
    let decision = PolicyEngine::evaluate(
        BuiltInRole::Restricted,
        "mcp_550e8400_read_file",
        &descriptor,
        RiskLevel::High,
    );
    assert!(matches!(decision, PolicyDecision::Deny(_)));
}

// ── Subagent security descriptor ──

fn subagent_descriptor(
    tool_name: &str,
    name: &str,
    permission: PermissionId,
    scope: ResourceScope,
    risk: RiskLevel,
    side_effects: Vec<SideEffectKind>,
) -> ToolSecurityDescriptor {
    ToolSecurityDescriptor {
        tool_name: tool_name.to_string(),
        requested_permissions: vec![permission.in_scope(scope)],
        resources: vec![ResourceDescriptor::Subagent {
            name: name.to_string(),
        }],
        default_risk: risk,
        side_effects,
    }
}

fn valid_subagent_descriptor() -> ToolSecurityDescriptor {
    subagent_descriptor(
        "subagent_researcher",
        "researcher",
        PermissionId::AgentDelegate,
        ResourceScope::Subagent,
        RiskLevel::High,
        vec![SideEffectKind::AgentDelegation],
    )
}

#[test]
fn subagent_descriptor_validates() {
    valid_subagent_descriptor()
        .validate_for_tool("subagent_researcher")
        .unwrap();
}

#[test]
fn subagent_descriptor_rejects_wrong_permission() {
    let desc = subagent_descriptor(
        "subagent_researcher",
        "researcher",
        PermissionId::AgentPlan,
        ResourceScope::Subagent,
        RiskLevel::High,
        vec![SideEffectKind::AgentDelegation],
    );
    assert!(matches!(
        desc.validate(),
        Err(DescriptorError::InvalidDescriptor(_))
    ));
}

#[test]
fn subagent_descriptor_rejects_wrong_scope() {
    let desc = subagent_descriptor(
        "subagent_researcher",
        "researcher",
        PermissionId::AgentDelegate,
        ResourceScope::AgentInternal,
        RiskLevel::High,
        vec![SideEffectKind::AgentDelegation],
    );
    assert!(matches!(
        desc.validate(),
        Err(DescriptorError::InvalidDescriptor(_))
    ));
}

#[test]
fn subagent_descriptor_rejects_low_risk() {
    let desc = subagent_descriptor(
        "subagent_researcher",
        "researcher",
        PermissionId::AgentDelegate,
        ResourceScope::Subagent,
        RiskLevel::Medium,
        vec![SideEffectKind::AgentDelegation],
    );
    assert!(matches!(
        desc.validate(),
        Err(DescriptorError::InvalidDescriptor(_))
    ));
}

#[test]
fn subagent_descriptor_requires_agent_delegation_side_effect() {
    let desc = subagent_descriptor(
        "subagent_researcher",
        "researcher",
        PermissionId::AgentDelegate,
        ResourceScope::Subagent,
        RiskLevel::High,
        vec![],
    );
    assert!(matches!(
        desc.validate(),
        Err(DescriptorError::InvalidDescriptor(_))
    ));
}

#[test]
fn subagent_descriptor_rejects_wrong_resource_type() {
    let mut desc = valid_subagent_descriptor();
    desc.resources = vec![ResourceDescriptor::Agent {
        action: "write_todos".to_string(),
    }];
    assert!(matches!(
        desc.validate(),
        Err(DescriptorError::InvalidDescriptor(_))
    ));
}

#[test]
fn subagent_descriptor_rejects_empty_name() {
    let desc = subagent_descriptor(
        "subagent_researcher",
        "",
        PermissionId::AgentDelegate,
        ResourceScope::Subagent,
        RiskLevel::High,
        vec![SideEffectKind::AgentDelegation],
    );
    assert!(matches!(
        desc.validate(),
        Err(DescriptorError::InvalidDescriptor(_))
    ));
}

#[test]
fn owner_and_standard_subagent_delegation_require_approval_restricted_denies() {
    use GrantMode::{Deny, RequireApproval};
    assert_eq!(
        RolePolicy::grant(BuiltInRole::Owner, PermissionId::AgentDelegate),
        RequireApproval
    );
    assert_eq!(
        RolePolicy::grant(BuiltInRole::Standard, PermissionId::AgentDelegate),
        RequireApproval
    );
    assert_eq!(
        RolePolicy::grant(BuiltInRole::Restricted, PermissionId::AgentDelegate),
        Deny
    );
}

#[test]
fn policy_engine_subagent_owner_requires_approval() {
    let descriptor = valid_subagent_descriptor();
    let decision = PolicyEngine::evaluate(
        BuiltInRole::Owner,
        "subagent_researcher",
        &descriptor,
        RiskLevel::High,
    );
    assert!(matches!(decision, PolicyDecision::RequireApproval(_)));
    assert_eq!(decision.context().policy_version, "security-rbac-v3");
}

#[test]
fn policy_engine_subagent_standard_requires_approval() {
    let descriptor = valid_subagent_descriptor();
    let decision = PolicyEngine::evaluate(
        BuiltInRole::Standard,
        "subagent_researcher",
        &descriptor,
        RiskLevel::High,
    );
    assert!(matches!(decision, PolicyDecision::RequireApproval(_)));
}

#[test]
fn policy_engine_subagent_restricted_denies() {
    let descriptor = valid_subagent_descriptor();
    let decision = PolicyEngine::evaluate(
        BuiltInRole::Restricted,
        "subagent_researcher",
        &descriptor,
        RiskLevel::High,
    );
    assert!(matches!(decision, PolicyDecision::Deny(_)));
}
