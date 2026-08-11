use std::path::{Component, Path, PathBuf};

use crate::config::types::{SandboxConfig, SandboxProfile};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SandboxPathError {
    #[error("workspace root must not be empty")]
    EmptyWorkspaceRoot,
    #[error("failed to resolve current directory: {0}")]
    CurrentDirectory(String),
}

/// Resolve a path relative to `workspace_root` and normalize `.`/`..`
/// components without requiring the target to exist on disk.
pub fn normalize_path(workspace_root: &Path, input: &Path) -> Result<PathBuf, SandboxPathError> {
    if workspace_root.as_os_str().is_empty() {
        return Err(SandboxPathError::EmptyWorkspaceRoot);
    }

    let combined = if input.is_absolute() {
        input.to_path_buf()
    } else {
        workspace_root.join(input)
    };

    Ok(normalize_components(&combined))
}

/// Return whether `target` is inside `root` using path-component boundaries.
/// Both paths should already be normalized with [`normalize_path`].
pub fn is_within_root(root: &Path, target: &Path) -> bool {
    if root.as_os_str().is_empty() {
        return false;
    }

    let mut root_components = root.components();
    let mut target_components = target.components();

    loop {
        match root_components.next() {
            Some(root_component) => match target_components.next() {
                Some(target_component)
                    if path_components_equal(&root_component, &target_component) => {}
                _ => return false,
            },
            None => return true,
        }
    }
}

#[cfg(windows)]
fn path_components_equal(left: &Component<'_>, right: &Component<'_>) -> bool {
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
}

#[cfg(not(windows))]
fn path_components_equal(left: &Component<'_>, right: &Component<'_>) -> bool {
    left == right
}

/// Check whether a path is writable under the workspace-write boundary.
/// The target does not need to exist on disk.
pub fn can_write_workspace(workspace_root: &Path, target: &Path) -> Result<bool, SandboxPathError> {
    let workspace_base = resolve_workspace_base(workspace_root)?;
    let normalized_root = normalize_path(&workspace_base, Path::new("."))?;
    let normalized_target = normalize_path(&workspace_base, target)?;

    Ok(is_within_root(&normalized_root, &normalized_target))
}

