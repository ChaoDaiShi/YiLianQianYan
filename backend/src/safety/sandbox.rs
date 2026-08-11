use std::path::{Component, Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SandboxPathError {
    #[error("workspace root must not be empty")]
    EmptyWorkspaceRoot,
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

    use super::{is_within_root, normalize_path};

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
}
