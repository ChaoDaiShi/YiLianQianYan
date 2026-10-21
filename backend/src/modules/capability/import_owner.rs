//! Runtime owner for enabled managed Skill imports.
//!
//! Imported content stays inert until the user explicitly enables it. Validated
//! Skill instructions and bundled resources are materialized byte-for-byte; declarative plugins are never loaded
//! as executable code here.

use super::import_store::Package;
use crate::modules::memory_skill::store::ManagedSkillStore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const OWNER_FILE: &str = ".yilian-import-owner.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Ownership {
    schema_version: u32,
    package_id: String,
    content_hash: String,
}

#[derive(Debug)]
pub struct ActivationSnapshot {
    target: PathBuf,
    previous_owner: Option<Vec<u8>>,
    previous_skill: Option<Vec<u8>>,
    previous_resources: std::collections::BTreeMap<String, Vec<u8>>,
    existed: bool,
}

fn runtime_name(id: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(id.as_bytes()));
    format!("managed-import-{}", &digest[..20])
}

fn target(store: &ManagedSkillStore, id: &str) -> PathBuf {
    store.root().join(runtime_name(id))
}

fn require_owned_target(path: &Path, id: &str) -> Result<Ownership, String> {
    if std::fs::symlink_metadata(path)
        .map_err(|_| "managed_import_runtime_unavailable")?
        .file_type()
        .is_symlink()
    {
        return Err("managed_import_runtime_path_unsafe".into());
    }
    let raw =
        std::fs::read(path.join(OWNER_FILE)).map_err(|_| "managed_import_runtime_owner_missing")?;
    let owner: Ownership =
        serde_json::from_slice(&raw).map_err(|_| "managed_import_runtime_owner_invalid")?;
    if owner.schema_version != 1 || owner.package_id != id {
        return Err("managed_import_runtime_owner_mismatch".into());
    }
    Ok(owner)
}

fn snapshot(store: &ManagedSkillStore, id: &str) -> Result<ActivationSnapshot, String> {
    let target = target(store, id);
    if !target.exists() {
        return Ok(ActivationSnapshot {
            target,
            previous_owner: None,
            previous_skill: None,
            previous_resources: Default::default(),
            existed: false,
        });
    }
    require_owned_target(&target, id)?;
    fn resources(
        root: &Path,
        current: &Path,
        result: &mut std::collections::BTreeMap<String, Vec<u8>>,
    ) -> Result<(), String> {
        for entry in std::fs::read_dir(current).map_err(|_| "managed_import_runtime_unavailable")? {
            let entry = entry.map_err(|_| "managed_import_runtime_unavailable")?;
            let meta = std::fs::symlink_metadata(entry.path())
                .map_err(|_| "managed_import_runtime_unavailable")?;
            if meta.file_type().is_symlink() {
                return Err("managed_import_runtime_path_unsafe".into());
            }
            if meta.is_dir() {
                resources(root, &entry.path(), result)?;
            } else {
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| "managed_import_runtime_path_unsafe")?
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative != "SKILL.md" && relative != OWNER_FILE {
                    result.insert(
                        relative,
                        std::fs::read(entry.path())
                            .map_err(|_| "managed_import_runtime_unavailable")?,
                    );
                }
            }
        }
        Ok(())
    }
    let mut previous_resources = Default::default();
    resources(&target, &target, &mut previous_resources)?;
    Ok(ActivationSnapshot {
        previous_resources,
        previous_owner: Some(
            std::fs::read(target.join(OWNER_FILE))
                .map_err(|_| "managed_import_runtime_unavailable")?,
        ),
        previous_skill: Some(
            std::fs::read(target.join("SKILL.md"))
                .map_err(|_| "managed_import_runtime_unavailable")?,
        ),
        target,
        existed: true,
    })
}

