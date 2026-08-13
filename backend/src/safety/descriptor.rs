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
    NetworkFromResponse {
        source: String,
        target_template: String,
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
    Mcp {
        server_id: String,
        tool_name: String,
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
        let profile = builtin_descriptor_profile(self)?;
        if self.requested_permissions != profile.requested_permissions {
            return Err(DescriptorError::InvalidDescriptor(format!(
                "tool {} permissions do not match its built-in profile",
                self.tool_name
            )));
        }
        if self.default_risk < profile.minimum_risk {
            return Err(DescriptorError::InvalidDescriptor(format!(
                "tool {} risk {} is below its built-in minimum {}",
                self.tool_name, self.default_risk, profile.minimum_risk
            )));
        }
        if self.side_effects != profile.side_effects {
            return Err(DescriptorError::InvalidDescriptor(format!(
                "tool {} side effects do not match its built-in profile",
                self.tool_name
            )));
        }
        Ok(())
    }

    pub fn validate_for_tool(&self, expected_tool_name: &str) -> Result<(), DescriptorError> {
        self.validate()?;
        if self.tool_name != expected_tool_name {
            return Err(DescriptorError::InvalidDescriptor(format!(
                "descriptor tool {} does not match requested tool {expected_tool_name}",
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

struct DescriptorProfile {
    requested_permissions: Vec<RequestedPermission>,
    minimum_risk: RiskLevel,
    side_effects: Vec<SideEffectKind>,
}

fn builtin_descriptor_profile(
    descriptor: &ToolSecurityDescriptor,
) -> Result<DescriptorProfile, DescriptorError> {
    let tool_name = descriptor.tool_name.as_str();
    let invalid_resources = || {
        DescriptorError::InvalidDescriptor(format!(
            "tool {tool_name} resources do not match its built-in profile"
        ))
    };
    let profile = |requested_permissions, minimum_risk, side_effects| DescriptorProfile {
        requested_permissions,
        minimum_risk,
        side_effects,
    };
    let single_file = matches!(
        descriptor.resources.as_slice(),
        [ResourceDescriptor::File { path }] if !path.trim().is_empty()
    );

    let value = match tool_name {
        "read_file" if single_file => profile(
            vec![permission(
                PermissionId::FilesystemRead,
                ResourceScope::Workspace,
            )],
            RiskLevel::Low,
            vec![],
        ),
        "write_file" | "edit_file" if single_file => profile(
            vec![permission(
                PermissionId::FilesystemWrite,
                ResourceScope::Workspace,
            )],
            RiskLevel::Medium,
            vec![SideEffectKind::FileMutation],
        ),
        "grep" | "glob" if single_file => profile(
            vec![permission(
                PermissionId::FilesystemRead,
                ResourceScope::Workspace,
            )],
            RiskLevel::Low,
            vec![],
        ),
        "bash" => match descriptor.resources.as_slice() {
            [ResourceDescriptor::Shell { command, .. }] if !command.trim().is_empty() => profile(
                vec![permission(
                    PermissionId::ShellExecute,
                    ResourceScope::ShellCommand,
                )],
                RiskLevel::High,
                vec![SideEffectKind::ProcessMutation],
            ),
            _ => return Err(invalid_resources()),
        },
        "process" => match descriptor.resources.as_slice() {
            [ResourceDescriptor::Process { action, pid: None }] if action == "list" => profile(
                vec![permission(
                    PermissionId::ProcessInspect,
                    ResourceScope::Process,
                )],
                RiskLevel::Low,
                vec![],
            ),
            [ResourceDescriptor::Process {
                action,
                pid: Some(_),
            }] if action == "kill" => profile(
                vec![permission(
                    PermissionId::ProcessControl,
                    ResourceScope::Process,
                )],
                RiskLevel::High,
                vec![SideEffectKind::ProcessMutation],
            ),
            _ => return Err(invalid_resources()),
        },
        "http_request" => match descriptor.resources.as_slice() {
            [ResourceDescriptor::Network { url, method }]
                if !url.trim().is_empty() && !method.trim().is_empty() =>
            {
                let minimum_risk = match method.as_str() {
                    "GET" | "HEAD" | "OPTIONS" => RiskLevel::Medium,
                    _ => RiskLevel::High,
                };
                profile(
                    vec![permission(
                        PermissionId::NetworkRequest,
                        ResourceScope::NetworkTarget,
                    )],
                    minimum_risk,
                    vec![SideEffectKind::NetworkEgress],
                )
            }
            _ => return Err(invalid_resources()),
        },
        "mouse" | "keyboard" => match descriptor.resources.as_slice() {
            [ResourceDescriptor::Desktop {
                action,
                target: None,
            }] if !action.trim().is_empty() => profile(
                vec![permission(
                    PermissionId::DesktopInteract,
                    ResourceScope::DesktopTarget,
                )],
                RiskLevel::High,
                vec![SideEffectKind::DesktopMutation],
            ),
            _ => return Err(invalid_resources()),
        },
        "screenshot" | "windows_list" | "ui_inspect" | "ui_find" => {
            match descriptor.resources.as_slice() {
                [ResourceDescriptor::Desktop { action, target }]
                    if action == tool_name
                        && (tool_name != "ui_find"
                            || target
                                .as_ref()
                                .is_some_and(|value| !value.trim().is_empty())) =>
                {
                    profile(
                        vec![permission(
                            PermissionId::DesktopObserve,
                            ResourceScope::DesktopTarget,
                        )],
                        RiskLevel::Low,
                        vec![],
                    )
                }
                _ => return Err(invalid_resources()),
            }
        }
        "windows_focus" => match descriptor.resources.as_slice() {
            [ResourceDescriptor::Desktop {
                action,
                target: Some(target),
            }] if action == tool_name && !target.trim().is_empty() => profile(
                vec![permission(
                    PermissionId::DesktopInteract,
                    ResourceScope::DesktopTarget,
                )],
                RiskLevel::Medium,
                vec![SideEffectKind::DesktopMutation],
            ),
            _ => return Err(invalid_resources()),
        },
        "ui_invoke" | "ui_set_value" => match descriptor.resources.as_slice() {
            [ResourceDescriptor::Desktop {
                action,
                target: Some(target),
            }] if action == tool_name && !target.trim().is_empty() => profile(
                vec![permission(
                    PermissionId::DesktopInteract,
                    ResourceScope::DesktopTarget,
                )],
                RiskLevel::High,
                vec![SideEffectKind::DesktopMutation],
            ),
            _ => return Err(invalid_resources()),
        },
        "load_skill" => match descriptor.resources.as_slice() {
            [ResourceDescriptor::Skill { name }] if !name.trim().is_empty() => profile(
                vec![permission(
                    PermissionId::SkillLoad,
                    ResourceScope::DiscoveredSkill,
                )],
                RiskLevel::Low,
                vec![],
            ),
            _ => return Err(invalid_resources()),
        },
        "write_todos" => match descriptor.resources.as_slice() {
            [ResourceDescriptor::Agent { action }] if action == "write_todos" => profile(
                vec![permission(
                    PermissionId::AgentPlan,
                    ResourceScope::AgentInternal,
                )],
                RiskLevel::Low,
                vec![],
            ),
            _ => return Err(invalid_resources()),
        },
        "upscale_image" => match descriptor.resources.as_slice() {
            [ResourceDescriptor::File { path: input }, ResourceDescriptor::File { path: output }, ResourceDescriptor::Network { url, method }, ResourceDescriptor::NetworkFromResponse {
                source: status_source,
                target_template: status_template,
                method: status_method,
            }, ResourceDescriptor::NetworkFromResponse {
                source: download_source,
                target_template: download_template,
                method: download_method,
            }] if !input.trim().is_empty()
                && output == &upscale_output_path(input)
                && url == "https://bigjpg.com/api/task/"
                && method == "POST"
                && status_source == "bigjpg.task_id"
                && status_template == "https://bigjpg.com/api/task/{value}"
                && status_method == "GET"
                && download_source == "bigjpg.task_result.url"
                && download_template == "{value}"
                && download_method == "GET" =>
            {
                profile(
                    vec![
                        permission(PermissionId::FilesystemRead, ResourceScope::Workspace),
                        permission(PermissionId::FilesystemWrite, ResourceScope::Workspace),
                        permission(PermissionId::NetworkRequest, ResourceScope::NetworkTarget),
                    ],
                    RiskLevel::High,
                    vec![
                        SideEffectKind::FileMutation,
                        SideEffectKind::NetworkEgress,
                        SideEffectKind::ExternalService,
                    ],
                )
            }
            _ => return Err(invalid_resources()),
        },
        "read_file" | "write_file" | "edit_file" | "grep" | "glob" => {
            return Err(invalid_resources())
        }
        // MCP tools: namespaced `mcp_*` names, always an external-service
        // invocation at minimum High risk regardless of any metadata claims.
        m if m.starts_with("mcp_") => match descriptor.resources.as_slice() {
            [ResourceDescriptor::Mcp {
                server_id,
                tool_name,
            }] if !server_id.trim().is_empty() && !tool_name.trim().is_empty() => profile(
                vec![permission(
                    PermissionId::McpInvoke,
                    ResourceScope::McpServer,
                )],
                RiskLevel::High,
                vec![SideEffectKind::ExternalService],
            ),
            _ => return Err(invalid_resources()),
        },
        _ => return Err(DescriptorError::UnknownTool(descriptor.tool_name.clone())),
    };

    Ok(value)
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

fn upscale_output_path(input_path: &str) -> String {
    let input = std::path::Path::new(input_path);
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let extension = input.extension().unwrap_or_default().to_string_lossy();
    let directory = input.parent().unwrap_or_else(|| std::path::Path::new("."));
    let output = if extension.is_empty() {
        directory.join(format!("{stem}_upscaled"))
    } else {
        directory.join(format!("{stem}_upscaled.{extension}"))
    };
    output.to_string_lossy().to_string()
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
            let output_path = upscale_output_path(&path);
            ToolSecurityDescriptor {
                tool_name: tool_name.to_string(),
                requested_permissions: vec![
                    permission(PermissionId::FilesystemRead, ResourceScope::Workspace),
                    permission(PermissionId::FilesystemWrite, ResourceScope::Workspace),
                    permission(PermissionId::NetworkRequest, ResourceScope::NetworkTarget),
                ],
                resources: vec![
                    ResourceDescriptor::File { path: path.clone() },
                    ResourceDescriptor::File { path: output_path },
                    ResourceDescriptor::Network {
                        url: "https://bigjpg.com/api/task/".to_string(),
                        method: "POST".to_string(),
                    },
                    ResourceDescriptor::NetworkFromResponse {
                        source: "bigjpg.task_id".to_string(),
                        target_template: "https://bigjpg.com/api/task/{value}".to_string(),
                        method: "GET".to_string(),
                    },
                    ResourceDescriptor::NetworkFromResponse {
                        source: "bigjpg.task_result.url".to_string(),
                        target_template: "{value}".to_string(),
                        method: "GET".to_string(),
                    },
                ],
                default_risk: RiskLevel::High,
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
