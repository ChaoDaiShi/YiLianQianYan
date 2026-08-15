// ============================================================
// Workspace model — identity, lifecycle, and field limits.
// ============================================================

use serde::{Deserialize, Serialize};

use super::service::validate_workspace_fields;

/// A validated workspace identifier (UUID v4 by default).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceId(String);

impl WorkspaceId {
    pub fn new(raw: impl Into<String>) -> Result<Self, WorkspaceFieldError> {
        let raw = raw.into();
        if raw.is_empty() || raw.chars().any(char::is_control) {
            return Err(WorkspaceFieldError::InvalidId);
        }
        Ok(Self(raw))
    }

    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Workspace lifecycle. A Workspace is not an execution, so there is no
/// running / paused state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Active,
    Archived,
}

impl std::fmt::Display for WorkspaceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Active => "active",
            Self::Archived => "archived",
        })
    }
}

/// A project container for tasks.
///
/// `root_path` is a convenience context hint only. It never grants filesystem
/// access by itself — the Security Execution Gateway still decides reachable
/// paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub description: String,
    pub root_path: Option<String>,
    pub status: WorkspaceStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Workspace {
    /// Create a new active workspace with the given name/description.
    pub fn new(
        id: WorkspaceId,
        name: String,
        description: String,
        root_path: Option<String>,
        now: i64,
    ) -> Result<Self, WorkspaceFieldError> {
        validate_workspace_fields(&name, &description)?;
        Ok(Self {
            id,
            name: name.trim().to_string(),
            description: description.trim().to_string(),
            root_path,
            status: WorkspaceStatus::Active,
            created_at: now,
            updated_at: now,
        })
    }
}

/// Validation errors for workspace fields.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkspaceFieldError {
    #[error("workspace id is invalid")]
    InvalidId,
    #[error("workspace name must not be empty")]
    EmptyName,
    #[error("workspace name exceeds 120 characters")]
    NameTooLong,
    #[error("workspace description exceeds 4000 characters")]
    DescriptionTooLong,
}
