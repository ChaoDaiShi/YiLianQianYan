//! Goal-scoped accounting for additional model work such as bounded AI repair.
use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::modules::task::proofloop::{GoalContract, PlanningUsage, ProofError};

const MODEL_USAGE:ProductMigrationSpec=ProductMigrationSpec::new(1018,"1018_proofloop_model_invocations",MigrationOwner::V1TaskWorld,
"CREATE TABLE proofloop_model_invocations (invocation_id TEXT PRIMARY KEY,goal_id TEXT NOT NULL,goal_version INTEGER NOT NULL,source TEXT NOT NULL,model_calls INTEGER NOT NULL DEFAULT 1 CHECK(model_calls >= 0),total_tokens INTEGER,elapsed_ms INTEGER,status TEXT NOT NULL,created_at INTEGER NOT NULL); CREATE INDEX proofloop_model_invocations_goal ON proofloop_model_invocations(goal_id,goal_version);");
fn error(e: impl ToString) -> ProofError {
    ProofError::new("proofloop_usage_unavailable", e.to_string())
}
impl Database {
    pub fn initialize_proofloop_model_usage(&self) -> Result<(), ProofError> {
        self.apply_product_migration(&MODEL_USAGE).map_err(error)
    }
    pub fn combined_goal_model_usage(
        &self,
        goal: &GoalContract,
        planning: &PlanningUsage,
    ) -> Result<PlanningUsage, ProofError> {
        self.initialize_proofloop_model_usage()?;
        let conn = self.conn();
        let (count,tokens,unknown):(u32,u64,u32)=conn.query_row("SELECT COALESCE(SUM(model_calls),0),COALESCE(SUM(total_tokens),0),COALESCE(SUM(CASE WHEN total_tokens IS NULL THEN 1 ELSE 0 END),0) FROM proofloop_model_invocations WHERE goal_id=?1 AND goal_version=?2",rusqlite::params![goal.goal_id,goal.version],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(error)?;
        if unknown > 0 {
            return Err(error("额外模型调用的真实用量尚未取得"));
        }
        let mut combined = planning.clone();
        combined.model_calls = combined
            .model_calls
            .checked_add(count)
            .ok_or_else(|| error("模型调用计数无效"))?;
        combined.total_tokens = combined
            .total_tokens
            .map(|value| value.saturating_add(tokens));
        Ok(combined)
    }
    pub fn reserve_goal_model_call(
        &self,
        goal: &GoalContract,
        planning: &PlanningUsage,
        source: &str,
        at: i64,
    ) -> Result<String, ProofError> {
        self.initialize_proofloop_model_usage()?;
        let Some(initial_tokens) = planning.total_tokens else {
            return Err(error("规划用量未知，不能确认剩余模型预算"));
        };
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(error)?;
        let (count,tokens,unknown):(u32,u64,u32)=tx.query_row("SELECT COALESCE(SUM(model_calls),0),COALESCE(SUM(total_tokens),0),COALESCE(SUM(CASE WHEN total_tokens IS NULL THEN 1 ELSE 0 END),0) FROM proofloop_model_invocations WHERE goal_id=?1 AND goal_version=?2",rusqlite::params![goal.goal_id,goal.version],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(error)?;
        if unknown > 0 {
            return Err(error("模型调用尚未完成结算"));
        }
        if planning.model_calls.saturating_add(count) >= goal.budget.max_model_calls
            || initial_tokens.saturating_add(tokens) >= goal.budget.max_total_tokens
        {
            return Err(ProofError::new(
                "proofloop_budget_exhausted",
                "目标的模型预算已耗尽",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO proofloop_model_invocations(invocation_id,goal_id,goal_version,source,total_tokens,elapsed_ms,status,created_at) VALUES(?1,?2,?3,?4,NULL,NULL,'pending',?5)",rusqlite::params![id,goal.goal_id,goal.version,source,at]).map_err(error)?;
        tx.commit().map_err(error)?;
        Ok(id)
    }
    pub fn finish_goal_model_call(
        &self,
        id: &str,
        usage: Option<&PlanningUsage>,
    ) -> Result<(), ProofError> {
        let tokens = usage.and_then(|u| u.total_tokens);
        let elapsed = usage.and_then(|u| u.elapsed_ms);
        let changed=self.conn().execute("UPDATE proofloop_model_invocations SET total_tokens=?2,elapsed_ms=?3,status=?4,model_calls=?5 WHERE invocation_id=?1 AND status='pending'",rusqlite::params![id,tokens,elapsed,if tokens.is_some(){"complete"}else{"unknown"},usage.map(|value|value.model_calls).unwrap_or(1)]).map_err(error)?;
        if changed != 1 {
            return Err(error("模型调用结算记录不存在或已被结算"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Database, GoalContract, PlanningUsage) {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let value: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/proofloop/declared-two-node.json"
        ))
        .unwrap();
        let contract = serde_json::from_value(value["contract"].clone()).unwrap();
        let usage = PlanningUsage {
            model_calls: 1,
            total_tokens: Some(100),
            started_at_ms: 1,
            elapsed_ms: Some(1),
        };
        (db, contract, usage)
    }
    #[test]
    fn session_settlement_counts_real_calls_not_one_per_session() {
        let (db, contract, planning) = fixture();
        let id = db
            .reserve_goal_model_call(&contract, &planning, "goal-chat", 1)
            .unwrap();
        let measured = PlanningUsage {
            model_calls: 3,
            total_tokens: Some(900),
            started_at_ms: 2,
            elapsed_ms: Some(20),
        };
        db.finish_goal_model_call(&id, Some(&measured)).unwrap();
        let total = db.combined_goal_model_usage(&contract, &planning).unwrap();
        assert_eq!(total.model_calls, 4);
        assert_eq!(total.total_tokens, Some(1000));
    }
    #[test]
    fn unmeasured_inflight_session_cannot_be_spent_again() {
        let (db, contract, planning) = fixture();
        let id = db
            .reserve_goal_model_call(&contract, &planning, "ai-repair", 1)
            .unwrap();
        assert_eq!(
            db.reserve_goal_model_call(&contract, &planning, "ai-repair", 2)
                .unwrap_err()
                .code,
            "proofloop_usage_unavailable"
        );
        db.finish_goal_model_call(&id, None).unwrap();
        assert_eq!(
            db.combined_goal_model_usage(&contract, &planning)
                .unwrap_err()
                .code,
            "proofloop_usage_unavailable"
        );
    }
}
