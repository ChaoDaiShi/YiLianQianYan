//! Trusted storage scope resolved from the active account database, never browser payloads.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::BrowserError;

/// Immutable account-profile location supplied by backend bootstrap. Scope leases
/// revoke access during account changes; they are not disk identity across restarts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserProfileContext {
    profile_directory: PathBuf,
}

impl BrowserProfileContext {
    pub(crate) fn from_database_path(db_path: &Path) -> Result<Self, BrowserError> {
        if db_path == Path::new(":memory:") || db_path.file_name().is_none() {
            return Err(BrowserError::new(
                "browser_profile_unavailable",
                "Persistent browsing requires an active disk profile",
            ));
        }
        let parent = db_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let profile_directory = parent.canonicalize().map_err(|_| {
            BrowserError::new(
                "browser_profile_unavailable",
                "Active account profile directory is unavailable",
            )
        })?;
        Ok(Self { profile_directory })
    }

    pub fn profile_directory(&self) -> &Path {
        &self.profile_directory
    }

    /// Workspace authority is validated by BrowserRuntime before native dispatch.
    /// Names are hashed with a domain prefix so personal and project stores differ.
    pub fn directory_for_workspace(&self, workspace: Option<&str>) -> PathBuf {
        let mut digest = Sha256::new();
        match workspace {
            Some(workspace) => {
                digest.update(b"workspace\0");
                digest.update(workspace.as_bytes());
            }
            None => digest.update(b"personal\0"),
        }
        self.profile_directory
            .join(format!("b{}", base32_digest(&digest.finalize())))
    }
}

/// Keep the complete digest in a short, case-insensitive filesystem component.
fn base32_digest(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut encoded = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut buffer = 0u16;
    let mut bits = 0u32;
    for &byte in bytes {
        buffer = (buffer << 8) | u16::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            encoded.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
        buffer &= (1 << bits) - 1;
    }
    if bits > 0 {
        encoded.push(ALPHABET[(buffer << (5 - bits)) as usize] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("browser-profile-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn browser_profiles_survive_runtime_restart_and_isolate_account_and_workspace() {
        let root = account_root();
        let account_a = root.join("profiles/account-a");
        let account_b = root.join("profiles/account-b");
        std::fs::create_dir_all(&account_a).unwrap();
        std::fs::create_dir_all(&account_b).unwrap();
        let first = BrowserProfileContext::from_database_path(&account_a.join("app.db")).unwrap();
        let restarted =
            BrowserProfileContext::from_database_path(&account_a.join("app.db")).unwrap();
        let other = BrowserProfileContext::from_database_path(&account_b.join("app.db")).unwrap();
        let a_project = first.directory_for_workspace(Some("project-a"));
        assert_eq!(
            a_project,
            restarted.directory_for_workspace(Some("project-a"))
        );
        assert_ne!(a_project, first.directory_for_workspace(Some("project-b")));
        assert_ne!(a_project, other.directory_for_workspace(Some("project-a")));
        assert_ne!(
            first.directory_for_workspace(None),
            first.directory_for_workspace(Some("personal"))
        );
        assert!(a_project.starts_with(account_a.canonicalize().unwrap()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn browser_workspace_names_cannot_escape_account_profile_directory() {
        let root = account_root();
        std::fs::create_dir_all(&root).unwrap();
        let context = BrowserProfileContext::from_database_path(&root.join("app.db")).unwrap();
        let directory = context.directory_for_workspace(Some("../../other-account/secret"));
        assert_eq!(
            directory.parent(),
            Some(root.canonicalize().unwrap().as_path())
        );
        assert!(!directory
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("secret"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn browser_profile_components_are_compact_full_digest_base32() {
        let root = account_root();
        std::fs::create_dir_all(&root).unwrap();
        let context = BrowserProfileContext::from_database_path(&root.join("app.db")).unwrap();
        for (workspace, expected) in [
            (
                None,
                "b56wf434yipjjbwftwgwxgoahcqyadzitwlofm4hxausc3tnci6jq",
            ),
            (
                Some("project-a"),
                "b27mhwcgytu5kk4ehrde7dfiltdonepmzie2tjvddtjprrsaysviq",
            ),
        ] {
            let directory = context.directory_for_workspace(workspace);
            let component = directory.file_name().unwrap().to_str().unwrap();
            assert_eq!(directory.parent(), Some(context.profile_directory()));
            assert_eq!(component, expected);
            assert_eq!(component.len(), 53);
            assert!(component
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte)));
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn browser_profile_base32_matches_unpadded_vectors() {
        for (bytes, expected) in [
            (b"".as_slice(), ""),
            (b"f".as_slice(), "my"),
            (b"fo".as_slice(), "mzxq"),
            (b"foo".as_slice(), "mzxw6"),
            (b"foob".as_slice(), "mzxw6yq"),
            (b"fooba".as_slice(), "mzxw6ytb"),
            (b"foobar".as_slice(), "mzxw6ytboi"),
        ] {
            assert_eq!(base32_digest(bytes), expected);
        }
        assert_eq!(
            base32_digest(&[0; 32]),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(
            base32_digest(&[255; 32]),
            "777777777777777777777777777777777777777777777777777q"
        );
        let ascending: Vec<u8> = (0..32).collect();
        assert_eq!(
            base32_digest(&ascending),
            "aaaqeayeaudaocajbifqydiob4ibceqtcqkrmfyydenbwha5dypq"
        );
    }

    #[test]
    fn browser_persistence_rejects_memory_database_and_missing_profile() {
        assert!(
            BrowserProfileContext::from_database_path(std::path::Path::new(":memory:")).is_err()
        );
        assert!(BrowserProfileContext::from_database_path(&account_root().join("app.db")).is_err());
    }

    #[test]
    fn runtime_browser_profile_is_immutable_after_account_scope_configuration() {
        let root = account_root();
        let other = root.join("other");
        std::fs::create_dir_all(&other).unwrap();
        let runtime = crate::browser::BrowserRuntime::new();
        assert!(runtime.profile_context().is_none());
        runtime
            .configure_profile_context(&root.join("app.db"))
            .unwrap();
        runtime
            .configure_profile_context(&root.join("app.db"))
            .unwrap();
        assert!(runtime
            .configure_profile_context(&other.join("app.db"))
            .is_err());
        assert_eq!(
            runtime.profile_context().unwrap().profile_directory(),
            root.canonicalize().unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
