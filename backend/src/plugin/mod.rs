// ============================================================
// Plugin manifests and explicitly controlled external service lifecycle.
//
//   manifest.rs — plugin.json model + validation + limits + secret rejection
//   path.rs     — contribution path containment (no ../ / absolute / symlink escape)
//   registry.rs — scan/parse/validate + enable/disable (never executes)
//
// A plugin is a declarative package of data contributions (agents, skills,
// workflows, MCP templates, services) and requested permissions. It never loads
// native code into the host and never grants RBAC.
// ============================================================

pub(crate) mod cloud_auth;
pub mod manifest;
pub mod path;
pub mod registry;
pub mod service;
mod service_process;
pub mod service_runtime;
pub mod settings;

#[cfg(test)]
mod tests;

pub use manifest::{
    parse_manifest, validate_manifest, ManifestError, McpServerTemplate, PluginContributions,
    PluginId, PluginManifest, MAX_PLUGIN_DESCRIPTION_CHARS, MAX_PLUGIN_MANIFEST_BYTES,
    MAX_PLUGIN_PERMISSIONS,
};
pub use path::{validate_contribution_path, PluginPathError};
pub use registry::{PluginRecord, PluginRefreshReport, PluginRegistry, PluginStatus};
