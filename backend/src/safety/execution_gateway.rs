use serde_json::Value;

use crate::tools::trait_def::RiskLevel;

use super::{
    describe_builtin_tool, BuiltInRole, DecisionContext, DescriptorError, PolicyDecision,
    PolicyEngine, SafetyPolicy, ToolSecurityDescriptor, POLICY_VERSION,
};

#[derive(Debug)]
pub struct SecurityExecutionRequest {
    pub conversation_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

pub struct SecurityExecutionGateway {
    policy_engine: PolicyEngine,
}

impl SecurityExecutionGateway {
    pub fn new() -> Self {
        Self {
            policy_engine: PolicyEngine,
        }
    }

    pub fn resolve_descriptor(
        &self,
        request: &SecurityExecutionRequest,
    ) -> Result<ToolSecurityDescriptor, DescriptorError> {
        let descriptor = describe_builtin_tool(&request.tool_name, &request.arguments)?;
        descriptor.validate_for_tool(&request.tool_name)?;
        Ok(descriptor)
    }

    fn build_decision_context(
        &self,
        request: &SecurityExecutionRequest,
        descriptor: &ToolSecurityDescriptor,
        role: BuiltInRole,
        risk_level: RiskLevel,
    ) -> Result<DecisionContext, DescriptorError> {
        descriptor.validate_for_tool(&request.tool_name)?;

        let requested_permissions = descriptor
            .requested_permissions
            .iter()
            .map(|requested| requested.permission)
            .collect::<Vec<_>>();
        let resource_scopes = descriptor
            .requested_permissions
            .iter()
            .map(|requested| requested.scope)
            .collect::<Vec<_>>();

        if requested_permissions.is_empty() || resource_scopes.is_empty() {
            return Err(DescriptorError::InvalidDescriptor(
                "security descriptor must declare permissions and resource scopes".to_string(),
            ));
        }

        Ok(DecisionContext {
            role,
            risk_level,
            policy_version: POLICY_VERSION.to_string(),
            requested_permissions,
            resource_scopes,
            reason: format!("tool {} requested security evaluation", request.tool_name),
        })
    }

    pub fn evaluate(
        &self,
        request: &SecurityExecutionRequest,
        role: BuiltInRole,
        final_risk: RiskLevel,
    ) -> Result<PolicyDecision, DescriptorError> {
        let descriptor = self.resolve_descriptor(request)?;
        let assessed_risk = SafetyPolicy::assess(
            &request.tool_name,
            descriptor.default_risk,
            &request.arguments,
        );
        let context = self.build_decision_context(request, &descriptor, role, assessed_risk)?;

        // PolicyEngine currently accepts raw role/descriptor/risk inputs and
        // rebuilds its own DecisionContext. Until that API accepts a context
        // directly, this gateway validates the canonical context first and
        // forwards its role and risk through the existing policy boundary.
        // PolicyEngine currently exposes a static evaluation API; retain it as
        // the gateway's explicit policy dependency while forwarding to that API.
        let _policy_engine = &self.policy_engine;
        Ok(PolicyEngine::evaluate(
            context.role,
            &request.tool_name,
            &descriptor,
            context.risk_level.max(final_risk),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{SecurityExecutionGateway, SecurityExecutionRequest};
    use crate::safety::{
        BuiltInRole, DescriptorError, PermissionId, PolicyDecision, ResourceScope,
        ToolSecurityDescriptor,
    };
    use crate::tools::trait_def::RiskLevel;

    fn request(tool_name: &str, arguments: serde_json::Value) -> SecurityExecutionRequest {
        SecurityExecutionRequest {
            conversation_id: "conversation-1".to_string(),
            tool_call_id: "call-1".to_string(),
            tool_name: tool_name.to_string(),
            arguments,
        }
    }

    #[test]
    fn gateway_can_be_constructed() {
        let _gateway = SecurityExecutionGateway::new();
    }

    #[test]
    fn gateway_resolves_descriptor_for_known_tool() {
        let gateway = SecurityExecutionGateway::new();
        let request = request("read_file", serde_json::json!({"path": "README.md"}));

        let descriptor = gateway.resolve_descriptor(&request).unwrap();

        assert_eq!(descriptor.tool_name, "read_file");
    }

    #[test]
    fn gateway_rejects_unknown_tool_descriptor() {
        let gateway = SecurityExecutionGateway::new();
        let request = request("definitely_not_a_tool", serde_json::json!({}));

        let result = gateway.resolve_descriptor(&request);

        assert!(matches!(
            result,
            Err(DescriptorError::UnknownTool(tool)) if tool == "definitely_not_a_tool"
        ));
    }

    #[test]
    fn gateway_builds_decision_context_from_descriptor() {
        let gateway = SecurityExecutionGateway::new();
        let request = request("read_file", serde_json::json!({"path": "README.md"}));
        let descriptor = gateway.resolve_descriptor(&request).unwrap();

        let context = gateway
            .build_decision_context(&request, &descriptor, BuiltInRole::Standard, RiskLevel::Low)
            .unwrap();

        assert_eq!(context.role, BuiltInRole::Standard);
        assert_eq!(context.risk_level, RiskLevel::Low);
        assert_eq!(
            context.requested_permissions,
            vec![PermissionId::FilesystemRead]
        );
        assert_eq!(context.resource_scopes, vec![ResourceScope::Workspace]);
        assert!(context.reason.contains("read_file"));
    }

    #[test]
    fn gateway_rejects_descriptor_missing_security_information() {
        let gateway = SecurityExecutionGateway::new();
        let request = request("read_file", serde_json::json!({"path": "README.md"}));
        let descriptor = ToolSecurityDescriptor {
            tool_name: "read_file".to_string(),
            requested_permissions: vec![],
            resources: vec![],
            default_risk: RiskLevel::Low,
            side_effects: vec![],
        };

        let result = gateway.build_decision_context(
            &request,
            &descriptor,
            BuiltInRole::Standard,
            RiskLevel::Low,
        );

        assert!(matches!(result, Err(DescriptorError::InvalidDescriptor(_))));
    }

    #[test]
    fn gateway_keeps_dynamic_risk_low_for_ordinary_read() {
        let gateway = SecurityExecutionGateway::new();
        let request = request("read_file", serde_json::json!({"path": "README.md"}));

        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Low)
            .unwrap();

        assert_eq!(decision.context().risk_level, RiskLevel::Low);
    }

    #[test]
    fn gateway_promotes_sensitive_write_to_high_via_safety_policy() {
        let gateway = SecurityExecutionGateway::new();
        let request = request(
            "write_file",
            serde_json::json!({
                "path": "C:\\Windows\\System32\\hosts",
                "content": "blocked"
            }),
        );

        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .unwrap();

        assert_eq!(decision.context().risk_level, RiskLevel::High);
    }

    #[test]
    fn gateway_forwards_allow_decision_from_policy_engine() {
        let gateway = SecurityExecutionGateway::new();
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "hello"}),
        );
        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Allow(_)));
    }

    #[test]
    fn gateway_forwards_approval_decision_from_policy_engine() {
        let gateway = SecurityExecutionGateway::new();
        let request = request(
            "bash",
            serde_json::json!({"command": "git push origin develop"}),
        );
        let decision = gateway
            .evaluate(&request, BuiltInRole::Owner, RiskLevel::High)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::RequireApproval(_)));
    }

    #[test]
    fn gateway_forwards_deny_decision_from_policy_engine() {
        let gateway = SecurityExecutionGateway::new();
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "hello"}),
        );
        let decision = gateway
            .evaluate(&request, BuiltInRole::Restricted, RiskLevel::Medium)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Deny(_)));
    }
}
