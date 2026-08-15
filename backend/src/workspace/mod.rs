// ============================================================
// Workspace domain — the top-level project container for Tasks.
//
// A Workspace groups Tasks, Artifacts, and Timeline history under one named
// project. It is deliberately NOT an execution unit: Workspaces have no
// running state, only Active / Archived lifecycle. Security is never implied
// by a Workspace root path — the Security Execution Gateway remains the only
// arbiter of what filesystem paths are reachable.
// ============================================================

pub mod model;
pub mod service;

#[cfg(test)]
mod tests;

pub use model::{Workspace, WorkspaceFieldError, WorkspaceId, WorkspaceStatus};
pub use service::validate_workspace_fields;
