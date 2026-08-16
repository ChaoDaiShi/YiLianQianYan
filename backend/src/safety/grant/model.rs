// ============================================================
// Security grant model — typed resource grants for the Security Execution
// Gateway. A grant is a persistent, subject-scoped authorization for a
// specific permission + resource. It reuses the existing PermissionId — no
// second parallel permission enum.
// ============================================================

use serde::{Deserialize, Serialize};

use crate::safety::{PermissionId, ResourceScope};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantEffect {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantSource {
    User,
    Migration,
    System,
}

/// Network address class. `Public` is the default and excludes loopback /
/// private / link-local / unspecified / multicast / metadata targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkZone {
    Public,
    Loopback,
    Private,
}

/// Scope of process control. `AllHostProcesses` is deliberately a distinct,
/// high-risk variant — it is never implied by a plain allow grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProcessGrantScope {
    ManagedChildren,
    ExplicitPid { pid: u32 },
    AllHostProcesses,
}

/// A typed grant resource. Never a raw JSON string that callers must guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GrantResource {
    Filesystem {
        /// Canonical root directory.
        root: String,
        #[serde(default)]
        recursive: bool,
    },
    Network {
        #[serde(default)]
        scheme: Option<String>,
        host: String,
        #[serde(default)]
        port: Option<u16>,
        /// At least one HTTP method (e.g. GET). Empty = reject at validation.
        #[serde(default)]
        methods: Vec<String>,
        #[serde(default = "default_zone")]
        zone: NetworkZone,
    },
    Process {
        scope: ProcessGrantScope,
    },
    Shell {
        host_escape_acknowledged: bool,
    },
}

fn default_zone() -> NetworkZone {
    NetworkZone::Public
}

/// A persistent security grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityGrant {
    pub id: String,
    pub subject_id: String,
    pub effect: GrantEffect,
    pub permission: PermissionId,
    pub resource: GrantResource,
    pub source: GrantSource,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

impl SecurityGrant {
    pub fn is_expired(&self, now_ms: i64) -> bool {
        self.expires_at.map(|exp| now_ms >= exp).unwrap_or(false)
    }
}

/// Validate that a permission/resource combination is coherent.
pub fn validate_grant(permission: PermissionId, resource: &GrantResource) -> Result<(), String> {
    let ok = match (permission, resource) {
        (
            PermissionId::FilesystemRead | PermissionId::FilesystemWrite,
            GrantResource::Filesystem { .. },
        ) => true,
        (PermissionId::NetworkRequest, GrantResource::Network { methods, .. }) => {
            !methods.is_empty()
        }
        (PermissionId::ProcessControl, GrantResource::Process { .. }) => true,
        (PermissionId::ShellExecute, GrantResource::Shell { .. }) => true,
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(format!(
            "invalid grant: permission {} cannot be combined with this resource",
            permission.as_str()
        ))
    }
}

/// The scope a grant resource resolves to (used to match descriptor resources).
pub fn resource_scope(permission: PermissionId) -> ResourceScope {
    match permission {
        PermissionId::FilesystemRead | PermissionId::FilesystemWrite => ResourceScope::Workspace,
        PermissionId::NetworkRequest => ResourceScope::NetworkTarget,
        PermissionId::ProcessControl | PermissionId::ProcessInspect => ResourceScope::Process,
        PermissionId::ShellExecute => ResourceScope::ShellCommand,
        _ => ResourceScope::Workspace,
    }
}

/// Result of evaluating a resource against the subject's grants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantDecision {
    Allow,
    RequireApproval { reason: String },
    Deny { reason: String },
}
