//! Validated, server-owned capability bindings. Registry remains discovery-only.
use crate::modules::capability::CapabilityDescriptor;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutableCapabilityBinding {
    pub capability_id: String,
    pub tool_registry_name: String,
    pub arguments: Value,
}

pub const MAX_ARGUMENT_BYTES: usize = 16 * 1024;
pub const HEADER_UNSUPPORTED: &str = "此工具需要安全请求头参数，当前画布尚未支持安全凭据绑定。";

pub fn has_header_binding(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.contains_key("x-mcp-header") || map.values().any(has_header_binding)
        }
        Value::Array(items) => items.iter().any(has_header_binding),
        _ => false,
    }
}
fn bounded_depth(value: &Value, depth: usize) -> bool {
    depth <= 16
        && match value {
            Value::Object(map) => map.values().all(|v| bounded_depth(v, depth + 1)),
            Value::Array(items) => items.iter().all(|v| bounded_depth(v, depth + 1)),
            _ => true,
        }
}

pub fn validate_binding(
    input: &Value,
    descriptor: Option<&CapabilityDescriptor>,
) -> Result<ExecutableCapabilityBinding, String> {
    use crate::modules::capability::{
        CapabilityKind, CapabilityProviderKind, CapabilityRuntimeStatus,
    };
    let id = input
        .get("executor_ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("capability://"))
        .filter(|id| !id.is_empty())
        .ok_or("invalid_capability_binding")?;
    let descriptor = descriptor.ok_or("ProviderUnavailable: 能力当前不可用")?;
    if descriptor.id.as_str() != id
        || descriptor.kind != CapabilityKind::McpTool
        || descriptor.provider != CapabilityProviderKind::Mcp
        || !descriptor.enabled
        || descriptor.status != CapabilityRuntimeStatus::Ready
        || !descriptor.metadata.runtime_ready
    {
        return Err("ProviderUnavailable: 当前类型或状态不支持执行".into());
    }
    if descriptor
        .input_schema
        .as_ref()
        .is_some_and(has_header_binding)
    {
        return Err(HEADER_UNSUPPORTED.into());
    }
    for forbidden in [
        "server_id",
        "transport",
        "url",
        "remote_command",
        "secret",
        "api_key",
        "tool_registry_name",
    ] {
        if input.get(forbidden).is_some() {
            return Err("client_execution_routing_forbidden".into());
        }
    }
    let arguments = input
        .get("capability_input")
        .filter(|v| v.is_object())
        .ok_or("capability_input_must_be_object")?;
    if arguments.to_string().len() > MAX_ARGUMENT_BYTES || !bounded_depth(arguments, 0) {
        return Err("capability_input_exceeds_limit".into());
    }
    let tool_name = descriptor
        .metadata
        .source_id
        .as_deref()
        .ok_or("invalid_capability_source")?;
    if tool_name
        .strip_prefix("mcp_")
        .map(|remote| format!("mcp.{remote}"))
        != Some(id.to_string())
    {
        return Err("invalid_capability_source".into());
    }
    Ok(ExecutableCapabilityBinding {
        capability_id: id.into(),
        tool_registry_name: tool_name.into(),
        arguments: arguments.clone(),
    })
}

pub async fn resolve_binding(
    server: &crate::app::state::AppServer,
    input: &Value,
) -> Result<ExecutableCapabilityBinding, String> {
    use crate::modules::capability::CapabilityId;
    let id = input
        .get("executor_ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("capability://"))
        .ok_or("invalid_capability_binding")?;
    let id = CapabilityId::new(id).map_err(|_| "invalid_capability_identity")?;
    // Fresh discovery reads managed runtime snapshots, never probes or calls a tool.
    let registry = server.build_capability_registry().await;
    let descriptor = registry.get(&id);
    validate_binding(input, descriptor.as_ref())
}

pub async fn validate_node_input(
    server: &crate::app::state::AppServer,
    input: &Value,
) -> Result<(), super::super::TaskWorldRuntimeError> {
    if input
        .get("executor_ref")
        .and_then(Value::as_str)
        .is_some_and(|r| r.starts_with("capability:"))
    {
        resolve_binding(server, input).await.map_err(|e| {
            super::super::TaskWorldRuntimeError::Harness(super::super::TaskHarnessError::Graph(e))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::capability::*;
    use serde_json::json;
    fn descriptor() -> CapabilityDescriptor {
        CapabilityDescriptor {
            id: CapabilityId::new("mcp.server_a_echo").unwrap(),
            kind: CapabilityKind::McpTool,
            provider: CapabilityProviderKind::Mcp,
            name: "echo".into(),
            description: "fixture".into(),
            input_schema: Some(json!({"type":"object","properties":{"text":{"type":"string"}}})),
            risk: CapabilityRisk::High,
            permissions: vec![],
            status: CapabilityRuntimeStatus::Ready,
            enabled: true,
            metadata: CapabilityMetadata {
                source_id: Some("mcp_server_a_echo".into()),
                runtime_ready: true,
                ..Default::default()
            },
        }
    }
    fn input() -> Value {
        json!({"executor_ref":"capability://mcp.server_a_echo","capability_input":{"text":"hello"}})
    }
    #[test]
    fn capability_binding_exact_identity_and_arguments() {
        let binding = validate_binding(&input(), Some(&descriptor())).unwrap();
        assert_eq!(binding.tool_registry_name, "mcp_server_a_echo");
        assert_eq!(binding.arguments, json!({"text":"hello"}));
    }
    #[test]
    fn capability_binding_requires_all_readiness_flags() {
        for flag in 0..5 {
            let mut d = descriptor();
            match flag {
                0 => d.enabled = false,
                1 => d.status = CapabilityRuntimeStatus::Unavailable,
                2 => d.metadata.runtime_ready = false,
                3 => d.kind = CapabilityKind::McpPrompt,
                _ => d.provider = CapabilityProviderKind::Builtin,
            };
            assert!(validate_binding(&input(), Some(&d)).is_err());
        }
        assert!(validate_binding(&input(), None).is_err());
    }
    #[test]
    fn capability_binding_rejects_client_routing_and_invalid_arguments() {
        for bad in [json!(null), json!([]), json!("text")] {
            let mut i = input();
            i["capability_input"] = bad;
            assert!(validate_binding(&i, Some(&descriptor())).is_err());
        }
        let mut i = input();
        i["tool_registry_name"] = json!("mcp_other");
        assert!(validate_binding(&i, Some(&descriptor())).is_err());
        i = input();
        i["executor_ref"] = json!("workflow://mcp.server_a_echo");
        assert!(validate_binding(&i, Some(&descriptor())).is_err());
        i = input();
        i["capability_input"] = json!({"text":"x".repeat(17000)});
        assert!(validate_binding(&i, Some(&descriptor())).is_err());
    }
    #[test]
    fn capability_binding_rejects_header_at_any_schema_depth() {
        let mut d = descriptor();
        d.input_schema = Some(
            json!({"type":"object","$defs":{"nested":{"properties":{"token":{"type":"string","x-mcp-header":"Authorization"}}}}}),
        );
        assert!(validate_binding(&input(), Some(&d)).is_err());
    }
    #[test]
    fn capability_binding_never_uses_display_name() {
        let mut d = descriptor();
        d.id = CapabilityId::new("mcp.server_b_echo").unwrap();
        d.metadata.source_id = Some("mcp_server_b_echo".into());
        assert!(validate_binding(&input(), Some(&d)).is_err());
        let mut i = input();
        i["executor_ref"] = json!("capability://mcp.server_b_echo");
        assert_eq!(
            validate_binding(&i, Some(&d)).unwrap().tool_registry_name,
            "mcp_server_b_echo"
        );
    }
}
