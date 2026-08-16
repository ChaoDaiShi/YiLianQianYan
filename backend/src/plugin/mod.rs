// ============================================================
// Plugin Manifest Foundation — declarative plugins only.
//
//   manifest.rs — plugin.json model + validation + limits + secret rejection
//   path.rs     — contribution path containment (no ../ / absolute / symlink escape)
//   registry.rs — scan/parse/validate + enable/disable (never executes)
//
// A plugin is a declarative package of data contributions (agents, skills,
// workflows, MCP templates) and requested permissions. It never loads native
// code and never grants RBAC.
// ============================================================

pub mod manifest;
pub mod path;
pub mod registry;

#[cfg(test)]
mod tests;

pub use manifest::{
    parse_manifest, validate_manifest, ManifestError, McpServerTemplate, PluginContributions,
    PluginId, PluginManifest, MAX_PLUGIN_DESCRIPTION_CHARS, MAX_PLUGIN_MANIFEST_BYTES,
    MAX_PLUGIN_PERMISSIONS,
};
pub use path::{validate_contribution_path, PluginPathError};
pub use registry::{PluginRecord, PluginRefreshReport, PluginRegistry, PluginStatus};
