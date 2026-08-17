// ============================================================
// GrantEvaluator — match a tool's resolved resource against a subject's
// persistent grants. Explicit Deny always wins. A missing allow is
// `RequireApproval` (never a hard deny), so the existing Ask workflow keeps
// working. Expired grants are ignored.
// ============================================================

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::safety::{PermissionId, ResourceDescriptor};

use super::model::{
    host_matches, parse_network_target, zone_allows, GrantDecision, GrantResource,
    ProcessGrantScope, SecurityGrant,
};

pub struct GrantEvaluator {
    grants: Vec<SecurityGrant>,
    workspace_root: PathBuf,
    /// Managed-process registry. Required for `ManagedChildren` to match at all:
    /// without it (or without a live pid record) a ManagedChildren grant does
    /// NOT authorize anything — it is never "any PID".
    registry: Option<Arc<crate::isolation::ManagedProcessRegistry>>,
}

impl GrantEvaluator {
    pub fn new(grants: Vec<SecurityGrant>, workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            grants,
            workspace_root: workspace_root.into(),
            registry: None,
        }
    }

    pub fn empty(workspace_root: impl Into<PathBuf>) -> Self {
        Self::new(Vec::new(), workspace_root)
    }

    pub fn with_registry(
        mut self,
        registry: Arc<crate::isolation::ManagedProcessRegistry>,
    ) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Evaluate a resolved resource. `now_ms` is the current epoch ms.
    pub fn evaluate(
        &self,
        permission: PermissionId,
        resource: &ResourceDescriptor,
        now_ms: i64,
    ) -> GrantDecision {
        // Only a subset of permissions are resource-grant enforced; the rest
        // (ProcessInspect, Desktop*, Skill, Agent, Mcp) remain RBAC-only.
        if !is_grant_enforced(permission) {
            return GrantDecision::Allow;
        }
        let mut allowed = false;
        for grant in &self.grants {
            if grant.permission != permission {
                continue;
            }
            if grant.is_expired(now_ms) {
                continue;
            }
            if !resource_matches(
                &grant.resource,
                resource,
                &self.workspace_root,
                self.registry.as_ref(),
            ) {
                continue;
            }
            match grant.effect {
                super::model::GrantEffect::Deny => {
                    return GrantDecision::Deny {
                        reason: "resource denied by explicit grant".to_string(),
                    }
                }
                super::model::GrantEffect::Allow => allowed = true,
            }
        }
        if allowed {
            GrantDecision::Allow
        } else {
            GrantDecision::RequireApproval {
                reason: "no matching resource grant".to_string(),
            }
        }
    }
}

fn is_grant_enforced(permission: PermissionId) -> bool {
    matches!(
        permission,
        PermissionId::FilesystemRead
            | PermissionId::FilesystemWrite
            | PermissionId::NetworkRequest
            | PermissionId::ProcessControl
            | PermissionId::ShellExecute
    )
}

fn resource_matches(
    resource: &GrantResource,
    desc: &ResourceDescriptor,
    root: &Path,
    registry: Option<&Arc<crate::isolation::ManagedProcessRegistry>>,
) -> bool {
    match (resource, desc) {
        (
            GrantResource::Filesystem {
                root: grant_root,
                recursive,
            },
            ResourceDescriptor::File { path },
        ) => {
            let Some(target) = canonicalize_target(root, Path::new(path)) else {
                return false;
            };
            let Ok(grant_root) = canonicalize_existing(root, Path::new(grant_root)) else {
                return false;
            };
            if *recursive {
                crate::safety::is_within_root(&grant_root, &target)
            } else {
                grant_root == target
            }
        }
        (
            GrantResource::Network {
                scheme,
                host,
                port,
                methods,
                zone,
            },
            ResourceDescriptor::Network { url, method },
        ) => {
            let Ok(parsed) = parse_network_target(url) else {
                return false;
            };
            if let Some(scheme) = scheme {
                if parsed.scheme != *scheme {
                    return false;
                }
            }
            if !host_matches(host, &parsed.host) {
                return false;
            }
            if let Some(port) = port {
                if parsed.port != Some(*port) {
                    return false;
                }
            }
            if !methods.is_empty() && !methods.iter().any(|m| m.eq_ignore_ascii_case(method)) {
                return false;
            }
            zone_allows(*zone, &parsed.host)
        }
        (GrantResource::Process { scope }, ResourceDescriptor::Process { pid, .. }) => {
            match scope {
                // ManagedChildren is NOT "any pid": it only matches a pid that
                // is currently tracked as a managed child. Without a registry
                // (or without a live record) it fails closed.
                ProcessGrantScope::ManagedChildren => match (registry, pid) {
                    (Some(registry), Some(pid)) => registry.contains_pid(*pid),
                    _ => false,
                },
                ProcessGrantScope::ExplicitPid { pid: granted } => Some(*granted) == *pid,
                ProcessGrantScope::AllHostProcesses => true,
            }
        }
        (
            GrantResource::Shell {
                host_escape_acknowledged,
            },
            ResourceDescriptor::Shell { .. },
        ) => *host_escape_acknowledged,
        _ => false,
    }
}

fn canonicalize_target(root: &Path, path: &Path) -> Option<PathBuf> {
    if path.is_absolute() {
        if path.exists() {
            std::fs::canonicalize(path).ok()
        } else {
            crate::safety::resolve_write_target(root, path).ok()
        }
    } else {
        let combined = root.join(path);
        if combined.exists() {
            std::fs::canonicalize(&combined).ok()
        } else {
            crate::safety::resolve_write_target(root, &combined).ok()
        }
    }
}

fn canonicalize_existing(root: &Path, path: &Path) -> Result<PathBuf, ()> {
    let combined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    std::fs::canonicalize(&combined).map_err(|_| ())
}
