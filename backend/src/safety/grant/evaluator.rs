// ============================================================
// GrantEvaluator — match a tool's resolved resource against a subject's
// persistent grants. Explicit Deny always wins. A missing allow is
// `RequireApproval` (never a hard deny), so the existing Ask workflow keeps
// working. Expired grants are ignored.
// ============================================================

use std::path::{Path, PathBuf};

use crate::safety::{PermissionId, ResourceDescriptor};

use super::model::{GrantDecision, GrantResource, NetworkZone, ProcessGrantScope, SecurityGrant};

pub struct GrantEvaluator {
    grants: Vec<SecurityGrant>,
    workspace_root: PathBuf,
}

impl GrantEvaluator {
    pub fn new(grants: Vec<SecurityGrant>, workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            grants,
            workspace_root: workspace_root.into(),
        }
    }

    pub fn empty(workspace_root: impl Into<PathBuf>) -> Self {
        Self::new(Vec::new(), workspace_root)
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
            if !resource_matches(&grant.resource, resource, &self.workspace_root) {
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

fn resource_matches(resource: &GrantResource, desc: &ResourceDescriptor, root: &Path) -> bool {
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
            let Ok(parsed) = parse_net_target(url) else {
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
                ProcessGrantScope::ManagedChildren => true,
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

struct NetTarget {
    scheme: String,
    host: String,
    port: Option<u16>,
}

fn parse_net_target(url: &str) -> Result<NetTarget, ()> {
    let (scheme, rest) = if let Some(r) = url.strip_prefix("https://") {
        ("https", r)
    } else if let Some(r) = url.strip_prefix("http://") {
        ("http", r)
    } else {
        return Err(());
    };
    let authority = rest
        .split('/')
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");
    if authority.contains('@') {
        return Err(()); // userinfo not allowed
    }
    let (host, port) = if let Some(host) = authority.strip_prefix('[') {
        // IPv6 literal
        let end = host.find(']').ok_or(())?;
        let h = &host[..end];
        let after = &host[end + 1..];
        let p = after.strip_prefix(':').and_then(|s| s.parse::<u16>().ok());
        (h.to_string(), p)
    } else {
        let (h, p) = authority
            .rsplit_once(':')
            .map(|(h, p)| (h.to_string(), p.parse::<u16>().ok()))
            .unwrap_or((authority.to_string(), None));
        (h, p)
    };
    if host.is_empty() {
        return Err(());
    }
    Ok(NetTarget {
        scheme: scheme.to_string(),
        host: host.to_lowercase(),
        port,
    })
}

fn host_matches(grant_host: &str, actual_host: &str) -> bool {
    if let Some(_wild) = grant_host.strip_prefix("*.") {
        // label-boundary wildcard: *.example.com matches a.example.com (single
        // label) but not evil-example.com or a.b.example.com.
        let suffix = &grant_host[1..]; // ".example.com"
        if !actual_host.ends_with(suffix) {
            return false;
        }
        let prefix_len = actual_host.len().saturating_sub(suffix.len());
        let prefix = &actual_host[..prefix_len];
        !prefix.is_empty() && !prefix.contains('.')
    } else {
        grant_host.eq_ignore_ascii_case(actual_host)
    }
}

fn zone_allows(zone: NetworkZone, host: &str) -> bool {
    match zone {
        NetworkZone::Public => {
            !is_literal_loopback(host) && !is_literal_private(host) && host != "localhost"
        }
        NetworkZone::Loopback => is_literal_loopback(host) || host == "localhost",
        NetworkZone::Private => is_literal_private(host) || is_literal_loopback(host),
    }
}

fn is_literal_loopback(host: &str) -> bool {
    host == "::1" || host.starts_with("127.")
}

fn is_literal_private(host: &str) -> bool {
    // RFC1918 + link-local + unspecified + multicast + metadata.
    host == "0.0.0.0"
        || host == "169.254.169.254"
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
        || host.starts_with("172.16.")
        || host.starts_with("172.17.")
        || host.starts_with("172.18.")
        || host.starts_with("172.19.")
        || host.starts_with("172.20.")
        || host.starts_with("172.21.")
        || host.starts_with("172.22.")
        || host.starts_with("172.23.")
        || host.starts_with("172.24.")
        || host.starts_with("172.25.")
        || host.starts_with("172.26.")
        || host.starts_with("172.27.")
        || host.starts_with("172.28.")
        || host.starts_with("172.29.")
        || host.starts_with("172.30.")
        || host.starts_with("172.31.")
        || host.starts_with("fc")
        || host.starts_with("fd")
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
