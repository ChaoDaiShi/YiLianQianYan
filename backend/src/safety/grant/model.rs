// ============================================================
// Security grant model — typed resource grants for the Security Execution
// Gateway. A grant is a persistent, subject-scoped authorization for a
// specific permission + resource. It reuses the existing PermissionId — no
// second parallel permission enum.
// ============================================================

use serde::{Deserialize, Serialize};

use crate::safety::{PermissionId, ResourceScope};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkTarget {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
}

pub fn parse_network_target(raw: &str) -> Result<NetworkTarget, String> {
    let url = url::Url::parse(raw).map_err(|_| "invalid network URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("only http and https URLs are supported".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("network URL userinfo is not allowed".to_string());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "network URL host is required".to_string())?
        .to_ascii_lowercase();
    if host.is_empty() || host == "*" || host.contains('*') {
        return Err("network URL host must be a literal host".to_string());
    }
    Ok(NetworkTarget {
        scheme: url.scheme().to_string(),
        host,
        port: url.port_or_known_default(),
    })
}

pub fn host_matches(grant_host: &str, actual_host: &str) -> bool {
    let grant_host = grant_host.to_ascii_lowercase();
    let actual_host = actual_host.to_ascii_lowercase();
    if let Some(suffix) = grant_host.strip_prefix("*.") {
        let prefix = actual_host.strip_suffix(&format!(".{suffix}"));
        prefix.is_some_and(|prefix| !prefix.is_empty() && !prefix.contains('.'))
    } else {
        grant_host == actual_host
    }
}

pub fn zone_allows(zone: NetworkZone, host: &str) -> bool {
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match zone {
            NetworkZone::Public => classify_ip(ip) == NetworkZone::Public,
            NetworkZone::Loopback => classify_ip(ip) == NetworkZone::Loopback,
            NetworkZone::Private => classify_ip(ip) != NetworkZone::Public,
        };
    }
    match zone {
        NetworkZone::Public => !is_loopback_or_private_literal(host),
        NetworkZone::Loopback => is_loopback_literal(host),
        NetworkZone::Private => is_loopback_or_private_literal(host),
    }
}

pub fn classify_ip(ip: std::net::IpAddr) -> NetworkZone {
    match ip {
        std::net::IpAddr::V4(ip) if ip.is_loopback() => NetworkZone::Loopback,
        std::net::IpAddr::V4(ip)
            if ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_multicast() =>
        {
            NetworkZone::Private
        }
        std::net::IpAddr::V6(ip) if ip.is_loopback() => NetworkZone::Loopback,
        std::net::IpAddr::V6(ip)
            if ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_unspecified()
                || ip.is_multicast() =>
        {
            NetworkZone::Private
        }
        _ => NetworkZone::Public,
    }
}

fn is_loopback_literal(host: &str) -> bool {
    host == "localhost" || host == "::1" || host.starts_with("127.")
}

fn is_loopback_or_private_literal(host: &str) -> bool {
    if is_loopback_literal(host) {
        return true;
    }
    host == "0.0.0.0"
        || host == "169.254.169.254"
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
        || (host.starts_with("172.")
            && host
                .split('.')
                .nth(1)
                .and_then(|octet| octet.parse::<u8>().ok())
                .is_some_and(|octet| (16..=31).contains(&octet)))
        || host.starts_with("fc")
        || host.starts_with("fd")
}

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

/// Server-created authorization evidence passed to side-effecting tools.
/// Tool JSON can describe a request, but it cannot manufacture this evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorizedResource {
    Network {
        scheme: String,
        host: String,
        port: u16,
        method: String,
        zone: NetworkZone,
        grant_id: Option<String>,
        one_shot_approval: bool,
    },
    Process {
        pid: Option<u32>,
        managed_only: bool,
    },
    Desktop {
        action: String,
        target: Option<String>,
        one_shot_approval: bool,
    },
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
            GrantResource::Filesystem { root, .. },
        ) => !root.trim().is_empty() && !root.contains('\0'),
        (
            PermissionId::NetworkRequest,
            GrantResource::Network {
                scheme,
                host,
                methods,
                ..
            },
        ) => {
            let scheme_valid = scheme
                .as_deref()
                .map(|scheme| matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https"))
                .unwrap_or(true);
            let host_valid = (!host.trim().is_empty() && host != "*" && !host.contains('*'))
                || (host.starts_with("*.") && host.len() > 2 && !host[2..].contains('*'));
            let methods_valid = !methods.is_empty()
                && methods.iter().all(|method| {
                    matches!(
                        method.to_ascii_uppercase().as_str(),
                        "GET" | "HEAD" | "OPTIONS" | "POST" | "PUT" | "PATCH" | "DELETE"
                    )
                });
            scheme_valid && host_valid && methods_valid
        }
        (
            PermissionId::ProcessControl,
            GrantResource::Process {
                scope: ProcessGrantScope::ManagedChildren,
            },
        ) => true,
        (PermissionId::ProcessControl, GrantResource::Process { .. }) => false,
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
