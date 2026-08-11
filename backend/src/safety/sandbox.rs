use std::path::{Component, Path, PathBuf};

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
                Some(target_component) if target_component == root_component => {}
                _ => return false,
            },
            None => return true,
        }
    }
}

/// Check whether a path is writable under the workspace-write boundary.
/// The target does not need to exist on disk.
pub fn can_write_workspace(workspace_root: &Path, target: &Path) -> Result<bool, SandboxPathError> {
    if workspace_root.as_os_str().is_empty() {
        return Err(SandboxPathError::EmptyWorkspaceRoot);
    }

    let workspace_base = if workspace_root.is_absolute() {
        workspace_root.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| SandboxPathError::CurrentDirectory(error.to_string()))?
            .join(workspace_root)
    };
    let normalized_root = normalize_path(&workspace_base, Path::new("."))?;
    let normalized_target = normalize_path(&workspace_base, target)?;

    Ok(is_within_root(&normalized_root, &normalized_target))
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

    use super::{can_write_workspace, is_within_root, normalize_path};

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
}
