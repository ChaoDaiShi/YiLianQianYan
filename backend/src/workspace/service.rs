// ============================================================
// Workspace service — field validation helpers.
// ============================================================

use super::model::WorkspaceFieldError;

pub const MAX_WORKSPACE_NAME_CHARS: usize = 120;
pub const MAX_WORKSPACE_DESCRIPTION_CHARS: usize = 4000;

/// Validate workspace name/description bounds. Returns the trimmed name so
/// callers can persist the canonical form.
pub fn validate_workspace_fields(
    name: &str,
    description: &str,
) -> Result<String, WorkspaceFieldError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(WorkspaceFieldError::EmptyName);
    }
    if name.chars().count() > MAX_WORKSPACE_NAME_CHARS {
        return Err(WorkspaceFieldError::NameTooLong);
    }
    if description.trim().chars().count() > MAX_WORKSPACE_DESCRIPTION_CHARS {
        return Err(WorkspaceFieldError::DescriptionTooLong);
    }
    Ok(name.to_string())
}
