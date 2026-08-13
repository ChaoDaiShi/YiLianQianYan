use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Filesystem,
    Shell,
    Process,
    Network,
    Desktop,
    Skill,
    Agent,
    Mcp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Read,
    Write,
    Execute,
    Inspect,
    Control,
    Request,
    Observe,
    Interact,
    Load,
    Plan,
    Invoke,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PermissionId {
    #[serde(rename = "filesystem.read")]
    FilesystemRead,
    #[serde(rename = "filesystem.write")]
    FilesystemWrite,
    #[serde(rename = "shell.execute")]
    ShellExecute,
    #[serde(rename = "process.inspect")]
    ProcessInspect,
    #[serde(rename = "process.control")]
    ProcessControl,
    #[serde(rename = "network.request")]
    NetworkRequest,
    #[serde(rename = "desktop.observe")]
    DesktopObserve,
    #[serde(rename = "desktop.interact")]
    DesktopInteract,
    #[serde(rename = "skill.load")]
    SkillLoad,
    #[serde(rename = "agent.plan")]
    AgentPlan,
    #[serde(rename = "mcp.invoke")]
    McpInvoke,
}

impl PermissionId {
    pub const ALL: [Self; 11] = [
        Self::FilesystemRead,
        Self::FilesystemWrite,
        Self::ShellExecute,
        Self::ProcessInspect,
        Self::ProcessControl,
        Self::NetworkRequest,
        Self::DesktopObserve,
        Self::DesktopInteract,
        Self::SkillLoad,
        Self::AgentPlan,
        Self::McpInvoke,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FilesystemRead => "filesystem.read",
            Self::FilesystemWrite => "filesystem.write",
            Self::ShellExecute => "shell.execute",
            Self::ProcessInspect => "process.inspect",
            Self::ProcessControl => "process.control",
            Self::NetworkRequest => "network.request",
            Self::DesktopObserve => "desktop.observe",
            Self::DesktopInteract => "desktop.interact",
            Self::SkillLoad => "skill.load",
            Self::AgentPlan => "agent.plan",
            Self::McpInvoke => "mcp.invoke",
        }
    }

    pub const fn capability(self) -> Capability {
        match self {
            Self::FilesystemRead | Self::FilesystemWrite => Capability::Filesystem,
            Self::ShellExecute => Capability::Shell,
            Self::ProcessInspect | Self::ProcessControl => Capability::Process,
            Self::NetworkRequest => Capability::Network,
            Self::DesktopObserve | Self::DesktopInteract => Capability::Desktop,
            Self::SkillLoad => Capability::Skill,
            Self::AgentPlan => Capability::Agent,
            Self::McpInvoke => Capability::Mcp,
        }
    }

    pub const fn action(self) -> Action {
        match self {
            Self::FilesystemRead => Action::Read,
            Self::FilesystemWrite => Action::Write,
            Self::ShellExecute => Action::Execute,
            Self::ProcessInspect => Action::Inspect,
            Self::ProcessControl => Action::Control,
            Self::NetworkRequest => Action::Request,
            Self::DesktopObserve => Action::Observe,
            Self::DesktopInteract => Action::Interact,
            Self::SkillLoad => Action::Load,
            Self::AgentPlan => Action::Plan,
            Self::McpInvoke => Action::Invoke,
        }
    }

    pub const fn in_scope(self, scope: ResourceScope) -> RequestedPermission {
        RequestedPermission {
            permission: self,
            scope,
        }
    }
}

impl std::fmt::Display for PermissionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ResourceScope {
    Workspace,
    ShellCommand,
    Process,
    NetworkTarget,
    DesktopTarget,
    DiscoveredSkill,
    AgentInternal,
    McpServer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RequestedPermission {
    pub permission: PermissionId,
    pub scope: ResourceScope,
}
