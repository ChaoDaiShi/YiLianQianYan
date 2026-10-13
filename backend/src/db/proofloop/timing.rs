//! Persist only ProofLoop activity; approval and review gaps are not execution.
use super::*;
use crate::modules::task::proofloop::GoalTimeUsage;

const TIME_MIGRATION: ProductMigrationSpec = ProductMigrationSpec::new(
    1016,
    "1016_proofloop_active_time",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE proofloop_time_usage (
        goal_id TEXT NOT NULL, goal_version INTEGER NOT NULL,
        planning_elapsed_ms INTEGER, execution_active_ms INTEGER NOT NULL DEFAULT 0,
        active_count INTEGER NOT NULL DEFAULT 0, active_started_at_ms INTEGER,
        human_wait_ms INTEGER, accounting_state TEXT NOT NULL, updated_at INTEGER NOT NULL,
        PRIMARY KEY(goal_id,goal_version));",
);

pub(crate) struct ProofLoopTimeRecord {
    pub usage: GoalTimeUsage,
    pub active_count: u32,
    pub active_started_at_ms: Option<i64>,
}

impl Database {
    pub(crate) fn initialize_proofloop_time(&self) -> Result<(), ProofLoopDbError> {
        self.apply_product_migration(&TIME_MIGRATION)?;
        Ok(())
    }

    pub(crate) fn recover_proofloop_time(&self) -> Result<(), ProofLoopDbError> {
        self.initialize_proofloop_time()?;
        self.conn().execute(
            "UPDATE proofloop_time_usage SET accounting_state='interrupted_unknown',
             active_count=0,active_started_at_ms=NULL WHERE active_count>0",
            [],
        )?;
        Ok(())
    }

    pub(crate) fn proofloop_has_execution_history(
        &self,
        graph_id: &str,
    ) -> Result<bool, ProofLoopDbError> {
        Ok(self.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM task_node_executions WHERE graph_id=?1
             AND (finished_at IS NOT NULL OR attempt>1))",
            [graph_id],
            |row| row.get(0),
        )?)
    }

    pub(crate) fn load_proofloop_time(
        &self,
        goal_id: &str,
        version: u32,
    ) -> Result<Option<ProofLoopTimeRecord>, ProofLoopDbError> {
        self.initialize_proofloop_time()?;
        Ok(self.conn().query_row(
            "SELECT planning_elapsed_ms,execution_active_ms,human_wait_ms,accounting_state,
             active_count,active_started_at_ms FROM proofloop_time_usage WHERE goal_id=?1 AND goal_version=?2",
            params![goal_id,version], |row| Ok(ProofLoopTimeRecord {
                usage: GoalTimeUsage { planning_elapsed_ms: row.get(0)?, execution_active_ms: row.get(1)?,
                    human_wait_ms: row.get(2)?, accounting_state: row.get(3)? },
                active_count: row.get(4)?, active_started_at_ms: row.get(5)?,
            }),
        ).optional()?)
    }

    pub(crate) fn save_proofloop_time(
        &self,
        goal_id: &str,
        version: u32,
        record: &ProofLoopTimeRecord,
        now: i64,
    ) -> Result<(), ProofLoopDbError> {
        self.initialize_proofloop_time()?;
        self.conn().execute(
            "INSERT INTO proofloop_time_usage(goal_id,goal_version,planning_elapsed_ms,execution_active_ms,
             human_wait_ms,accounting_state,active_count,active_started_at_ms,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(goal_id,goal_version) DO UPDATE SET
             planning_elapsed_ms=excluded.planning_elapsed_ms,execution_active_ms=excluded.execution_active_ms,
             human_wait_ms=excluded.human_wait_ms,accounting_state=excluded.accounting_state,
             active_count=excluded.active_count,active_started_at_ms=excluded.active_started_at_ms,updated_at=excluded.updated_at",
            params![goal_id, version, record.usage.planning_elapsed_ms, record.usage.execution_active_ms,
                record.usage.human_wait_ms, record.usage.accounting_state, record.active_count,
                record.active_started_at_ms, now],
        )?;
        Ok(())
    }
}
