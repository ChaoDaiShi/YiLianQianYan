// ============================================================
// Artifact service — bounded artifact registration + path safety.
//
// An Artifact is metadata only, never a persistent filesystem grant. Paths are
// validated (canonicalized, no `..` escape, inside the workspace root when one
// is provided). The Security Gateway still governs real file access.
// ============================================================

use std::path::Path;

use crate::db::Database;
use crate::task::model::{Artifact, ArtifactId, ArtifactType, TaskExecutionId, TaskId};
use crate::utils::text::truncate_chars;
use crate::workspace::WorkspaceId;

pub const MAX_ARTIFACT_NAME_CHARS: usize = 200;
pub const MAX_ARTIFACT_SUMMARY_CHARS: usize = 8000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArtifactError {
    #[error("artifact name must not be empty")]
    EmptyName,
    #[error("artifact name exceeds 200 characters")]
    NameTooLong,
    #[error("artifact summary exceeds 8000 characters")]
    SummaryTooLong,
    #[error("artifact path escapes the allowed workspace root")]
    PathOutsideRoot,
    #[error("artifact persistence failed: {0}")]
    Db(String),
}

#[derive(Clone)]
pub struct ArtifactService {
    db: Database,
}

impl ArtifactService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Validate an artifact path against a workspace root (when provided).
    ///
    /// The path is canonicalized and must resolve inside the root; `..`
    /// traversal and symlink escapes are rejected. `None` root disables the
    /// containment check (the Security Gateway still enforces real access).
    pub fn validate_path(path: &str, workspace_root: Option<&str>) -> Result<(), ArtifactError> {
        let Some(root) = workspace_root else {
            return Ok(());
        };
        let root = Path::new(root);
        let target = Path::new(path);
        let root_abs = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let target_abs = target
            .canonicalize()
            .unwrap_or_else(|_| target.to_path_buf());
        if !target_abs.starts_with(&root_abs) {
            return Err(ArtifactError::PathOutsideRoot);
        }
        Ok(())
    }

    /// Register a bounded artifact. Path containment is validated when a
    /// workspace root is supplied.
    pub fn register(
        &self,
        workspace_id: &WorkspaceId,
        task_id: &TaskId,
        task_execution_id: &TaskExecutionId,
        name: impl Into<String>,
        artifact_type: ArtifactType,
        path: Option<String>,
        mime_type: Option<String>,
        size: Option<u64>,
        summary: impl Into<String>,
        workspace_root: Option<&str>,
        now: i64,
    ) -> Result<Artifact, ArtifactError> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(ArtifactError::EmptyName);
        }
        if name.chars().count() > MAX_ARTIFACT_NAME_CHARS {
            return Err(ArtifactError::NameTooLong);
        }
        let summary = truncate_chars(summary.into().as_str(), MAX_ARTIFACT_SUMMARY_CHARS);
        if let Some(path) = &path {
            Self::validate_path(path, workspace_root)?;
        }
        let artifact = Artifact {
            id: ArtifactId::generate(),
            workspace_id: workspace_id.clone(),
            task_id: task_id.clone(),
            task_execution_id: task_execution_id.clone(),
            name,
            artifact_type,
            path,
            mime_type,
            size,
            summary,
            created_at: now,
            updated_at: now,
        };
        self.db
            .create_artifact(&artifact)
            .map_err(ArtifactError::Db)?;
        Ok(artifact)
    }
}