pub fn activate(
    store: &ManagedSkillStore,
    package: &Package,
) -> Result<ActivationSnapshot, String> {
    if package.kind != "skill" {
        return Err("declarative_plugin_runtime_not_supported".into());
    }
    let content = package
        .files
        .get("SKILL.md")
        .ok_or("missing_skill_markdown")?;
    let saved = snapshot(store, &package.id)?;
    if !saved.existed {
        std::fs::create_dir_all(store.root()).map_err(|_| "managed_import_runtime_unavailable")?;
        std::fs::create_dir(&saved.target).map_err(|_| "managed_import_runtime_unavailable")?;
    }
    let owner = Ownership {
        schema_version: 1,
        package_id: package.id.clone(),
        content_hash: package.content_hash.clone(),
    };
    let result = serde_json::to_vec(&owner)
        .map_err(|_| "managed_import_runtime_owner_invalid".to_string())
        .and_then(|raw| {
            std::fs::write(saved.target.join(OWNER_FILE), raw)
                .map_err(|_| "managed_import_runtime_unavailable")?;
            std::fs::write(saved.target.join("SKILL.md"), content)
                .map_err(|_| "managed_import_runtime_unavailable".to_string())?;
            for relative in package.files.keys() {
                let path = saved.target.join(relative);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|_| "managed_import_runtime_unavailable")?;
                }
                std::fs::write(
                    path,
                    super::import_store::resource_bytes(package, relative)?,
                )
                .map_err(|_| "managed_import_runtime_unavailable")?;
            }
            Ok(())
        });
    if let Err(error) = result {
        let _ = restore(saved);
        return Err(error);
    }
    Ok(saved)
}

pub fn deactivate(
    store: &ManagedSkillStore,
    package_id: &str,
) -> Result<ActivationSnapshot, String> {
    let saved = snapshot(store, package_id)?;
    if saved.existed {
        std::fs::remove_dir_all(&saved.target).map_err(|_| "managed_import_runtime_unavailable")?;
    }
    Ok(saved)
}

pub fn restore(saved: ActivationSnapshot) -> Result<(), String> {
    if !saved.existed {
        if saved.target.exists() {
            std::fs::remove_dir_all(saved.target)
                .map_err(|_| "managed_import_runtime_rollback_failed")?;
        }
        return Ok(());
    }
    if saved.target.exists() {
        std::fs::remove_dir_all(&saved.target)
            .map_err(|_| "managed_import_runtime_rollback_failed")?;
    }
    std::fs::create_dir_all(&saved.target).map_err(|_| "managed_import_runtime_rollback_failed")?;
    std::fs::write(
        saved.target.join(OWNER_FILE),
        saved.previous_owner.unwrap_or_default(),
    )
    .map_err(|_| "managed_import_runtime_rollback_failed")?;
    std::fs::write(
        saved.target.join("SKILL.md"),
        saved.previous_skill.unwrap_or_default(),
    )
    .map_err(|_| "managed_import_runtime_rollback_failed")?;
    for (relative, bytes) in saved.previous_resources {
        let path = saved.target.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| "managed_import_runtime_rollback_failed")?;
        }
        std::fs::write(path, bytes).map_err(|_| "managed_import_runtime_rollback_failed")?;
    }
    Ok(())
}

pub fn is_active(store: &ManagedSkillStore, package: &Package) -> bool {
    if package.kind != "skill" {
        return false;
    }
    let target = target(store, &package.id);
    let Ok(owner) = require_owned_target(&target, &package.id) else {
        return false;
    };
    owner.content_hash == package.content_hash
        && package.files.iter().all(|(relative, _)| {
            super::import_store::resource_bytes(package, relative).is_ok_and(|expected| {
                std::fs::read(target.join(relative)).is_ok_and(|actual| actual == expected)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::capability::import_store::markdown_package;

    #[test]
    fn activation_is_owned_visible_and_reversible() {
        let root =
            std::env::temp_dir().join(format!("yilian-import-owner-{}", uuid::Uuid::new_v4()));
        let store = ManagedSkillStore::new(root.clone());
        let package = markdown_package("demo.skill", "Demo", "1.0.0", "# Demo", "test").unwrap();

        let empty = activate(&store, &package).unwrap();
        assert!(is_active(&store, &package));
        restore(empty).unwrap();
        assert!(!is_active(&store, &package));

        let _ = std::fs::remove_dir_all(root);
    }
}
