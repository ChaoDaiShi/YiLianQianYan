use std::collections::BTreeSet;

use crate::modules::{
    memory_skill::{repository::hash, service::MemorySkillService},
    task::proofloop::{
        contracts::{AcceptanceReport, GoalStatus},
        enhancement_contracts::SkillRecommendation,
    },
};

pub fn feedback_outcome(report: &AcceptanceReport) -> &'static str {
    match report.status {
        GoalStatus::Pass => "pass",
        GoalStatus::Fail => "fail",
        GoalStatus::Incomplete => "incomplete",
    }
}

impl MemorySkillService {
    /// Record only recommendations whose context actually entered this draft's
    /// planning prompt. Call this after the draft row has been persisted.
    pub fn record_planning_skill_usage(
        &self,
        goal_id: &str,
        goal_version: u32,
        draft_id: &str,
        used_recommendations: &[SkillRecommendation],
        recorded_at_ms: i64,
    ) -> Result<(), String> {
        if used_recommendations.is_empty() {
            return Ok(());
        }
        if goal_id.trim().is_empty() || draft_id.trim().is_empty() || goal_version == 0 {
            return Err("Skill 使用记录缺少目标版本或草稿 ID".into());
        }
        self.db.initialize_skill_candidates()?;
        self.db.initialize_skill_usage()?;

        let draft_matches = self
            .db
            .conn()
            .query_row(
                "SELECT 1 FROM proofloop_drafts
                 WHERE draft_id=?1 AND goal_id=?2 AND goal_version=?3",
                rusqlite::params![draft_id, goal_id, goal_version],
                |_| Ok(()),
            )
            .optional()
            .map_err(|_| "Skill 使用记录无法核对草稿")?
            .is_some();
        if !draft_matches {
            return Err("Skill 使用记录与已保存的目标草稿不匹配".into());
        }

        let mut unique = BTreeSet::new();
        let conn = self.db.conn();
        for recommendation in used_recommendations {
            if recommendation.context_text.is_empty()
                || !unique.insert((recommendation.skill_name.clone(), recommendation.version))
            {
                continue;
            }
            let stored: Option<(String, String)> = conn
                .query_row(
                    "SELECT v.content,v.content_hash
                     FROM managed_skill_versions v
                     JOIN skill_candidates c ON c.id=v.candidate_id
                     WHERE v.skill_name=?1 AND v.version=?2 AND v.active=1
                       AND c.status='confirmed' AND c.skill_name=v.skill_name
                       AND c.skill_version=v.version",
                    rusqlite::params![recommendation.skill_name, recommendation.version],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|_| "Skill 使用记录无法核对已审核版本")?;
            let Some((content, content_hash)) = stored else {
                return Err("记录时 Skill 已不再是当前激活的已审核版本".into());
            };
            if hash(&content) != content_hash || !content.starts_with(&recommendation.context_text)
            {
                return Err("进入规划的 Skill 文本与已审核版本不匹配".into());
            }
            let path = self
                .store
                .root()
                .join(&recommendation.skill_name)
                .join("SKILL.md");
            if !self.store.is_editable(&path)
                || std::fs::read_to_string(path).map_or(true, |current| {
                    current != content || hash(&current) != content_hash
                })
            {
                return Err("Skill 文件已不匹配已审核版本，未记录为规划使用".into());
            }
        }
        drop(conn);
        let skills = unique.into_iter().collect::<Vec<_>>();
        if skills.is_empty() {
            return Ok(());
        }
        self.db.record_skill_usage_pending_batch(
            goal_id,
            goal_version,
            draft_id,
            &skills,
            recorded_at_ms,
        )
    }

    /// Attach the current persisted report to usage rows for this draft. The
    /// caller supplies the live TaskWorld revision after its usual latest-run
    /// and evidence checks; stale reports cannot affect recommendation scores.
    pub fn record_skill_report_feedback(
        &self,
        draft_id: &str,
        goal_id: &str,
        goal_version: u32,
        current_graph_revision: u64,
        report: &AcceptanceReport,
    ) -> Result<usize, String> {
        if report.report_id.trim().is_empty()
            || report.goal_id != goal_id
            || report.goal_version != goal_version
        {
            return Err("验收报告与 Skill 使用记录的目标版本不匹配".into());
        }
        if report.graph_revision != current_graph_revision {
            return Err("验收报告对应的任务图版本已过期，需重新验收".into());
        }

        let (persisted_report, evidence) = self
            .db
            .get_acceptance_report(&report.report_id)
            .map_err(|_| "验收报告证据不可用")?
            .ok_or("验收报告尚未持久化")?;
        if persisted_report != *report
            || evidence.goal_id != goal_id
            || evidence.goal_version != goal_version
            || evidence.graph_id != report.graph_id
            || evidence.graph_revision != current_graph_revision
            || evidence.snapshot_id != report.snapshot_id
        {
            return Err("验收报告与持久化证据快照不一致".into());
        }

        let contract = self
            .db
            .load_goal_version(goal_id, goal_version)
            .map_err(|_| "目标契约不可用")?
            .ok_or("目标版本不存在")?;
        if contract.goal_id != goal_id || contract.version != goal_version {
            return Err("验收报告不属于当前目标版本".into());
        }
        let draft = self
            .db
            .confirmed_proof_draft_for_goal(goal_id, goal_version)
            .map_err(|_| "已确认的 ProofLoop 草稿不可用")?
            .ok_or("目标没有已确认的 ProofLoop 草稿")?;
        if draft.draft_id != draft_id || draft.graph_id != report.graph_id {
            return Err("验收报告与实际使用 Skill 的目标草稿不匹配".into());
        }

        self.db.record_skill_usage_outcome(
            goal_id,
            goal_version,
            draft_id,
            &report.report_id,
            feedback_outcome(report),
            report.checked_at_ms,
        )
    }
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::feedback_outcome;
    use crate::modules::task::proofloop::contracts::{AcceptanceReport, GoalStatus};

    fn report(status: GoalStatus) -> AcceptanceReport {
        AcceptanceReport {
            report_id: "report-1".into(),
            goal_id: "goal-1".into(),
            goal_version: 1,
            graph_id: "graph-1".into(),
            graph_revision: 1,
            snapshot_id: "snapshot-1".into(),
            status,
            results: Vec::new(),
            deliverable_results: Vec::new(),
            human_reviews: Vec::new(),
            checked_at_ms: 1,
        }
    }

    #[test]
    fn acceptance_status_maps_to_distinct_feedback_outcomes() {
        assert_eq!(feedback_outcome(&report(GoalStatus::Pass)), "pass");
        assert_eq!(feedback_outcome(&report(GoalStatus::Fail)), "fail");
        assert_eq!(
            feedback_outcome(&report(GoalStatus::Incomplete)),
            "incomplete"
        );
    }
}
