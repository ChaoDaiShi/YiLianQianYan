//! The Memory-to-Skill service handle and its managed-file side effects.
//!
//! `install` and `restore_file` are the only places this module writes a
//! `SKILL.md`. Both are written to a temporary file and renamed into place so
//! a reader never observes a half-written rule, and `restore_file` refuses to
//! undo a change it cannot prove it made.

use crate::{db::Database, modules::memory_skill::store::ManagedSkillStore};

pub struct MemorySkillService {
    pub(crate) db: Database,
    pub(crate) store: ManagedSkillStore,
}

impl MemorySkillService {
    pub fn new(db: Database, store: ManagedSkillStore) -> Self {
        Self { db, store }
    }

    pub(crate) fn install(&self, name: &str, content: &str) -> Result<(), String> {
        let directory = self.store.root().join(name);
        if !directory.exists() {
            self.store
                .create(name, content)
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        let root = self
            .store
            .root()
            .canonicalize()
            .map_err(|_| "托管技能目录不可用")?;
        let directory = directory.canonicalize().map_err(|_| "技能目录不可用")?;
        if directory == root || !directory.starts_with(&root) {
            return Err("技能目录超出工作区".into());
        }
        let temporary = directory.join(format!(".candidate-{}.tmp", uuid::Uuid::new_v4()));
        let path = directory.join("SKILL.md");
        let result = (|| -> std::io::Result<()> {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temporary, &path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result.map_err(|_| "Skill 原子写入失败".into())
    }

    pub(crate) fn restore_file(
        &self,
        name: &str,
        before: Option<&str>,
        installed_hash: &str,
    ) -> Result<(), String> {
        let path = self.store.root().join(name).join("SKILL.md");
        let current = std::fs::read_to_string(&path).map_err(|_| "需恢复 Skill 文件")?;
        if super::repository::hash(&current) != installed_hash {
            return Err("Skill 在失败回滚期间被修改，未覆盖该内容".into());
        }
        if let Some(before) = before {
            self.install(name, before)
        } else {
            std::fs::remove_file(&path).map_err(|_| "需清理未提交 Skill 文件")?;
            if let Some(parent) = path.parent() {
                let _ = std::fs::remove_dir(parent);
            }
            Ok(())
        }
    }
}
