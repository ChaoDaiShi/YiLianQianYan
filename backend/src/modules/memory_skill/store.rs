use std::path::{Path, PathBuf};

const MAX_SKILL_NAME_CHARS: usize = 80;
const MAX_SKILL_CONTENT_BYTES: usize = 512 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum SkillManagementError {
    #[error("技能名称不能为空")]
    EmptyName,
    #[error("技能名称不安全")]
    UnsafeName,
    #[error("技能名称过长")]
    NameTooLong,
    #[error("技能内容不能为空")]
    EmptyContent,
    #[error("技能内容超过 512 KB")]
    ContentTooLarge,
    #[error("技能已存在")]
    AlreadyExists,
    #[error("技能不存在")]
    NotFound,
    #[error("技能不在工作区托管目录内")]
    OutsideManagedRoot,
    #[error("技能文件操作失败：{0}")]
    Io(#[from] std::io::Error),
}

pub struct ManagedSkillStore {
    root: PathBuf,
}

impl ManagedSkillStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create(&self, name: &str, content: &str) -> Result<PathBuf, SkillManagementError> {
        validate_skill_name(name)?;
        validate_skill_content(content)?;
        std::fs::create_dir_all(&self.root)?;

        let target = self.root.join(name);
        if target.exists() {
            return Err(SkillManagementError::AlreadyExists);
        }
        std::fs::create_dir(&target)?;
        let skill_file = target.join("SKILL.md");
        if let Err(error) = std::fs::write(&skill_file, content) {
            let _ = std::fs::remove_dir_all(&target);
            return Err(SkillManagementError::Io(error));
        }
        self.require_managed_directory(&target)?;
        Ok(skill_file)
    }

    pub fn update(
        &self,
        current_name: &str,
        new_name: &str,
        content: &str,
    ) -> Result<PathBuf, SkillManagementError> {
        validate_skill_name(current_name)?;
        validate_skill_name(new_name)?;
        validate_skill_content(content)?;

        let source = self.root.join(current_name);
        if !source.exists() {
            return Err(SkillManagementError::NotFound);
        }
        self.require_managed_directory(&source)?;

        let target = self.root.join(new_name);
        let renamed = current_name != new_name;
        if renamed {
            if target.exists() {
                return Err(SkillManagementError::AlreadyExists);
            }
            std::fs::rename(&source, &target)?;
        }

        let skill_file = target.join("SKILL.md");
        if let Err(error) = std::fs::write(&skill_file, content) {
            if renamed {
                let _ = std::fs::rename(&target, &source);
            }
            return Err(SkillManagementError::Io(error));
        }
        self.require_managed_directory(&target)?;
        Ok(skill_file)
    }

    pub fn delete(&self, name: &str) -> Result<(), SkillManagementError> {
        validate_skill_name(name)?;
        let target = self.root.join(name);
        if !target.exists() {
            return Err(SkillManagementError::NotFound);
        }
        self.require_managed_directory(&target)?;
        std::fs::remove_dir_all(target)?;
        Ok(())
    }

    pub fn is_editable(&self, skill_file: &Path) -> bool {
        let Ok(root) = self.root.canonicalize() else {
            return false;
        };
        let Ok(skill_file) = skill_file.canonicalize() else {
            return false;
        };
        skill_file.starts_with(root)
            && skill_file
                .file_name()
                .is_some_and(|name| name == "SKILL.md")
    }

    fn require_managed_directory(&self, directory: &Path) -> Result<(), SkillManagementError> {
        let root = self.root.canonicalize()?;
        let directory = directory.canonicalize()?;
        if directory == root || !directory.starts_with(&root) {
            return Err(SkillManagementError::OutsideManagedRoot);
        }
        Ok(())
    }
}

fn validate_skill_name(name: &str) -> Result<(), SkillManagementError> {
    if name.is_empty() {
        return Err(SkillManagementError::EmptyName);
    }
    if name.trim() != name || name == "." || name == ".." {
        return Err(SkillManagementError::UnsafeName);
    }
    if name.chars().count() > MAX_SKILL_NAME_CHARS {
        return Err(SkillManagementError::NameTooLong);
    }
    if name
        .chars()
        .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':'))
    {
        return Err(SkillManagementError::UnsafeName);
    }
    let upper = name.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit());
    if reserved {
        return Err(SkillManagementError::UnsafeName);
    }
    Ok(())
}

fn validate_skill_content(content: &str) -> Result<(), SkillManagementError> {
    if content.trim().is_empty() {
        return Err(SkillManagementError::EmptyContent);
    }
    if content.len() > MAX_SKILL_CONTENT_BYTES {
        return Err(SkillManagementError::ContentTooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ManagedSkillStore;

    struct TempSkillRoot {
        path: std::path::PathBuf,
    }

    impl TempSkillRoot {
        fn new(label: &str) -> Self {
            Self {
                path: std::env::temp_dir()
                    .join(format!("yilian-skills-{label}-{}", uuid::Uuid::new_v4())),
            }
        }
    }

    impl Drop for TempSkillRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn managed_skill_store_rejects_path_traversal() {
        let temp = TempSkillRoot::new("path-safety");
        let store = ManagedSkillStore::new(temp.path.join("skills"));

        assert!(store.create("../escape", "# Escape").is_err());
        assert!(store.create("nested/escape", "# Escape").is_err());
        assert!(store.create("..", "# Escape").is_err());
    }

    #[test]
    fn managed_skill_store_creates_updates_and_deletes_skill_md() {
        let temp = TempSkillRoot::new("crud");
        let store = ManagedSkillStore::new(temp.path.join("skills"));

        store.create("demo", "# Demo\n\nFirst").unwrap();
        store
            .update("demo", "demo-renamed", "# Demo\n\nSecond")
            .unwrap();

        let renamed = store.root().join("demo-renamed").join("SKILL.md");
        assert_eq!(
            std::fs::read_to_string(&renamed).unwrap(),
            "# Demo\n\nSecond"
        );
        assert!(!store.root().join("demo").exists());

        store.delete("demo-renamed").unwrap();
        assert!(!store.root().join("demo-renamed").exists());
    }
}
