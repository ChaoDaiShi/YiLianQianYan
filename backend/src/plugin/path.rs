// ============================================================
// Plugin path containment — all contribution paths must resolve inside the
// plugin root, be relative, and never escape via `..` or symlinks.
// ============================================================

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PluginPathError {
    #[error("contribution path must be relative")]
    NotRelative,
    #[error("contribution path escapes the plugin root")]
    OutsideRoot,
    #[error("contribution path does not exist")]
    Missing,
    #[error("contribution path traverses a symlink outside the plugin root")]
    SymlinkEscape,
}

/// Validate a relative contribution path against a plugin root.
///
/// The path must be relative (no absolute path, no `..`), must canonicalize to
/// inside `root`, and must not traverse a symlink outside `root`.
pub fn validate_contribution_path(root: &Path, relative: &str) -> Result<PathBuf, PluginPathError> {
    let path = Path::new(relative);
    if path.is_absolute() {
        return Err(PluginPathError::NotRelative);
    }
    if path.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(PluginPathError::NotRelative);
    }

    let joined = root.join(path);
    let root_abs = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let target_abs = joined
        .canonicalize()
        .map_err(|_| PluginPathError::Missing)?;
    if !target_abs.starts_with(&root_abs) {
        return Err(PluginPathError::OutsideRoot);
    }
    Ok(target_abs)
}
