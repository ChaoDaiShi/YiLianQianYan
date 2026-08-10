use serde::{Deserialize, Serialize};

use super::{BuiltInRole, PermissionId};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum GrantMode {
    Allow,
    RequireApproval,
    Deny,
}

pub struct RolePolicy;

impl RolePolicy {
    pub const fn grant(role: BuiltInRole, permission: PermissionId) -> GrantMode {
        use BuiltInRole::{Owner, Restricted, Standard};
        use GrantMode::{Allow, Deny, RequireApproval};
        use PermissionId::*;

        match (role, permission) {
            (_, FilesystemRead | ProcessInspect | SkillLoad | AgentPlan) => Allow,
            (Owner | Standard, FilesystemWrite | NetworkRequest) => Allow,
            (Restricted, FilesystemWrite | NetworkRequest) => Deny,
            (Owner | Standard, ShellExecute | ProcessControl) => RequireApproval,
            (Restricted, ShellExecute | ProcessControl) => Deny,
            (Owner | Standard, DesktopObserve) => Allow,
            (Restricted, DesktopObserve) => RequireApproval,
            (Owner, DesktopInteract) => Allow,
            (Standard, DesktopInteract) => RequireApproval,
            (Restricted, DesktopInteract) => Deny,
        }
    }
}
