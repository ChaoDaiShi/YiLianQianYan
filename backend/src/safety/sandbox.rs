use std::path::{Component, Path, PathBuf};

use crate::config::types::{SandboxConfig, SandboxProfile};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SandboxPathError {
    #[error("workspace root must not be empty")]
    EmptyWorkspaceRoot,
    #[error("failed to resolve current directory: {0}")]
    CurrentDirectory(String),
    #[error("failed to canonicalize path: {0}")]
    CanonicalizeFailed(String),
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

/// Resolve a write target path against the real filesystem, following
/// symlinks / junctions / reparse points so that containment checks are
/// not bypassed through indirection.
///
/// - If the full normalized path exists, canonicalize it directly.
/// - If only ancestor directories exist, canonicalize the nearest
///   existing parent and re-join the remainder.
/// - If nothing exists up to the root, fail closed.
pub fn resolve_write_target(
    workspace_root: &Path,
    target: &Path,
) -> Result<PathBuf, SandboxPathError> {
    let normalized = normalize_path(workspace_root, target)?;

    if normalized.exists() {
        return std::fs::canonicalize(&normalized)
            .map_err(|error| SandboxPathError::CanonicalizeFailed(error.to_string()));
    }

    let mut current = normalized.clone();
    let mut remainder = PathBuf::new();

    loop {
        if current.exists() {
            let canonical_parent = std::fs::canonicalize(&current)
                .map_err(|error| SandboxPathError::CanonicalizeFailed(error.to_string()))?;
            return Ok(canonical_parent.join(&remainder));
        }

        match current.file_name().map(|name| name.to_os_string()) {
            Some(name) if !name.is_empty() => {
                remainder = PathBuf::from(&name).join(&remainder);
                if !current.pop() {
                    break;
                }
            }
            _ => break,
        }
    }

    Err(SandboxPathError::CanonicalizeFailed(
        "cannot resolve any existing parent directory for the write target".to_string(),
    ))
}

/// Apply the configured sandbox profile to a write target.
///
/// Both the target path and every boundary path (root, denied paths,
/// writable paths) are resolved against the real filesystem via
/// [`resolve_write_target`] so symlink / junction escapes are caught.
///
/// Explicit denied paths always take precedence over the selected profile.
pub fn can_write(
    config: &SandboxConfig,
    workspace_root: &Path,
    target: &Path,
) -> Result<bool, SandboxPathError> {
    let workspace_base = resolve_workspace_base(workspace_root)?;
    let real_root = resolve_write_target(&workspace_base, &workspace_base)?;
    let real_target = resolve_write_target(&workspace_base, target)?;

    // denied_write_paths always have highest priority
    for denied in &config.denied_write_paths {
        let real_denied = resolve_write_target(&workspace_base, &PathBuf::from(denied))?;
        if is_within_root(&real_denied, &real_target) {
            return Ok(false);
        }
    }

    match &config.profile {
        SandboxProfile::ReadOnly => Ok(false),
        SandboxProfile::WorkspaceWrite => Ok(is_within_root(&real_root, &real_target)),
        SandboxProfile::Custom => Ok(config.writable_paths.iter().any(|writable| {
            resolve_write_target(&workspace_base, &PathBuf::from(writable))
                .map(|real_writable| is_within_root(&real_writable, &real_target))
                .unwrap_or(false)
        })),
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

    // ── resolve_write_target tests ──

    #[test]
    fn resolve_write_target_existing_file() {
        let dir =
            std::env::temp_dir().join(format!("yilian-resolve-existing-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("real.txt");
        std::fs::write(&file, "hello").unwrap();

        let resolved = super::resolve_write_target(&dir, Path::new("real.txt")).unwrap();
        let expected = std::fs::canonicalize(&file).unwrap();
        assert_eq!(resolved, expected);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_write_target_new_file() {
        let dir = std::env::temp_dir().join(format!("yilian-resolve-new-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let resolved = super::resolve_write_target(&dir, Path::new("new/file.txt")).unwrap();
        let expected = std::fs::canonicalize(&dir)
            .unwrap()
            .join("new")
            .join("file.txt");
        assert_eq!(resolved, expected);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_write_target_fails_closed_on_completely_nonexistent_root() {
        // On Windows: use a non-existent drive letter whose root directory
        // cannot be canonicalized.
        // On Unix: canonicalize succeeds because "/" always exists, so we
        // verify fail-closed behaviour through `can_write` instead (see
        // `canonicalize_failure_is_not_allowed`).
        #[cfg(windows)]
        {
            let result =
                super::resolve_write_target(Path::new("."), Path::new("Q:\\nonexistent\\file.txt"));
            assert!(result.is_err());
        }
    }

    // ── can_write with real filesystem resolution ──

    #[test]
    fn can_write_allows_existing_workspace_file_with_real_paths() {
        let dir =
            std::env::temp_dir().join(format!("yilian-can-write-real-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("notes.txt");
        std::fs::write(&file, "hello").unwrap();

        let config = sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]);
        assert!(can_write(&config, &dir, Path::new("notes.txt")).unwrap());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn can_write_allows_new_file_with_real_workspace() {
        let dir =
            std::env::temp_dir().join(format!("yilian-new-file-real-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let config = sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]);
        assert!(can_write(&config, &dir, Path::new("new/subdir/file.txt")).unwrap());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn can_write_denies_dotdot_escape_with_real_paths() {
        let dir = std::env::temp_dir().join(format!("yilian-dotdot-real-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let config = sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]);
        assert!(!can_write(&config, &dir, Path::new("../outside.txt")).unwrap());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ── Symlink escape tests ──

    /// Helper: create a symlink (dir) on Unix or Windows. Returns Ok(()) or skips.
    fn create_dir_symlink(original: &Path, link: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(original, link)
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(original, link)
        }
    }

    #[test]
    fn symlink_inside_workspace_to_inside_is_allowed() {
        let base =
            std::env::temp_dir().join(format!("yilian-symlink-inside-{}", uuid::Uuid::new_v4()));
        let workspace = base.join("workspace");
        std::fs::create_dir_all(workspace.join("real")).unwrap();
        // workspace/link → workspace/real
        let link = workspace.join("link");
        let result = create_dir_symlink(&workspace.join("real"), &link);
        if result.is_err() {
            eprintln!("skipping symlink test: cannot create symlink ({result:?})");
            std::fs::remove_dir_all(&base).ok();
            return;
        }

        // Writing to workspace/link/file.txt should be allowed (resolves inside workspace)
        let config = sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]);
        assert!(can_write(&config, &workspace, Path::new("link/file.txt")).unwrap());

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn symlink_inside_workspace_to_outside_is_denied() {
        let base =
            std::env::temp_dir().join(format!("yilian-symlink-outside-{}", uuid::Uuid::new_v4()));
        let workspace = base.join("workspace");
        let outside = base.join("outside");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        // workspace/link → base/outside (external to workspace)
        let link = workspace.join("link");
        let result = create_dir_symlink(&outside, &link);
        if result.is_err() {
            eprintln!("skipping symlink test: cannot create symlink ({result:?})");
            std::fs::remove_dir_all(&base).ok();
            return;
        }

        let config = sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]);
        assert!(!can_write(&config, &workspace, Path::new("link/file.txt")).unwrap());

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn symlink_inside_custom_writable_to_outside_is_denied() {
        let base =
            std::env::temp_dir().join(format!("yilian-symlink-custom-{}", uuid::Uuid::new_v4()));
        let workspace = base.join("workspace");
        let outside = base.join("outside");
        std::fs::create_dir_all(workspace.join("writable")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        // workspace/writable/link → base/outside
        let link = workspace.join("writable").join("link");
        let result = create_dir_symlink(&outside, &link);
        if result.is_err() {
            eprintln!("skipping symlink test: cannot create symlink ({result:?})");
            std::fs::remove_dir_all(&base).ok();
            return;
        }

        let config = sandbox_config(SandboxProfile::Custom, &["writable"], &[]);
        assert!(!can_write(&config, &workspace, Path::new("writable/link/file.txt")).unwrap());

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn denied_path_accessed_via_symlink_is_denied() {
        let base =
            std::env::temp_dir().join(format!("yilian-symlink-denied-{}", uuid::Uuid::new_v4()));
        let workspace = base.join("workspace");
        let secret_dir = workspace.join("secret");
        std::fs::create_dir_all(&secret_dir).unwrap();
        std::fs::write(secret_dir.join("key.txt"), "classified").unwrap();
        // workspace/link → workspace/secret
        let link = workspace.join("link");
        let result = create_dir_symlink(&secret_dir, &link);
        if result.is_err() {
            eprintln!("skipping symlink test: cannot create symlink ({result:?})");
            std::fs::remove_dir_all(&base).ok();
            return;
        }

        // Deny writes to workspace/secret but allow workspace/*
        let config = sandbox_config(SandboxProfile::WorkspaceWrite, &[], &["secret"]);
        // Accessing secret/key.txt via symlink workspace/link/key.txt should still be denied
        assert!(!can_write(&config, &workspace, Path::new("link/key.txt")).unwrap());

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn canonicalize_failure_is_not_allowed() {
        // A path whose parent chain cannot be resolved should fail closed.
        // On Windows, a non-existent drive letter triggers this.
        #[cfg(windows)]
        {
            let result = can_write(
                &sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]),
                Path::new("Q:\\nonexistent\\workspace"),
                Path::new("file.txt"),
            );
            assert!(result.is_err());
        }
        #[cfg(not(windows))]
        {
            let result = can_write(
                &sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]),
                Path::new("/nonexistent/deep/path/workspace"),
                Path::new("file.txt"),
            );
            assert!(result.is_err());
        }
    }
}
