use serde_json::Value;

use crate::tools::trait_def::RiskLevel;

use super::{
    describe_builtin_tool, BuiltInRole, DescriptorError, PolicyDecision, PolicyEngine,
    ToolSecurityDescriptor,
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

    pub fn evaluate(
        &self,
        request: &SecurityExecutionRequest,
        role: BuiltInRole,
        final_risk: RiskLevel,
    ) -> Result<PolicyDecision, DescriptorError> {
        let descriptor = self.resolve_descriptor(request)?;

        // PolicyEngine currently exposes a static evaluation API; retain it as
        // the gateway's explicit policy dependency while forwarding to that API.
        let _policy_engine = &self.policy_engine;
        Ok(PolicyEngine::evaluate(
            role,
            &request.tool_name,
            &descriptor,
            final_risk,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{SecurityExecutionGateway, SecurityExecutionRequest};
    use crate::safety::{BuiltInRole, DescriptorError, PolicyDecision};
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
