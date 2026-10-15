use rusqlite::params;

use super::{migrations::MigrationOwner, migrations::ProductMigrationSpec, Database};

const SKILL_USAGE: ProductMigrationSpec = ProductMigrationSpec::new(
    1017,
    "1017_proofloop_skill_usage",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE proofloop_skill_usage (
       goal_id TEXT NOT NULL,
       goal_version INTEGER NOT NULL CHECK(goal_version > 0),
       skill_name TEXT NOT NULL,
       skill_version INTEGER NOT NULL CHECK(skill_version > 0),
       draft_id TEXT NOT NULL,
       used_in_planning INTEGER NOT NULL CHECK(used_in_planning IN (0,1)),
       report_id TEXT,
       outcome TEXT NOT NULL CHECK(outcome IN ('pending','pass','fail','incomplete')),
       recorded_at_ms INTEGER NOT NULL,
       PRIMARY KEY(goal_id, goal_version, skill_name, skill_version, draft_id)
     );
     CREATE INDEX idx_proofloop_skill_usage_version
       ON proofloop_skill_usage(skill_name, skill_version, outcome, report_id);",
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillUsageHistory {
    pub goal_id: String,
    pub goal_version: u32,
    pub draft_id: String,
    pub report_id: Option<String>,
    pub outcome: String,
    pub recorded_at_ms: i64,
}

impl Database {
    pub fn initialize_skill_usage(&self) -> Result<(), String> {
        self.apply_product_migration(&SKILL_USAGE)
            .map_err(|error| error.to_string())
    }

    pub fn record_skill_usage_pending(
        &self,
        goal_id: &str,
        goal_version: u32,
        draft_id: &str,
        skill_name: &str,
        skill_version: u32,
        recorded_at_ms: i64,
    ) -> Result<(), String> {
        self.record_skill_usage_pending_batch(
            goal_id,
            goal_version,
            draft_id,
            &[(skill_name.to_owned(), skill_version)],
            recorded_at_ms,
        )
    }

    pub fn record_skill_usage_pending_batch(
        &self,
        goal_id: &str,
        goal_version: u32,
        draft_id: &str,
        skills: &[(String, u32)],
        recorded_at_ms: i64,
    ) -> Result<(), String> {
        if goal_id.trim().is_empty() || draft_id.trim().is_empty() || goal_version == 0 {
            return Err("Skill 使用记录缺少目标版本或草稿 ID".into());
        }
        self.initialize_skill_usage()?;
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| "Skill 使用记录事务不可用")?;
        for (skill_name, skill_version) in skills {
            if skill_name.trim().is_empty() || *skill_version == 0 {
                return Err("Skill 使用记录缺少技能版本".into());
            }
            tx.execute(
                "INSERT INTO proofloop_skill_usage
                 (goal_id,goal_version,skill_name,skill_version,draft_id,used_in_planning,
                  report_id,outcome,recorded_at_ms)
                 VALUES (?1,?2,?3,?4,?5,1,NULL,'pending',?6)
                 ON CONFLICT(goal_id,goal_version,skill_name,skill_version,draft_id) DO NOTHING",
                params![
                    goal_id,
                    goal_version,
                    skill_name,
                    skill_version,
                    draft_id,
                    recorded_at_ms
                ],
            )
            .map_err(|_| "Skill 使用记录保存失败")?;
        }
        tx.commit().map_err(|_| "Skill 使用记录提交失败".into())
    }

    pub fn record_skill_usage_outcome(
        &self,
        goal_id: &str,
        goal_version: u32,
        draft_id: &str,
        report_id: &str,
        outcome: &str,
        recorded_at_ms: i64,
    ) -> Result<usize, String> {
        if !matches!(outcome, "pass" | "fail" | "incomplete") || report_id.trim().is_empty() {
            return Err("Skill 使用结果或报告 ID 无效".into());
        }
        self.initialize_skill_usage()?;
        self.conn()
            .execute(
                "UPDATE proofloop_skill_usage
                 SET report_id=?4, outcome=?5, recorded_at_ms=?6
                 WHERE goal_id=?1 AND goal_version=?2 AND draft_id=?3
                   AND used_in_planning=1",
                params![
                    goal_id,
                    goal_version,
                    draft_id,
                    report_id,
                    outcome,
                    recorded_at_ms
                ],
            )
            .map_err(|_| "Skill 验收结果保存失败".into())
    }

    pub fn load_skill_usage_history(
        &self,
        skill_name: &str,
        skill_version: u32,
    ) -> Result<Vec<SkillUsageHistory>, String> {
        self.initialize_skill_usage()?;
        let conn = self.conn();
        let mut query = conn
            .prepare(
                "SELECT goal_id,goal_version,draft_id,report_id,outcome,recorded_at_ms
                 FROM proofloop_skill_usage
                 WHERE skill_name=?1 AND skill_version=?2 AND used_in_planning=1
                 ORDER BY recorded_at_ms DESC,goal_id,goal_version,draft_id",
            )
            .map_err(|_| "Skill 使用历史不可用")?;
        query
            .query_map(params![skill_name, skill_version], |row| {
                Ok(SkillUsageHistory {
                    goal_id: row.get(0)?,
                    goal_version: row.get(1)?,
                    draft_id: row.get(2)?,
                    report_id: row.get(3)?,
                    outcome: row.get(4)?,
                    recorded_at_ms: row.get(5)?,
                })
            })
            .map_err(|_| "Skill 使用历史不可用")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Skill 使用历史无效".into())
    }
}

#[cfg(test)]
mod tests {
    use crate::db::Database;

    #[test]
    fn finalized_skill_usage_updates_the_same_draft_record_on_retry() {
        let path = std::env::temp_dir().join(format!(
            "yilian-skill-usage-test-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(&path).unwrap();
        db.initialize_skill_usage().unwrap();

        db.record_skill_usage_pending("goal-1", 2, "draft-1", "demo", 3, 100)
            .unwrap();
        db.record_skill_usage_pending("goal-1", 2, "draft-1", "demo", 3, 101)
            .unwrap();
        db.record_skill_usage_outcome("goal-1", 2, "draft-1", "report-1", "pass", 200)
            .unwrap();
        db.record_skill_usage_outcome("goal-1", 2, "draft-1", "report-1", "pass", 201)
            .unwrap();

        let rows = db.load_skill_usage_history("demo", 3).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].report_id.as_deref(), Some("report-1"));
        assert_eq!(rows[0].outcome, "pass");

        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
