//! User-reviewed evidence candidates; never an automatic long-term Memory write.
use crate::{
    db::{Database, ManagedSkillVersion, SkillCandidate},
    skill_management::ManagedSkillStore,
    task::artifact::ArtifactSource,
};

pub struct MemorySkillService {
    db: Database,
    store: ManagedSkillStore,
}
impl MemorySkillService {
    pub fn new(db: Database, store: ManagedSkillStore) -> Self {
        Self { db, store }
    }
    pub fn create(
        &self,
        sources: Vec<ArtifactSource>,
        lesson: &str,
        authorized: bool,
        now: i64,
    ) -> Result<SkillCandidate, String> {
        if !authorized {
            return Err("必须明确授权使用所选完成任务的证据".into());
        }
        self.db.initialize_skill_candidates()?;
        let mut conn = self.db.conn();
        let transaction = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| "候选事务不可用")?;
        screen_evidence(&transaction, &sources, lesson)?;
        let candidate = SkillCandidate {
            id: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            status: "draft".into(),
            sources,
            lesson: lesson.trim().into(),
            sensitivity: "screened".into(),
            skill_name: None,
            skill_version: None,
            created_at: now,
            updated_at: now,
        };
        transaction.execute("INSERT INTO skill_candidates(id,revision,status,source_json,lesson,sensitivity,created_at,updated_at) VALUES (?1,1,'draft',?2,?3,'screened',?4,?4)", rusqlite::params![candidate.id,serde_json::to_string(&candidate.sources).map_err(|_|"来源无效")?,candidate.lesson,now]).map_err(|_|"候选保存失败")?;
        save_revision(&transaction, &candidate)?;
        transaction.commit().map_err(|_| "候选提交失败")?;
        Ok(candidate)
    }
    pub fn edit(
        &self,
        id: &str,
        revision: u64,
        lesson: &str,
        now: i64,
    ) -> Result<SkillCandidate, String> {
        self.change(id, revision, Some(lesson), "draft", now)
    }
    pub fn validate(&self, id: &str, revision: u64, now: i64) -> Result<SkillCandidate, String> {
        self.change(id, revision, None, "validated", now)
    }
    pub fn reject(&self, id: &str, revision: u64, now: i64) -> Result<SkillCandidate, String> {
        self.change(id, revision, None, "rejected", now)
    }
    fn change(
        &self,
        id: &str,
        revision: u64,
        lesson: Option<&str>,
        status: &str,
        now: i64,
    ) -> Result<SkillCandidate, String> {
        self.db.initialize_skill_candidates()?;
        let mut conn = self.db.conn();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| "候选事务不可用")?;
        let mut candidate = load_candidate(&tx, id)?;
        check_revision(&candidate, revision)?;
        if let Some(lesson) = lesson {
            candidate.lesson = lesson.trim().into();
        }
        if status != "rejected" {
            screen_evidence(&tx, &candidate.sources, &candidate.lesson)?;
        } else {
            screen_lesson(&candidate.lesson)?;
        }
        candidate.revision = candidate
            .revision
            .checked_add(1)
            .ok_or("候选版本达到上限")?;
        candidate.status = status.into();
        candidate.updated_at = now;
        tx.execute(
            "UPDATE skill_candidates SET revision=?2,status=?3,lesson=?4,updated_at=?5 WHERE id=?1",
            rusqlite::params![id, candidate.revision, status, candidate.lesson, now],
        )
        .map_err(|_| "候选更新失败")?;
        save_revision(&tx, &candidate)?;
        tx.commit().map_err(|_| "候选提交失败")?;
        Ok(candidate)
    }
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
    pub fn list(&self) -> Result<Vec<SkillCandidate>, String> {
        self.db.initialize_skill_candidates()?;
        let conn = self.db.conn();
        let mut query = conn
            .prepare("SELECT id FROM skill_candidates ORDER BY updated_at DESC LIMIT 100")
            .map_err(|_| "候选列表不可用")?;
        let ids = query
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| "候选列表不可用")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "候选记录无效")?;
        ids.iter().map(|id| load_candidate(&conn, id)).collect()
    }
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
    fn install(&self, name: &str, content: &str) -> Result<(), String> {
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
    fn restore_file(
        &self,
        name: &str,
        before: Option<&str>,
        installed_hash: &str,
    ) -> Result<(), String> {
        let path = self.store.root().join(name).join("SKILL.md");
        let current = std::fs::read_to_string(&path).map_err(|_| "需恢复 Skill 文件")?;
        if hash(&current) != installed_hash {
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
fn hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 80
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'))
    {
        Err("Skill 名称须为 1-80 个小写字母、数字、横线或下划线".into())
    } else {
        Ok(())
    }
}
fn screen_lesson(lesson: &str) -> Result<(), String> {
    use crate::agent::memory::{
        validate_candidate, MemoryCandidate, MemoryCategory, MemoryWritePolicy,
    };
    let candidate = MemoryCandidate::new(
        MemoryCategory::Preference,
        lesson,
        None,
        "user-reviewed-candidate",
        1.0,
    );
    validate_candidate(&candidate, &MemoryWritePolicy::default(), &[])
        .map_err(|_| "经验内容为空、过长或包含敏感信息".into())
}
fn screen_evidence(
    conn: &rusqlite::Connection,
    sources: &[ArtifactSource],
    lesson: &str,
) -> Result<(), String> {
    screen_lesson(lesson)?;
    if sources.is_empty()
        || sources.len() > 8
        || serde_json::to_string(sources)
            .map_err(|_| "来源无效")?
            .len()
            > 4096
    {
        return Err("须明确选择 1-8 个有界完成任务来源".into());
    }
    for source in sources {
        crate::db::validate_completed_source(conn, source)?;
        let text=match source{
            ArtifactSource::Task{task_id,execution_id}=>{
                let mut value:String=conn.query_row("SELECT title || char(10) || description FROM tasks WHERE id=?1",[task_id],|r|r.get(0)).map_err(|_|"来源任务不可用")?;
                let mut q=conn.prepare("SELECT summary FROM artifacts WHERE task_id=?1 AND task_execution_id=?2 LIMIT 8").map_err(|_|"来源结果不可用")?;
                for summary in q.query_map(rusqlite::params![task_id,execution_id],|r|r.get::<_,String>(0)).map_err(|_|"来源结果不可用")?{value.push_str(&summary.map_err(|_|"来源结果无效")?);}value
            }
            ArtifactSource::Node{execution_id,..}=>conn.query_row("SELECT context_json || char(10) || COALESCE(output_json,'') FROM task_node_executions WHERE execution_id=?1",[execution_id],|r|r.get::<_,String>(0)).map_err(|_|"来源执行不可用")?,
        };
        if crate::safety::contains_sensitive_content(&text) {
            return Err("所选任务证据含敏感信息，未保存候选".into());
        }
    }
    Ok(())
}
fn check_revision(candidate: &SkillCandidate, revision: u64) -> Result<(), String> {
    if candidate.revision != revision {
        return Err("候选已更新，请刷新后重试".into());
    }
    if candidate.status == "confirmed" || candidate.status == "rejected" {
        return Err("此候选已结束审核，请创建新候选".into());
    }
    Ok(())
}
fn load_candidate(conn: &rusqlite::Connection, id: &str) -> Result<SkillCandidate, String> {
    conn.query_row("SELECT id,revision,status,source_json,lesson,sensitivity,skill_name,skill_version,created_at,updated_at FROM skill_candidates WHERE id=?1",[id],|row|{
        let sources:String=row.get(3)?;let sources=serde_json::from_str(&sources).map_err(|error|rusqlite::Error::FromSqlConversionFailure(3,rusqlite::types::Type::Text,Box::new(error)))?;
        Ok(SkillCandidate{id:row.get(0)?,revision:row.get(1)?,status:row.get(2)?,sources,lesson:row.get(4)?,sensitivity:row.get(5)?,skill_name:row.get(6)?,skill_version:row.get(7)?,created_at:row.get(8)?,updated_at:row.get(9)?})
    }).map_err(|_|"候选不存在或记录无效".into())
}
fn save_revision(conn: &rusqlite::Connection, candidate: &SkillCandidate) -> Result<(), String> {
    conn.execute("INSERT INTO skill_candidate_revisions(candidate_id,revision,lesson,status,created_at) VALUES (?1,?2,?3,?4,?5)",rusqlite::params![candidate.id,candidate.revision,candidate.lesson,candidate.status,candidate.updated_at]).map_err(|_|"候选修订保存失败")?;
    Ok(())
}
fn versions_on(
    conn: &rusqlite::Connection,
    name: &str,
) -> Result<Vec<ManagedSkillVersion>, String> {
    let mut query=conn.prepare("SELECT skill_name,version,candidate_id,content,content_hash,active,created_at FROM managed_skill_versions WHERE skill_name=?1 ORDER BY version DESC LIMIT 100").map_err(|_|"版本列表不可用")?;
    let versions = query
        .query_map([name], |r| {
            Ok(ManagedSkillVersion {
                skill_name: r.get(0)?,
                version: r.get(1)?,
                candidate_id: r.get(2)?,
                content: r.get(3)?,
                content_hash: r.get(4)?,
                active: r.get(5)?,
                created_at: r.get(6)?,
            })
        })
        .map_err(|_| "版本列表不可用")?;
    versions
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "版本记录无效".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        shared::event::EventHub,
        task::{TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind, TaskWorldRuntime},
    };
    fn setup() -> (
        std::path::PathBuf,
        Database,
        MemorySkillService,
        ArtifactSource,
    ) {
        let root = std::env::temp_dir().join(format!("skill-evidence-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let db = Database::new(&root.join("db.sqlite")).unwrap();
        let runtime = TaskWorldRuntime::new(&db, EventHub::new(4)).unwrap();
        let graph = TaskGraphId::new("lesson-task").unwrap();
        let node = TaskNodeId::new("node").unwrap();
        runtime
            .create_graph(
                graph.clone(),
                vec![TaskNode::new(
                    node.clone(),
                    TaskNodeKind::Work,
                    "Completed evidence",
                    serde_json::json!({}),
                )
                .unwrap()],
                vec![],
                1,
            )
            .unwrap();
        let execution = runtime.start_execution(&graph, &node, 1, 2).unwrap();
        runtime
            .complete_execution(
                &graph,
                &execution.id,
                serde_json::json!({"ok":true,"result":"Verified evidence"}),
                crate::task::validation::ValidationPolicy::StructuredResult,
                3,
            )
            .unwrap();
        let source = ArtifactSource::Node {
            graph_id: graph.to_string(),
            node_id: node.to_string(),
            execution_id: execution.id.to_string(),
            workspace_id: String::new(),
        };
        let service =
            MemorySkillService::new(db.clone(), ManagedSkillStore::new(root.join("skills")));
        (root, db, service, source)
    }
    #[test]
    fn authorized_candidate_is_reviewed_versioned_discoverable_and_reversible() {
        let (root, db, service, source) = setup();
        assert!(service
            .create(vec![source.clone()], "完成后记录验证步骤", false, 4)
            .is_err());
        let draft = service
            .create(vec![source.clone()], "完成后记录验证步骤", true, 4)
            .unwrap();
        assert_eq!(draft.status, "draft");
        assert!(!root.join("skills/reviewed/SKILL.md").exists());
        assert!(service
            .confirm(&draft.id, draft.revision, "reviewed", true, 5)
            .is_err());
        let edited = service
            .edit(
                &draft.id,
                draft.revision,
                "完成后先记录验证步骤，再整理结果",
                5,
            )
            .unwrap();
        assert!(service.validate(&draft.id, draft.revision, 6).is_err());
        let valid = service.validate(&edited.id, edited.revision, 6).unwrap();
        assert!(service
            .confirm(&valid.id, valid.revision, "reviewed", false, 7)
            .is_err());
        let first = service
            .confirm(&valid.id, valid.revision, "reviewed", true, 7)
            .unwrap();
        assert_eq!(first.version, 1);
        let discovery = crate::tools::skill::SkillDiscovery::discover(
            &[root.join("skills").to_string_lossy().into()],
            root.to_str().unwrap(),
        );
        assert!(discovery.all().iter().any(|skill| skill.name == "reviewed"));
        let second = service
            .create(vec![source], "完成后先整理结果，再记录后续问题", true, 8)
            .unwrap();
        let second = service.validate(&second.id, second.revision, 9).unwrap();
        assert_eq!(
            service
                .confirm(&second.id, second.revision, "reviewed", true, 10)
                .unwrap()
                .version,
            2
        );
        service.deactivate("reviewed").unwrap();
        assert!(!root.join("skills/reviewed/SKILL.md").exists());
        assert_eq!(service.rollback("reviewed", 1, 11).unwrap().version, 1);
        assert!(
            std::fs::read_to_string(root.join("skills/reviewed/SKILL.md"))
                .unwrap()
                .contains("先记录验证步骤")
        );
        assert_eq!(service.versions("reviewed").unwrap().len(), 2);
        let memories: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
            .unwrap();
        assert_eq!(memories, 0);
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn confirmation_database_failure_leaves_no_installed_rule() {
        let (root, db, service, source) = setup();
        let draft = service
            .create(vec![source], "记录任务的验证步骤", true, 4)
            .unwrap();
        let valid = service.validate(&draft.id, draft.revision, 5).unwrap();
        db.conn().execute_batch("CREATE TRIGGER fail_skill_version BEFORE INSERT ON managed_skill_versions BEGIN SELECT RAISE(ABORT,'injected version failure'); END;").unwrap();
        assert!(service
            .confirm(&valid.id, valid.revision, "rollback-safe", true, 6)
            .is_err());
        assert!(!root.join("skills/rollback-safe/SKILL.md").exists());
        assert_eq!(service.list().unwrap()[0].status, "validated");
        assert!(service.versions("rollback-safe").unwrap().is_empty());
        let rejected = service.reject(&valid.id, valid.revision, 7).unwrap();
        assert_eq!(rejected.status, "rejected");
        assert!(service
            .confirm(&valid.id, rejected.revision, "rejected", true, 8)
            .is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn sensitivity_is_checked_before_draft_and_again_before_confirmation() {
        let (root, db, service, source) = setup();
        assert!(service
            .create(vec![source.clone()], "password = PLACEHOLDER", true, 4)
            .is_err());
        assert!(service.list().unwrap().is_empty());
        let draft = service
            .create(vec![source], "记录已验证的步骤和限制", true, 5)
            .unwrap();
        let valid = service.validate(&draft.id, draft.revision, 6).unwrap();
        db.conn()
            .execute(
                "UPDATE skill_candidates SET lesson='password = PLACEHOLDER' WHERE id=?1",
                [&valid.id],
            )
            .unwrap();
        assert!(service
            .confirm(&valid.id, valid.revision, "unsafe", true, 7)
            .is_err());
        assert!(!root.join("skills/unsafe/SKILL.md").exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