/// Return whether a target is inside any denied write path.
pub fn is_denied_write_path(
    workspace_root: &Path,
    target: &Path,
    denied_paths: &[PathBuf],
) -> Result<bool, SandboxPathError> {
    let workspace_base = resolve_workspace_base(workspace_root)?;
    let normalized_target = normalize_path(&workspace_base, target)?;

    for denied_path in denied_paths {
        let normalized_denied = normalize_path(&workspace_base, denied_path)?;
        if is_within_root(&normalized_denied, &normalized_target) {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Return whether a target is inside any configured writable path.
pub fn is_writable_path(
    workspace_root: &Path,
    target: &Path,
    writable_paths: &[PathBuf],
) -> Result<bool, SandboxPathError> {
    let workspace_base = resolve_workspace_base(workspace_root)?;
    let normalized_target = normalize_path(&workspace_base, target)?;

    for writable_path in writable_paths {
        let normalized_writable = normalize_path(&workspace_base, writable_path)?;
        if is_within_root(&normalized_writable, &normalized_target) {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Apply the configured sandbox profile to a write target.
///
/// Explicit denied paths always take precedence over the selected profile.
pub fn can_write(
    config: &SandboxConfig,
    workspace_root: &Path,
    target: &Path,
) -> Result<bool, SandboxPathError> {
    let denied_paths = config
        .denied_write_paths
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if is_denied_write_path(workspace_root, target, &denied_paths)? {
        return Ok(false);
    }

    match &config.profile {
        SandboxProfile::ReadOnly => Ok(false),
        SandboxProfile::WorkspaceWrite => can_write_workspace(workspace_root, target),
        SandboxProfile::Custom => {
            let writable_paths = config
                .writable_paths
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            is_writable_path(workspace_root, target, &writable_paths)
        }
        SandboxProfile::Open => Ok(true),
    }
}

fn resolve_workspace_base(workspace_root: &Path) -> Result<PathBuf, SandboxPathError> {
    if workspace_root.as_os_str().is_empty() {
        return Err(SandboxPathError::EmptyWorkspaceRoot);
    }

    if workspace_root.is_absolute() {
        Ok(workspace_root.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|error| SandboxPathError::CurrentDirectory(error.to_string()))?
            .join(workspace_root))
    }
}

fn normalize_components(path: &Path) -> PathBuf {
    let is_absolute = path.is_absolute();
    let mut normalized = PathBuf::new();
    let mut normal_component_count = 0usize;

    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::Normal(value) => {
                normalized.push(value);
                normal_component_count += 1;
            }
            Component::ParentDir => {
                if normal_component_count > 0 {
                    normalized.pop();
                    normal_component_count -= 1;
                } else if !is_absolute {
                    normalized.push(component.as_os_str());
                }
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        normalized.push(".");
    }

    normalized
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::config::types::{SandboxConfig, SandboxProfile};

    use super::{
        can_write, can_write_workspace, is_denied_write_path, is_within_root, is_writable_path,
        normalize_path,
    };

    fn sandbox_config(
        profile: SandboxProfile,
        writable_paths: &[&str],
        denied_write_paths: &[&str],
    ) -> SandboxConfig {
        SandboxConfig {
            profile,
            writable_paths: writable_paths
                .iter()
                .map(|path| (*path).to_string())
                .collect(),
            denied_write_paths: denied_write_paths
                .iter()
                .map(|path| (*path).to_string())
                .collect(),
        }
    }

    #[test]
    fn normalizes_workspace_relative_file() {
        let root = Path::new("workspace");
        let target = normalize_path(root, Path::new("README.md")).unwrap();

        assert_eq!(target, root.join("README.md"));
        assert!(is_within_root(root, &target));
    }

    #[test]
    fn resolves_parent_segments_without_escaping_workspace() {
        let root = Path::new("workspace");
        let target = normalize_path(root, Path::new("docs/../README.md")).unwrap();

        assert_eq!(target, root.join("README.md"));
        assert!(is_within_root(root, &target));
    }

    #[test]
    fn detects_parent_escape_after_normalization() {
        let root = Path::new("workspace");
        let target = normalize_path(root, Path::new("../../outside.txt")).unwrap();

        assert!(!is_within_root(root, &target));
    }

    #[test]
    fn distinguishes_similar_prefix_directories() {
        let root = Path::new("workspace");
        let target = PathBuf::from("workspace-evil").join("README.md");

        assert!(!is_within_root(root, &target));
    }

    #[test]
    fn rejects_empty_root() {
        assert!(!is_within_root(Path::new(""), Path::new("README.md")));
    }

    #[test]
    fn normalizes_absolute_path_without_requiring_target_to_exist() {
        let root = std::env::temp_dir().join("yilian-sandbox-root");
        let target = root.join("new-file.txt");

        let normalized = normalize_path(&root, &target).unwrap();

        assert_eq!(normalized, target);
    }

    #[test]
    fn allows_existing_workspace_file_path() {
        let root = Path::new("workspace");

        assert!(can_write_workspace(root, Path::new("README.md")).unwrap());
    }

    #[test]
    fn allows_workspace_relative_to_current_directory() {
        assert!(can_write_workspace(Path::new("."), Path::new("README.md")).unwrap());
    }

    #[test]
    fn allows_absolute_path_inside_relative_workspace() {
        let root = Path::new("workspace");
        let target = std::env::current_dir()
            .unwrap()
            .join(root)
            .join("new/file.txt");

        assert!(can_write_workspace(root, &target).unwrap());
    }

    #[test]
    fn allows_new_workspace_file_path_without_filesystem_access() {
        let root = Path::new("workspace");

        assert!(can_write_workspace(root, Path::new("new/file.txt")).unwrap());
    }

    #[test]
    fn rejects_parent_escape_for_workspace_write() {
        let root = Path::new("workspace");

        assert!(!can_write_workspace(root, Path::new("../outside.txt")).unwrap());
    }

    #[test]
    fn rejects_absolute_path_outside_workspace() {
        let root = std::env::temp_dir().join("yilian-workspace-write-root");
        let outside = std::env::temp_dir()
            .join("yilian-workspace-write-outside")
            .join("file.txt");

        assert!(!can_write_workspace(&root, &outside).unwrap());
    }

    #[test]
    fn rejects_similar_workspace_prefix_directory() {
        let root = std::env::temp_dir().join("workspace");
        let target = std::env::temp_dir().join("workspace-evil").join("file.txt");

        assert!(!can_write_workspace(&root, &target).unwrap());
    }

    #[test]
    fn denies_directory_descendants() {
        let root = Path::new("workspace");
        let denied = [PathBuf::from(".git")];

        assert!(is_denied_write_path(root, Path::new(".git/config"), &denied).unwrap());
    }

    #[test]
    fn denies_exact_file_path() {
        let root = Path::new("workspace");
        let denied = [PathBuf::from("secrets/key.txt")];

        assert!(is_denied_write_path(root, Path::new("secrets/key.txt"), &denied).unwrap());
    }

    #[test]
    fn allows_unlisted_workspace_file() {
        let root = Path::new("workspace");
        let denied = [PathBuf::from("secrets")];

        assert!(!is_denied_write_path(root, Path::new("src/main.rs"), &denied).unwrap());
    }

    #[test]
    fn does_not_deny_similar_prefix_directory() {
        let root = Path::new("workspace");
        let denied = [PathBuf::from("secret")];

        assert!(!is_denied_write_path(root, Path::new("secret-copy/a.txt"), &denied).unwrap());
    }

    #[test]
    fn resolves_relative_denied_path_against_workspace_root() {
        let root = std::env::temp_dir().join("yilian-denied-root");
        let target = root.join(".git").join("config");
        let denied = [PathBuf::from(".git")];

        assert!(is_denied_write_path(&root, &target, &denied).unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn denies_windows_case_variant_path() {
        let root = std::env::temp_dir().join("yilian-denied-case-root");
        let target = root.join(".GIT").join("config");
        let denied = [PathBuf::from(".git")];

        assert!(is_denied_write_path(&root, &target, &denied).unwrap());
    }

    #[test]
    fn allows_target_inside_writable_directory() {
        let root = Path::new("workspace");
        let writable = [PathBuf::from("src")];

        assert!(is_writable_path(root, Path::new("src/main.rs"), &writable).unwrap());
    }

    #[test]
    fn allows_new_file_inside_writable_directory() {
        let root = Path::new("workspace");
        let writable = [PathBuf::from("src")];

        assert!(is_writable_path(root, Path::new("src/new.rs"), &writable).unwrap());
    }

    #[test]
    fn rejects_target_outside_writable_directories() {
        let root = Path::new("workspace");
        let writable = [PathBuf::from("src"), PathBuf::from("docs")];

        assert!(!is_writable_path(root, Path::new("tests/test.rs"), &writable).unwrap());
    }

    #[test]
    fn rejects_similar_writable_prefix_directory() {
        let root = std::env::temp_dir().join("yilian-writable-root");
        let writable = [PathBuf::from("src")];
        let target = root.join("src-old").join("main.rs");

        assert!(!is_writable_path(&root, &target, &writable).unwrap());
    }

    #[test]
    fn resolves_relative_writable_path_against_workspace_root() {
        let root = std::env::temp_dir().join("yilian-writable-relative-root");
        let target = root.join("docs").join("readme.md");
        let writable = [PathBuf::from("docs")];

        assert!(is_writable_path(&root, &target, &writable).unwrap());
    }

    #[test]
    fn read_only_profile_rejects_workspace_file() {
        let config = sandbox_config(SandboxProfile::ReadOnly, &[], &[]);

        assert!(!can_write(&config, Path::new("workspace"), Path::new("README.md")).unwrap());
    }

    #[test]
    fn workspace_write_profile_allows_workspace_file() {
        let config = sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]);

        assert!(can_write(&config, Path::new("workspace"), Path::new("README.md")).unwrap());
    }

    #[test]
    fn workspace_write_profile_rejects_outside_file() {
        let config = sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]);
        let workspace_root = std::env::temp_dir().join("yilian-can-write-root");
        let outside = std::env::temp_dir()
            .join("yilian-can-write-outside")
            .join("file.txt");

        assert!(!can_write(&config, &workspace_root, &outside).unwrap());
    }

    #[test]
    fn custom_profile_allows_writable_path() {
        let config = sandbox_config(SandboxProfile::Custom, &["src"], &[]);

        assert!(can_write(&config, Path::new("workspace"), Path::new("src/main.rs")).unwrap());
    }

    #[test]
    fn custom_profile_rejects_outside_writable_path() {
        let config = sandbox_config(SandboxProfile::Custom, &["src"], &[]);

        assert!(!can_write(&config, Path::new("workspace"), Path::new("tests/test.rs")).unwrap());
    }

    #[test]
    fn custom_profile_denied_path_overrides_writable_path() {
        let config = sandbox_config(SandboxProfile::Custom, &["src"], &["src/private"]);

        assert!(!can_write(
            &config,
            Path::new("workspace"),
            Path::new("src/private/file.rs")
        )
        .unwrap());
    }

    #[test]
    fn open_profile_allows_ordinary_path() {
        let config = sandbox_config(SandboxProfile::Open, &[], &[]);

        assert!(can_write(
            &config,
            Path::new("workspace"),
            Path::new("anywhere/file.txt")
        )
        .unwrap());
    }

    #[test]
    fn open_profile_still_rejects_denied_path() {
        let config = sandbox_config(SandboxProfile::Open, &[], &[".git"]);

        assert!(!can_write(&config, Path::new("workspace"), Path::new(".git/config")).unwrap());
    }
}
