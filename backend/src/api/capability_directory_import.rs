//! Materialize a user-selected Skill directory as an inert import package.

/// Read a user-selected Skill directory without running its contents.
pub(super) fn directory_skill_package(
    id: &str,
    name: &str,
    version: &str,
    directory: &str,
) -> Result<crate::modules::capability::import_store::Package, String> {
    use crate::modules::capability::import_store::{
        markdown_package, validate_package, MAX_ENTRIES, MAX_PACKAGE_BYTES,
    };
    let root = std::path::Path::new(directory);
    if !root.is_absolute() {
        return Err("absolute_skill_directory_required".into());
    }
    let root = root
        .canonicalize()
        .map_err(|_| "skill_directory_unavailable")?;
    fn visit(
        root: &std::path::Path,
        current: &std::path::Path,
        files: &mut std::collections::BTreeMap<String, String>,
        encodings: &mut std::collections::BTreeMap<String, String>,
        total: &mut usize,
    ) -> Result<(), String> {
        for entry in std::fs::read_dir(current).map_err(|_| "skill_directory_unavailable")? {
            let entry = entry.map_err(|_| "skill_directory_unavailable")?;
            let meta =
                std::fs::symlink_metadata(entry.path()).map_err(|_| "skill_file_unavailable")?;
            if meta.file_type().is_symlink() {
                return Err("skill_directory_links_not_supported".into());
            }
            if meta.is_dir() {
                visit(root, &entry.path(), files, encodings, total)?;
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| "invalid_skill_path")?
                .to_string_lossy()
                .replace('\\', "/");
            if meta.len() > 512 * 1024 || files.len() >= MAX_ENTRIES {
                return Err("skill_package_limit".into());
            }
            *total += meta.len() as usize;
            if *total > MAX_PACKAGE_BYTES {
                return Err("skill_package_limit".into());
            }
            let bytes = std::fs::read(entry.path()).map_err(|_| "skill_resource_unavailable")?;
            let (content, encoding) =
                crate::modules::capability::import_store::encode_resource(&bytes);
            if let Some(encoding) = encoding {
                encodings.insert(relative.clone(), encoding);
            }
            files.insert(relative, content);
        }
        Ok(())
    }
    let mut files = std::collections::BTreeMap::new();
    let mut encodings = std::collections::BTreeMap::new();
    visit(&root, &root, &mut files, &mut encodings, &mut 0)?;
    let content = files.get("SKILL.md").ok_or("missing_skill_markdown")?;
    let mut package = markdown_package(
        id,
        name,
        version,
        content,
        &format!("directory:{}", root.display()),
    )?;
    package.files = files;
    package.file_encodings = encodings;
    validate_package(&mut package)?;
    Ok(package)
}

#[cfg(test)]
mod directory_import_tests {
    use super::*;
    #[test]
    fn selected_skill_directory_preserves_script_and_image_bytes_on_activation() {
        use crate::modules::capability::{import_owner, import_store::resource_bytes};
        use base64::Engine;
        let root =
            std::env::temp_dir().join(format!("yilian-complete-skill-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("SKILL.md"),"---\nname: complete-skill\n---\n# Complete Skill\nUse scripts/task.py and assets/pixel.png").unwrap();
        let script = b"raise RuntimeError('scripts must not run while importing')\n";
        let image=base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/lZYAAAAASUVORK5CYII=").unwrap();
        std::fs::write(root.join("scripts/task.py"), script).unwrap();
        std::fs::write(root.join("assets/pixel.png"), &image).unwrap();
        let package = directory_skill_package(
            "complete-skill",
            "Complete Skill",
            "1.0.0",
            &root.to_string_lossy(),
        )
        .unwrap();
        assert_eq!(resource_bytes(&package, "scripts/task.py").unwrap(), script);
        assert_eq!(resource_bytes(&package, "assets/pixel.png").unwrap(), image);
        assert_eq!(package.file_encodings["assets/pixel.png"], "base64");
        let db = crate::db::Database::new(std::path::Path::new(":memory:")).unwrap();
        let preview = crate::modules::capability::import_store::inspect(&db, package, 1).unwrap();
        let installed =
            crate::modules::capability::import_store::confirm(&db, &preview.preview_id, 2).unwrap();
        let store =
            crate::modules::memory_skill::store::ManagedSkillStore::new(root.join("installed"));
        let snapshot = import_owner::activate(&store, &installed.package).unwrap();
        let runtime = std::fs::read_dir(store.root())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            std::fs::read(runtime.join("scripts/task.py")).unwrap(),
            script
        );
        assert_eq!(
            std::fs::read(runtime.join("assets/pixel.png")).unwrap(),
            image
        );
        assert!(import_owner::is_active(&store, &installed.package));
        import_owner::restore(snapshot).unwrap();
        assert!(directory_skill_package("demo", "Demo", "1.0.0", "relative").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
