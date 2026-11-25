//! Runtime support projection, deliberately separate from authorization.
use super::{
    CapabilityDescriptor, CapabilityKind, CapabilityProviderKind, CapabilityRisk,
    CapabilityRuntimeStatus,
};
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityAvailability {
    pub discoverable: bool,
    pub bindable: bool,
    /// Supported by this runtime snapshot, not permission to execute arguments.
    pub executable: bool,
    pub requires_approval: bool,
    pub authorization: &'static str,
    pub reasons: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ContextAvailability {
    pub chat: CapabilityAvailability,
    pub task_node: CapabilityAvailability,
    pub personal: CapabilityAvailability,
}
fn projection(
    ready: bool,
    supported: bool,
    requires_approval: bool,
    unsupported: &str,
) -> CapabilityAvailability {
    CapabilityAvailability {
        discoverable: true,
        bindable: ready && supported,
        executable: ready && supported,
        requires_approval,
        authorization: "runtime_check_required",
        reasons: if !ready {
            vec!["能力已被发现，但当前运行时未就绪或已禁用".into()]
        } else if !supported {
            vec![unsupported.into()]
        } else {
            vec!["仅表示此运行时支持；实际参数、权限、预算与审批在执行前重新检查".into()]
        },
    }
}
fn has_header(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.contains_key("x-mcp-header") || map.values().any(has_header)
        }
        serde_json::Value::Array(values) => values.iter().any(has_header),
        _ => false,
    }
}
pub fn for_descriptor(
    d: &CapabilityDescriptor,
    tool_names: &HashSet<String>,
) -> ContextAvailability {
    let ready = d.enabled && d.status == CapabilityRuntimeStatus::Ready && d.metadata.runtime_ready;
    let source = d.metadata.source_id.as_deref().unwrap_or("");
    let requires_approval = matches!(d.risk, CapabilityRisk::High | CapabilityRisk::Dynamic);
    let chat_supported = match d.kind {
        CapabilityKind::Tool | CapabilityKind::McpTool => {
            !source.is_empty() && tool_names.contains(source)
        }
        CapabilityKind::Subagent => tool_names.contains(&format!("subagent_{source}")),
        CapabilityKind::Skill => tool_names.contains("load_skill"),
        _ => false,
    };
    let header_unsupported = d.input_schema.as_ref().is_some_and(has_header);
    let task_supported = match d.kind {
        CapabilityKind::Workflow => d.provider == CapabilityProviderKind::WorkflowRuntime,
        CapabilityKind::McpTool => {
            d.provider == CapabilityProviderKind::Mcp
                && d.risk == CapabilityRisk::High
                && source.strip_prefix("mcp_").map(|raw| format!("mcp.{raw}"))
                    == Some(d.id.to_string())
                && tool_names.contains(source)
                && !header_unsupported
        }
        _ => false,
    };
    ContextAvailability {
        chat: projection(
            ready,
            chat_supported,
            requires_approval,
            "此能力不能直接作为聊天工具调用；使用对应工作流或任务入口",
        ),
        task_node: projection(
            ready,
            task_supported,
            requires_approval,
            if header_unsupported {
                "任务节点尚不支持此 MCP 工具所需的安全请求头绑定"
            } else {
                "此能力未接入任务节点执行适配器；目录中的 Ready 不代表该入口支持执行"
            },
        ),
        personal: projection(
            ready,
            false,
            requires_approval,
            "个人委托使用其已授权的专用工具范围；能力目录不能为委托增加权限",
        ),
    }
}

pub fn public_descriptor(d: &CapabilityDescriptor, tools: &HashSet<String>) -> serde_json::Value {
    let mut value = serde_json::to_value(d).expect("bounded capability descriptor serializes");
    value["availability"] =
        serde_json::to_value(for_descriptor(d, tools)).expect("availability serializes");
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::capability::{CapabilityId, CapabilityMetadata};
    fn mcp() -> CapabilityDescriptor {
        CapabilityDescriptor {
            id: CapabilityId::new("mcp.demo_echo").unwrap(),
            kind: CapabilityKind::McpTool,
            provider: CapabilityProviderKind::Mcp,
            name: "echo".into(),
            description: "fixture".into(),
            input_schema: Some(serde_json::json!({"type":"object"})),
            output_schema: None,
            risk: CapabilityRisk::High,
            permissions: vec![],
            status: CapabilityRuntimeStatus::Ready,
            enabled: true,
            metadata: CapabilityMetadata {
                source_id: Some("mcp_demo_echo".into()),
                runtime_ready: true,
                ..Default::default()
            },
        }
    }
    #[test]
    fn availability_ready_is_not_authorization_or_personal_tool_access() {
        let a = for_descriptor(&mcp(), &["mcp_demo_echo".into()].into());
        assert!(a.chat.executable && a.task_node.bindable && a.task_node.requires_approval);
        assert!(!a.personal.bindable && !a.personal.executable);
        assert_eq!(a.task_node.authorization, "runtime_check_required");
    }
    #[test]
    fn availability_missing_mapping_and_header_binding_fail_closed() {
        let mut d = mcp();
        assert!(!for_descriptor(&d, &HashSet::new()).chat.executable);
        d.input_schema =
            Some(serde_json::json!({"properties":{"token":{"x-mcp-header":"Authorization"}}}));
        let a = for_descriptor(&d, &["mcp_demo_echo".into()].into());
        assert!(a.chat.executable);
        assert!(!a.task_node.executable);
        assert!(a.task_node.reasons[0].contains("请求头"));
    }
    #[test]
    fn availability_disabled_ready_metadata_never_enables_execution() {
        let mut d = mcp();
        d.enabled = false;
        let a = for_descriptor(&d, &["mcp_demo_echo".into()].into());
        assert!(a.chat.discoverable);
        assert!(!a.chat.bindable && !a.task_node.executable);
    }
    #[test]
    fn availability_builtin_tools_do_not_become_generic_task_capabilities() {
        let mut d = mcp();
        d.kind = CapabilityKind::Tool;
        d.provider = CapabilityProviderKind::Builtin;
        d.metadata.source_id = Some("read_file".into());
        let a = for_descriptor(&d, &["read_file".into()].into());
        assert!(a.chat.executable);
        assert!(!a.task_node.bindable);
    }
}
