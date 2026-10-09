//! Candidate creation and listing.
//!
//! A candidate starts as `draft` and is never installed from here. Creation
//! requires an explicit authorization flag, so a candidate cannot be created
//! as a side effect of a task completing.

use crate::{db::SkillCandidate, modules::task::artifact::ArtifactSource};

use super::{
    evidence::screen_evidence,
    repository::{load_candidate, save_revision},
    service::MemorySkillService,
};

impl MemorySkillService {
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
}
