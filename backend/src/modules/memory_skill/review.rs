//! The review state machine: `draft` -> `validated` / `rejected`.
//!
//! Every transition is revision-checked, so two reviewers cannot both act on
//! the same revision. Evidence is re-screened on each edit rather than only at
//! creation, so a lesson cannot be rewritten into something that would have
//! been refused on the way in.

use crate::db::SkillCandidate;

use super::{
    evidence::screen_evidence,
    repository::{load_candidate, save_revision},
    sensitivity::screen_lesson,
    service::MemorySkillService,
    validator::check_revision,
};

impl MemorySkillService {
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
}
