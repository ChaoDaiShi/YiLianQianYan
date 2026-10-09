//! Install, list, roll back and deactivate managed skill versions.
//!
//! Installing writes a file *and* commits a database row, so every path here
//! keeps an undo: if the commit fails the previous file content is restored,
//! and a file that changed underneath us is never silently overwritten.
//! `checked_current` is the guard that makes the second guarantee real — it
//! refuses to proceed when the on-disk rule no longer matches the recorded
//! hash, rather than treating the database as the truth.

use crate::db::ManagedSkillVersion;

use super::{
    evidence::screen_evidence,
    repository::{hash, load_candidate, save_revision, versions_on},
    sensitivity::screen_lesson,
    service::MemorySkillService,
    validator::{check_name, check_revision},
};

impl MemorySkillService {
    pub fn confirm(
        &self,
        id: &str,
        revision: u64,
        name: &str,
        confirmed: bool,
        now: i64,
    ) -> Result<ManagedSkillVersion, String> {
        if !confirmed {
            return Err("确认安装 Skill 需要明确的用户操作".into());
        }
        check_name(name)?;
        self.db.initialize_skill_candidates()?;
        let mut conn = self.db.conn();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| "Skill 事务不可用")?;
        let mut candidate = load_candidate(&tx, id)?;
        check_revision(&candidate, revision)?;
        if candidate.status != "validated" {
            return Err("候选须先通过验证".into());
        }
        screen_evidence(&tx, &candidate.sources, &candidate.lesson)?;
        let versions = versions_on(&tx, name)?;
        let before = self.checked_current(name, &versions)?;
        let version = versions
            .iter()
            .map(|v| v.version)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("Skill 版本达到上限")?;
        let content=format!("# {name}\n\n{}\n\n## 用户确认的来源\n\n候选 {}，修订 {}。\n{}\n\n执行仍受现有权限与审批约束。\n",candidate.lesson,candidate.id,candidate.revision,serde_json::to_string(&candidate.sources).map_err(|_|"来源无效")?);
        screen_lesson(&candidate.lesson)?;
        if crate::safety::contains_sensitive_content(&content) {
            return Err("生成的 Skill 内容触发敏感信息检查".into());
        }
        self.install(name, &content)?;
        let installed_hash = hash(&content);
        let committed = (|| -> Result<(), String> {
            tx.execute(
                "UPDATE managed_skill_versions SET active=0 WHERE skill_name=?1",
                [name],
            )
            .map_err(|_| "Skill 版本切换失败")?;
            tx.execute("INSERT INTO managed_skill_versions(skill_name,version,candidate_id,content,content_hash,active,created_at) VALUES (?1,?2,?3,?4,?5,1,?6)",rusqlite::params![name,version,id,content,installed_hash,now]).map_err(|_|"Skill 版本保存失败")?;
            candidate.revision += 1;
            candidate.status = "confirmed".into();
            candidate.skill_name = Some(name.into());
            candidate.skill_version = Some(version);
            candidate.updated_at = now;
            tx.execute("UPDATE skill_candidates SET revision=?2,status='confirmed',skill_name=?3,skill_version=?4,updated_at=?5 WHERE id=?1",rusqlite::params![id,candidate.revision,name,version,now]).map_err(|_|"候选确认失败")?;
            save_revision(&tx, &candidate)?;
            tx.commit().map_err(|_| "Skill 提交失败")?;
            Ok(())
        })();
        if let Err(error) = committed {
            self.restore_file(name, before.as_deref(), &installed_hash)?;
            return Err(error);
        }
        Ok(ManagedSkillVersion {
            skill_name: name.into(),
            version,
            candidate_id: id.into(),
            content,
            content_hash: installed_hash,
            active: true,
            created_at: now,
        })
    }

    pub fn versions(&self, name: &str) -> Result<Vec<ManagedSkillVersion>, String> {
        check_name(name)?;
        self.db.initialize_skill_candidates()?;
        versions_on(&self.db.conn(), name)
    }

    pub fn rollback(
        &self,
        name: &str,
        version: u32,
        _now: i64,
    ) -> Result<ManagedSkillVersion, String> {
        check_name(name)?;
        self.db.initialize_skill_candidates()?;
        let mut conn = self.db.conn();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| "Skill 事务不可用")?;
        let versions = versions_on(&tx, name)?;
        let before = self.checked_current(name, &versions)?;
        let mut selected = versions
            .into_iter()
            .find(|item| item.version == version)
            .ok_or("历史版本不存在")?;
        if crate::safety::contains_sensitive_content(&selected.content)
            || hash(&selected.content) != selected.content_hash
        {
            return Err("历史版本未通过敏感或完整性检查".into());
        }
        self.install(name, &selected.content)?;
        let result = (|| -> Result<(), String> {
            tx.execute(
                "UPDATE managed_skill_versions SET active=0 WHERE skill_name=?1",
                [name],
            )
            .map_err(|_| "Skill 回滚失败")?;
            tx.execute(
                "UPDATE managed_skill_versions SET active=1 WHERE skill_name=?1 AND version=?2",
                rusqlite::params![name, version],
            )
            .map_err(|_| "Skill 回滚失败")?;
            tx.commit().map_err(|_| "Skill 回滚提交失败")?;
            Ok(())
        })();
        if let Err(error) = result {
            self.restore_file(name, before.as_deref(), &selected.content_hash)?;
            return Err(error);
        }
        selected.active = true;
        Ok(selected)
    }

    pub fn deactivate(&self, name: &str) -> Result<(), String> {
        check_name(name)?;
        self.db.initialize_skill_candidates()?;
        let mut conn = self.db.conn();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| "Skill 事务不可用")?;
        let versions = versions_on(&tx, name)?;
        let before = self.checked_current(name, &versions)?;
        if before.is_none() {
            return Ok(());
        }
        let path = self.store.root().join(name).join("SKILL.md");
        let archived = path.with_file_name(format!("SKILL.disabled-{}.md", uuid::Uuid::new_v4()));
        std::fs::rename(&path, &archived).map_err(|_| "Skill 停用失败")?;
        let result = (|| -> Result<(), String> {
            tx.execute(
                "UPDATE managed_skill_versions SET active=0 WHERE skill_name=?1",
                [name],
            )
            .map_err(|_| "Skill 停用记录失败")?;
            tx.commit().map_err(|_| "Skill 停用提交失败")?;
            Ok(())
        })();
        if result.is_err() && !path.exists() {
            std::fs::rename(&archived, &path).map_err(|_| "Skill 停用未完成，需恢复托管文件")?;
        }
        result
    }

    /// Reads the managed rule currently on disk and proves it still matches the
    /// recorded active version. Returns `None` when there is genuinely nothing
    /// installed, and refuses when the directory holds something this service
    /// does not own.
    fn checked_current(
        &self,
        name: &str,
        versions: &[ManagedSkillVersion],
    ) -> Result<Option<String>, String> {
        let path = self.store.root().join(name).join("SKILL.md");
        let active = versions.iter().find(|v| v.active);
        if !path.exists() {
            if active.is_some() {
                return Err("当前 Skill 文件缺失，未覆盖历史状态".into());
            }
            if versions.is_empty() && path.parent().is_some_and(|p| p.exists()) {
                return Err("技能目录已存在且不属于此版本记录".into());
            }
            return Ok(None);
        }
        if !self.store.is_editable(&path)
            || std::fs::symlink_metadata(&path)
                .map_err(|_| "Skill 不可读")?
                .file_type()
                .is_symlink()
        {
            return Err("Skill 不在受控托管目录内".into());
        }
        let content = std::fs::read_to_string(&path).map_err(|_| "Skill 不可读")?;
        if content.len() > 64 * 1024 || active.is_none_or(|v| v.content_hash != hash(&content)) {
            return Err("Skill 已被外部修改，请先审核差异".into());
        }
        Ok(Some(content))
    }
}
