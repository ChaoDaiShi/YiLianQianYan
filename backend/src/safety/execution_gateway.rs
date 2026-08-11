use serde_json::Value;

use crate::tools::trait_def::RiskLevel;

use super::{BuiltInRole, PolicyDecision, PolicyEngine, ToolSecurityDescriptor};

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

    pub fn evaluate(
        &self,
        request: &SecurityExecutionRequest,
        role: BuiltInRole,
        descriptor: &ToolSecurityDescriptor,
        final_risk: RiskLevel,
    ) -> PolicyDecision {
        // PolicyEngine currently exposes a static evaluation API; retain it as
        // the gateway's explicit policy dependency while forwarding to that API.
        let _policy_engine = &self.policy_engine;
        PolicyEngine::evaluate(role, &request.tool_name, descriptor, final_risk)
    }
}

#[cfg(test)]
mod tests {
    use super::{SecurityExecutionGateway, SecurityExecutionRequest};
    use crate::safety::{describe_builtin_tool, BuiltInRole, PolicyDecision};
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
    fn gateway_forwards_allow_decision_from_policy_engine() {
        let gateway = SecurityExecutionGateway::new();
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "hello"}),
        );
        let descriptor = describe_builtin_tool("write_file", &request.arguments).unwrap();

        let decision = gateway.evaluate(
            &request,
            BuiltInRole::Standard,
            &descriptor,
            RiskLevel::Medium,
        );

        assert!(matches!(decision, PolicyDecision::Allow(_)));
    }

    #[test]
    fn gateway_forwards_approval_decision_from_policy_engine() {
        let gateway = SecurityExecutionGateway::new();
        let request = request(
            "bash",
            serde_json::json!({"command": "git push origin develop"}),
        );
        let descriptor = describe_builtin_tool("bash", &request.arguments).unwrap();

        let decision = gateway.evaluate(&request, BuiltInRole::Owner, &descriptor, RiskLevel::High);

        assert!(matches!(decision, PolicyDecision::RequireApproval(_)));
    }

    #[test]
    fn gateway_forwards_deny_decision_from_policy_engine() {
        let gateway = SecurityExecutionGateway::new();
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "hello"}),
        );
        let descriptor = describe_builtin_tool("write_file", &request.arguments).unwrap();

        let decision = gateway.evaluate(
            &request,
            BuiltInRole::Restricted,
            &descriptor,
            RiskLevel::Medium,
        );

        assert!(matches!(decision, PolicyDecision::Deny(_)));
    }
}
