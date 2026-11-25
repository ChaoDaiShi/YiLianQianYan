use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SubjectType {
    LocalUser,
    WindowsAccount,
    OrganizationUser,
    Subagent,
    PluginService,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BuiltInRole {
    Owner,
    Standard,
    Restricted,
}

impl BuiltInRole {
    pub const ALL: [Self; 3] = [Self::Owner, Self::Standard, Self::Restricted];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Standard => "standard",
            Self::Restricted => "restricted",
        }
    }
}

impl std::fmt::Display for BuiltInRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecuritySubject {
    pub subject_id: String,
    pub subject_type: SubjectType,
    pub provider: String,
    pub external_ref: Option<String>,
}

impl SecuritySubject {
    pub fn local_user() -> Self {
        Self {
            subject_id: "local-user".to_string(),
            subject_type: SubjectType::LocalUser,
            provider: "local".to_string(),
            external_ref: None,
        }
    }

    /// Build a subject carrying only a subject id, as reconstructed from an
    /// [`crate::execution::ExecutionContext`]. The subject id is the source of
    /// truth for role resolution; type/provider are neutral placeholders and
    /// are never used to authorize anything.
    pub fn from_subject_id(subject_id: impl Into<String>) -> Self {
        Self {
            subject_id: subject_id.into(),
            subject_type: SubjectType::LocalUser,
            provider: "workflow".to_string(),
            external_ref: None,
        }
    }

    pub(crate) fn subagent(name: &str) -> Self {
        Self {
            subject_id: format!("subagent:{name}"),
            subject_type: SubjectType::Subagent,
            provider: "local".into(),
            external_ref: None,
        }
    }

    pub(crate) fn acp_agent(id: &str) -> Self {
        Self {
            subject_id: format!("acp-agent:{id}"),
            subject_type: SubjectType::Subagent,
            provider: "acp".into(),
            external_ref: None,
        }
    }
}

/// Identity emitted by the gateway when it actually dispatches a tool.
#[derive(Debug, Clone)]
pub(crate) struct ToolInvocation {
    pub(crate) subject: SecuritySubject,
    pub(crate) agent_id: Option<String>,
    pub(crate) conversation_id: String,
    pub(crate) tool_call_id: String,
    pub(crate) tool_name: String,
}

/// Authority stays with the local child and carries a reference to the
/// particular approved or directly authorized delegation that started it.
#[derive(Debug, Clone)]
pub(crate) struct DelegatedSubject {
    pub(crate) subject: SecuritySubject,
    pub(crate) child_name: String,
    pub(crate) handoff_id: String,
    pub(crate) parent: ToolInvocation,
    pub(crate) default_read_root: std::path::PathBuf,
}

impl DelegatedSubject {
    pub(crate) fn new(
        subject: SecuritySubject,
        name: &str,
        handoff_id: String,
        parent: ToolInvocation,
        default_read_root: std::path::PathBuf,
    ) -> Self {
        Self {
            subject,
            child_name: name.into(),
            handoff_id,
            parent,
            default_read_root,
        }
    }
}

/// Only server-side service registration supplies the plugin and service IDs.
/// A live process invocation is independently authorized by its service runtime.
#[derive(Debug, Clone)]
pub(crate) struct PluginServicePrincipal(SecuritySubject);

impl PluginServicePrincipal {
    pub(crate) fn new(plugin_id: &str, service_id: &str) -> Self {
        Self(SecuritySubject {
            subject_id: format!("plugin-service:{plugin_id}/{service_id}"),
            subject_type: SubjectType::PluginService,
            provider: "local".into(),
            external_ref: None,
        })
    }

    pub(crate) fn subject(&self) -> &SecuritySubject {
        &self.0
    }
}
