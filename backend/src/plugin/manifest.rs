// ============================================================
// Plugin manifest — declarative plugin.json model + validation.
//
// A plugin is a *declarative package*: it describes data contributions (agents,
// skills, workflows, MCP templates) and requested permissions. It can never
// load native code, eval JS, exec Python, or run arbitrary binaries.
// ============================================================

use serde::{Deserialize, Serialize};

pub const MAX_PLUGIN_MANIFEST_BYTES: usize = 256 * 1024;
pub const MAX_PLUGIN_ID_CHARS: usize = 128;
pub const MAX_PLUGIN_DESCRIPTION_CHARS: usize = 4000;
pub const MAX_PLUGIN_PERMISSIONS: usize = 64;
pub const MAX_PLUGIN_CONTRIBUTIONS_PER_KIND: usize = 100;

/// A validated plugin identifier (ASCII lowercase + `.` `-` `_`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PluginId(String);

impl PluginId {
    pub fn new(raw: impl Into<String>) -> Result<Self, ManifestError> {
        let raw = raw.into();
        if raw.is_empty() || raw.len() > MAX_PLUGIN_ID_CHARS {
            return Err(ManifestError::InvalidId);
        }
        if raw.chars().any(|c| {
            !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-' || c == '_')
        }) {
            return Err(ManifestError::InvalidId);
        }
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PluginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A declarative MCP server template. Never auto-connected; the user must
/// explicitly import + enable it. Only environment-variable *names* are allowed
/// (never actual secret values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServerTemplate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub transport: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginContributions {
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub workflows: Vec<String>,
    #[serde(default)]
    pub mcp_server_templates: Vec<McpServerTemplate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub contributes: PluginContributions,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("plugin manifest exceeds the size limit")]
    Oversized,
    #[error("plugin manifest is not valid JSON")]
    InvalidJson,
    #[error("plugin id is invalid")]
    InvalidId,
    #[error("plugin name must not be empty")]
    EmptyName,
    #[error("plugin schema version unsupported")]
    UnsupportedSchema,
    #[error("plugin description exceeds {0} characters")]
    DescriptionTooLong(usize),
    #[error("plugin has too many permissions")]
    TooManyPermissions,
    #[error("plugin has too many contributions")]
    TooManyContributions,
    #[error("plugin manifest appears to contain a secret")]
    ContainsSecret,
}

/// Parse and validate a plugin.json payload.
pub fn parse_manifest(raw: &str) -> Result<PluginManifest, ManifestError> {
    if raw.len() > MAX_PLUGIN_MANIFEST_BYTES {
        return Err(ManifestError::Oversized);
    }
    let manifest: PluginManifest =
        serde_json::from_str(raw).map_err(|_| ManifestError::InvalidJson)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn validate_manifest(manifest: &PluginManifest) -> Result<(), ManifestError> {
    // Re-validate the id field itself.
    PluginId::new(manifest.id.clone())?;
    if manifest.schema_version != 1 {
        return Err(ManifestError::UnsupportedSchema);
    }
    if manifest.name.trim().is_empty() {
        return Err(ManifestError::EmptyName);
    }
    if manifest.description.chars().count() > MAX_PLUGIN_DESCRIPTION_CHARS {
        return Err(ManifestError::DescriptionTooLong(
            MAX_PLUGIN_DESCRIPTION_CHARS,
        ));
    }
    if manifest.permissions.len() > MAX_PLUGIN_PERMISSIONS {
        return Err(ManifestError::TooManyPermissions);
    }
    if manifest.contributes.agents.len() > MAX_PLUGIN_CONTRIBUTIONS_PER_KIND
        || manifest.contributes.skills.len() > MAX_PLUGIN_CONTRIBUTIONS_PER_KIND
        || manifest.contributes.workflows.len() > MAX_PLUGIN_CONTRIBUTIONS_PER_KIND
        || manifest.contributes.mcp_server_templates.len() > MAX_PLUGIN_CONTRIBUTIONS_PER_KIND
    {
        return Err(ManifestError::TooManyContributions);
    }
    // Secret check on *descriptive* fields only. MCP template `env` values are
    // environment-variable *names* by design, so they are not scanned here.
    if manifest_contains_secret(manifest) {
        return Err(ManifestError::ContainsSecret);
    }
    Ok(())
}

fn manifest_contains_secret(manifest: &PluginManifest) -> bool {
    crate::safety::contains_sensitive_content(&manifest.description)
        || crate::safety::contains_sensitive_content(&manifest.name)
        || manifest
            .publisher
            .as_deref()
            .is_some_and(crate::safety::contains_sensitive_content)
}
