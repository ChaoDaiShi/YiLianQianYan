use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::tools::trait_def::RiskLevel;

use super::capability::{PermissionId, RequestedPermission, ResourceScope};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectKind {
    FileMutation,
    ProcessMutation,
    NetworkEgress,
    DesktopMutation,
    ExternalService,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResourceDescriptor {
    File {
        path: String,
    },
    Shell {
        command: String,
        working_directory: Option<String>,
    },
    Process {
        action: String,
        pid: Option<u32>,
    },
    Network {
        url: String,
        method: String,
    },
    Desktop {
        action: String,
        target: Option<String>,
    },
    Skill {
        name: String,
    },
    Agent {
        action: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSecurityDescriptor {
    pub tool_name: String,
    pub requested_permissions: Vec<RequestedPermission>,
    pub resources: Vec<ResourceDescriptor>,
    pub default_risk: RiskLevel,
    pub side_effects: Vec<SideEffectKind>,
}

impl ToolSecurityDescriptor {
    pub fn validate(&self) -> Result<(), DescriptorError> {
        if self.tool_name.trim().is_empty() {
            return Err(DescriptorError::InvalidDescriptor(
                "tool_name must not be empty".to_string(),
            ));
        }
        if self.requested_permissions.is_empty() {
            return Err(DescriptorError::InvalidDescriptor(format!(
                "tool {} declares no permissions",
                self.tool_name
            )));
        }
        if self.resources.is_empty() {
            return Err(DescriptorError::InvalidDescriptor(format!(
                "tool {} declares no resources",
                self.tool_name
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DescriptorError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("tool {tool} is missing required argument {argument}")]
    MissingArgument {
        tool: String,
        argument: &'static str,
    },
    #[error("tool {tool} has invalid action {action}")]
    InvalidAction { tool: String, action: String },
    #[error("invalid security descriptor: {0}")]
    InvalidDescriptor(String),
}

fn required_string(tool: &str, args: &Value, key: &'static str) -> Result<String, DescriptorError> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| DescriptorError::MissingArgument {
            tool: tool.to_string(),
            argument: key,
        })
}

fn required_u32(tool: &str, args: &Value, key: &'static str) -> Result<u32, DescriptorError> {
    args.get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| DescriptorError::MissingArgument {
            tool: tool.to_string(),
            argument: key,
        })
}

fn permission(permission: PermissionId, scope: ResourceScope) -> RequestedPermission {
    permission.in_scope(scope)
}

fn finish(descriptor: ToolSecurityDescriptor) -> Result<ToolSecurityDescriptor, DescriptorError> {
    descriptor.validate()?;
    Ok(descriptor)
}

pub fn describe_builtin_tool(
    tool_name: &str,
    args: &Value,
) -> Result<ToolSecurityDescriptor, DescriptorError> {
    let descriptor = match tool_name {
        "read_file" => ToolSecurityDescriptor {
            tool_name: tool_name.to_string(),
            requested_permissions: vec![permission(
                PermissionId::FilesystemRead,
                ResourceScope::Workspace,
            )],
            resources: vec![ResourceDescriptor::File {
                path: required_string(tool_name, args, "path")?,
            }],
            default_risk: RiskLevel::Low,
            side_effects: vec![],
        },
        "write_file" | "edit_file" => ToolSecurityDescriptor {
            tool_name: tool_name.to_string(),
            requested_permissions: vec![permission(
                PermissionId::FilesystemWrite,
                ResourceScope::Workspace,
            )],
            resources: vec![ResourceDescriptor::File {
                path: required_string(tool_name, args, "path")?,
            }],
            default_risk: RiskLevel::Medium,
            side_effects: vec![SideEffectKind::FileMutation],
        },
        "grep" | "glob" => ToolSecurityDescriptor {
            tool_name: tool_name.to_string(),
            requested_permissions: vec![permission(
                PermissionId::FilesystemRead,
                ResourceScope::Workspace,
            )],
            resources: vec![ResourceDescriptor::File {
                path: args
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or(".")
                    .to_string(),
            }],
            default_risk: RiskLevel::Low,
            side_effects: vec![],
        },
        "bash" => ToolSecurityDescriptor {
            tool_name: tool_name.to_string(),
            requested_permissions: vec![permission(
                PermissionId::ShellExecute,
                ResourceScope::ShellCommand,
            )],
            resources: vec![ResourceDescriptor::Shell {
                command: required_string(tool_name, args, "command")?,
                working_directory: args.get("cwd").and_then(Value::as_str).map(str::to_string),
            }],
            default_risk: RiskLevel::High,
            side_effects: vec![SideEffectKind::ProcessMutation],
        },
        "process" => {
            let action = required_string(tool_name, args, "action")?;
            let (permission_id, risk, effects, pid) = match action.as_str() {
                "list" => (PermissionId::ProcessInspect, RiskLevel::Low, vec![], None),
                "kill" => (
                    PermissionId::ProcessControl,
                    RiskLevel::High,
                    vec![SideEffectKind::ProcessMutation],
                    Some(required_u32(tool_name, args, "pid")?),
                ),
                _ => {
                    return Err(DescriptorError::InvalidAction {
                        tool: tool_name.to_string(),
                        action,
                    });
                }
            };
            ToolSecurityDescriptor {
                tool_name: tool_name.to_string(),
                requested_permissions: vec![permission(permission_id, ResourceScope::Process)],
                resources: vec![ResourceDescriptor::Process { action, pid }],
                default_risk: risk,
                side_effects: effects,
            }
        }
        "http_request" => {
            let method = args
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("GET")
                .to_ascii_uppercase();
            let risk = match method.as_str() {
                "GET" | "HEAD" | "OPTIONS" => RiskLevel::Medium,
                _ => RiskLevel::High,
            };
            ToolSecurityDescriptor {
                tool_name: tool_name.to_string(),
                requested_permissions: vec![permission(
                    PermissionId::NetworkRequest,
                    ResourceScope::NetworkTarget,
                )],
                resources: vec![ResourceDescriptor::Network {
                    url: required_string(tool_name, args, "url")?,
                    method,
                }],
                default_risk: risk,
                side_effects: vec![SideEffectKind::NetworkEgress],
            }
        }
        "mouse" | "keyboard" => ToolSecurityDescriptor {
            tool_name: tool_name.to_string(),
            requested_permissions: vec![permission(
                PermissionId::DesktopInteract,
                ResourceScope::DesktopTarget,
            )],
            resources: vec![ResourceDescriptor::Desktop {
                action: required_string(tool_name, args, "action")?,
                target: None,
            }],
            // Coordinate and raw keyboard tools have no semantic UI target,
            // so even Owner must explicitly approve them.
            default_risk: RiskLevel::High,
            side_effects: vec![SideEffectKind::DesktopMutation],
        },
        "screenshot" | "windows_list" | "ui_inspect" | "ui_find" => {
            let target = if tool_name == "ui_find" {
                Some(
                    args.get("selector")
                        .ok_or_else(|| DescriptorError::MissingArgument {
                            tool: tool_name.to_string(),
                            argument: "selector",
                        })?
                        .to_string(),
                )
            } else {
                args.get("selector")
                    .map(Value::to_string)
                    .or_else(|| args.get("monitor").map(Value::to_string))
            };
            ToolSecurityDescriptor {
                tool_name: tool_name.to_string(),
                requested_permissions: vec![permission(
                    PermissionId::DesktopObserve,
                    ResourceScope::DesktopTarget,
                )],
                resources: vec![ResourceDescriptor::Desktop {
                    action: tool_name.to_string(),
                    target,
                }],
                default_risk: RiskLevel::Low,
                side_effects: vec![],
            }
        }
        "windows_focus" => {
            let target = args
                .get("name")
                .map(Value::to_string)
                .or_else(|| args.get("process_id").map(Value::to_string))
                .ok_or_else(|| DescriptorError::MissingArgument {
                    tool: tool_name.to_string(),
                    argument: "name_or_process_id",
                })?;
            ToolSecurityDescriptor {
                tool_name: tool_name.to_string(),
                requested_permissions: vec![permission(
                    PermissionId::DesktopInteract,
                    ResourceScope::DesktopTarget,
                )],
                resources: vec![ResourceDescriptor::Desktop {
                    action: tool_name.to_string(),
                    target: Some(target),
                }],
                default_risk: RiskLevel::Medium,
                side_effects: vec![SideEffectKind::DesktopMutation],
            }
        }
        "ui_invoke" | "ui_set_value" => {
            let target = args
                .get("selector")
                .ok_or_else(|| DescriptorError::MissingArgument {
                    tool: tool_name.to_string(),
                    argument: "selector",
                })?
                .to_string();
            ToolSecurityDescriptor {
                tool_name: tool_name.to_string(),
                requested_permissions: vec![permission(
                    PermissionId::DesktopInteract,
                    ResourceScope::DesktopTarget,
                )],
                resources: vec![ResourceDescriptor::Desktop {
                    action: tool_name.to_string(),
                    target: Some(target),
                }],
                // Unknown UI semantics may submit or mutate external state.
                default_risk: RiskLevel::High,
                side_effects: vec![SideEffectKind::DesktopMutation],
            }
        }
        "load_skill" => ToolSecurityDescriptor {
            tool_name: tool_name.to_string(),
            requested_permissions: vec![permission(
                PermissionId::SkillLoad,
                ResourceScope::DiscoveredSkill,
            )],
            resources: vec![ResourceDescriptor::Skill {
                name: required_string(tool_name, args, "name")?,
            }],
            default_risk: RiskLevel::Low,
            side_effects: vec![],
        },
        "write_todos" => ToolSecurityDescriptor {
            tool_name: tool_name.to_string(),
            requested_permissions: vec![permission(
                PermissionId::AgentPlan,
                ResourceScope::AgentInternal,
            )],
            resources: vec![ResourceDescriptor::Agent {
                action: "write_todos".to_string(),
            }],
            default_risk: RiskLevel::Low,
            side_effects: vec![],
        },
        "upscale_image" => {
            let path = required_string(tool_name, args, "path")?;
            ToolSecurityDescriptor {
                tool_name: tool_name.to_string(),
                requested_permissions: vec![
                    permission(PermissionId::FilesystemRead, ResourceScope::Workspace),
                    permission(PermissionId::FilesystemWrite, ResourceScope::Workspace),
                    permission(PermissionId::NetworkRequest, ResourceScope::NetworkTarget),
                ],
                resources: vec![
                    ResourceDescriptor::File { path: path.clone() },
                    ResourceDescriptor::Network {
                        url: "https://bigjpg.com".to_string(),
                        method: "POST".to_string(),
                    },
                ],
                default_risk: RiskLevel::Medium,
                side_effects: vec![
                    SideEffectKind::FileMutation,
                    SideEffectKind::NetworkEgress,
                    SideEffectKind::ExternalService,
                ],
            }
        }
        _ => return Err(DescriptorError::UnknownTool(tool_name.to_string())),
    };

    finish(descriptor)
}
