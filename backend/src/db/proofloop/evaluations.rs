//! Acceptance and repair record persistence.
use super::*;
use rusqlite::{OptionalExtension, TransactionBehavior};

impl Database {
    pub fn initialize_proofloop_eval_schema(&self) -> Result<(), ProofLoopDbError> {
        self.apply_product_migration(&PROOFLOOP_EVAL_SCHEMA_MIGRATION)
            .map_err(ProofLoopDbError::Database)
    }

    pub fn save_acceptance_report(
        &self,
        report: &AcceptanceReport,
        evidence: &EvidenceSnapshot,
        created_at: i64,
    ) -> Result<(), ProofLoopDbError> {
        self.initialize_proofloop_eval_schema()?;
        let evidence_json =
            serde_json::to_string(evidence).map_err(|e| ProofLoopDbError::Json {
                field: "evidence_json",
                message: e.to_string(),
            })?;
        let report_json = serde_json::to_string(report).map_err(|e| ProofLoopDbError::Json {
            field: "report_json",
            message: e.to_string(),
        })?;

        let conn = self.conn();
        conn.execute(
            "INSERT INTO proofloop_acceptance_reports
             (report_id, goal_id, goal_version, graph_id, graph_revision, evidence_json, report_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(report_id) DO UPDATE SET
                report_json = excluded.report_json",
            params![
                report.report_id,
                report.goal_id,
                report.goal_version,
                report.graph_id,
                report.graph_revision,
                evidence_json,
                report_json,
                created_at,
            ],
        )?;
        Ok(())
    }

    pub fn save_acceptance_report_if_current(
        &self,
        report: &AcceptanceReport,
        evidence: &EvidenceSnapshot,
        created_at: i64,
    ) -> Result<(), ProofLoopDbError> {
        self.initialize_proofloop_eval_schema()?;
        let evidence_json =
            serde_json::to_string(evidence).map_err(|error| ProofLoopDbError::Json {
                field: "evidence_json",
                message: error.to_string(),
            })?;
        let report_json =
            serde_json::to_string(report).map_err(|error| ProofLoopDbError::Json {
                field: "report_json",
                message: error.to_string(),
            })?;

        if report.graph_id != evidence.graph_id || report.graph_revision != evidence.graph_revision
        {
            return Err(ProofLoopDbError::Integrity(
                "验收报告与证据快照的图版本不一致".into(),
            ));
        }
        let expected_revision = i64::try_from(evidence.graph_revision)
            .map_err(|_| ProofLoopDbError::Integrity("证据快照的图版本超出持久化范围".into()))?;

        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current_revision: Option<i64> = transaction
            .query_row(
                "SELECT graph_revision FROM task_world_supervisor_snapshots WHERE graph_id=?1",
                [&evidence.graph_id],
                |row| row.get(0),
            )
            .optional()?;
        if current_revision != Some(expected_revision) {
            return Err(ProofLoopDbError::Integrity(
                "任务图版本已改变，请重新验收".into(),
            ));
        }

        for selected in &evidence.executions {
            if selected.graph_id != evidence.graph_id
                || selected.graph_revision != evidence.graph_revision
            {
                return Err(ProofLoopDbError::Integrity(
                    "选定执行与证据快照的图版本不一致".into(),
                ));
            }

            let current: Option<(String, String, String)> = transaction
                .query_row(
                    "SELECT graph_id, node_id, status FROM task_node_executions WHERE execution_id=?1",
                    [&selected.execution_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((graph_id, node_id, status)) = current else {
                return Err(ProofLoopDbError::Integrity(format!(
                    "选定执行 '{}' 已不存在",
                    selected.execution_id
                )));
            };
            if graph_id != evidence.graph_id || node_id != selected.node_id || status == "stale" {
                return Err(ProofLoopDbError::Integrity(format!(
                    "选定执行 '{}' 已失效",
                    selected.execution_id
                )));
            }

            let latest_execution: Option<String> = transaction
                .query_row(
                    "SELECT execution_id FROM task_node_executions
                     WHERE graph_id=?1 AND node_id=?2 ORDER BY attempt DESC LIMIT 1",
                    params![evidence.graph_id, selected.node_id],
                    |row| row.get(0),
                )
                .optional()?;
            if latest_execution.as_deref() != Some(selected.execution_id.as_str()) {
                return Err(ProofLoopDbError::Integrity(format!(
                    "节点 '{}' 已产生更新的执行 attempt",
                    selected.node_id
                )));
            }
        }

        transaction.execute(
            "INSERT INTO proofloop_acceptance_reports
             (report_id, goal_id, goal_version, graph_id, graph_revision, evidence_json, report_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(report_id) DO UPDATE SET
                report_json = excluded.report_json",
            params![
                report.report_id,
                report.goal_id,
                report.goal_version,
                report.graph_id,
                report.graph_revision,
                evidence_json,
                report_json,
                created_at,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_acceptance_reports(
        &self,
        goal_id: &str,
    ) -> Result<Vec<AcceptanceReport>, ProofLoopDbError> {
        self.initialize_proofloop_eval_schema()?;
        let conn = self.conn();
        let mut statement = conn.prepare(
            "SELECT report_json FROM proofloop_acceptance_reports
             WHERE goal_id=?1 ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map([goal_id], |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        })?;

        let mut reports = Vec::new();
        for row in rows {
            let json_str = row?;
            let report: AcceptanceReport =
                serde_json::from_str(&json_str).map_err(|e| ProofLoopDbError::Json {
                    field: "report_json",
                    message: e.to_string(),
                })?;
            reports.push(report);
        }
        Ok(reports)
    }

    pub fn get_acceptance_report(
        &self,
        report_id: &str,
    ) -> Result<Option<(AcceptanceReport, EvidenceSnapshot)>, ProofLoopDbError> {
        self.initialize_proofloop_eval_schema()?;
        let conn = self.conn();
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT report_json, evidence_json FROM proofloop_acceptance_reports WHERE report_id=?1",
                params![report_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        match row {
            Some((report_str, evidence_str)) => {
                let report: AcceptanceReport =
                    serde_json::from_str(&report_str).map_err(|e| ProofLoopDbError::Json {
                        field: "report_json",
                        message: e.to_string(),
                    })?;
                let evidence: EvidenceSnapshot =
                    serde_json::from_str(&evidence_str).map_err(|e| ProofLoopDbError::Json {
                        field: "evidence_json",
                        message: e.to_string(),
                    })?;
                Ok(Some((report, evidence)))
            }
            None => Ok(None),
        }
    }

    pub fn save_repair_proposal(
        &self,
        proposal: &RepairProposal,
        status: &str,
        created_at: i64,
    ) -> Result<(), ProofLoopDbError> {
        self.initialize_proofloop_eval_schema()?;
        let proposal_json =
            serde_json::to_string(proposal).map_err(|e| ProofLoopDbError::Json {
                field: "proposal_json",
                message: e.to_string(),
            })?;

        let conn = self.conn();
        conn.execute(
            "INSERT INTO proofloop_repairs
             (repair_id, goal_id, goal_version, source_report_id, proposal_json, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(repair_id) DO UPDATE SET
                proposal_json = excluded.proposal_json,
                status = excluded.status",
            params![
                proposal.repair_id,
                proposal.goal_id,
                proposal.goal_version,
                proposal.source_report_id,
                proposal_json,
                status,
                created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_repair_proposal(
        &self,
        repair_id: &str,
    ) -> Result<Option<RepairProposal>, ProofLoopDbError> {
        self.initialize_proofloop_eval_schema()?;
        let conn = self.conn();
        let row: Option<String> = conn
            .query_row(
                "SELECT proposal_json FROM proofloop_repairs WHERE repair_id=?1",
                params![repair_id],
                |row| row.get(0),
            )
            .optional()?;

        match row {
            Some(json_str) => {
                let proposal: RepairProposal =
                    serde_json::from_str(&json_str).map_err(|e| ProofLoopDbError::Json {
                        field: "proposal_json",
                        message: e.to_string(),
                    })?;
                Ok(Some(proposal))
            }
            None => Ok(None),
        }
    }

    pub fn update_repair_status(
        &self,
        repair_id: &str,
        status: &str,
        applied_revision: Option<u64>,
    ) -> Result<(), ProofLoopDbError> {
        self.initialize_proofloop_eval_schema()?;
        let conn = self.conn();
        conn.execute(
            "UPDATE proofloop_repairs SET status=?1, applied_revision=?2 WHERE repair_id=?3",
            params![status, applied_revision, repair_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::task::proofloop::contracts::{ExecutionRef, GoalStatus};
    use std::path::Path;

    fn insert_execution(db: &Database, execution_id: &str, attempt: i64) {
        db.conn()
            .execute(
                "INSERT INTO task_node_executions
                 (execution_id,graph_id,node_id,attempt,status,context_json,retry_policy_json,created_at,updated_at)
                 VALUES (?1,'graph','node',?2,'succeeded','{}','{}',1,1)",
                params![execution_id, attempt],
            )
            .unwrap();
    }

    #[test]
    fn acceptance_report_save_rejects_a_newer_node_attempt() {
        let db = Database::new(Path::new(":memory:")).unwrap();
        db.initialize_task_world_schema().unwrap();
        db.conn()
            .execute(
                "INSERT INTO task_world_supervisor_snapshots
                 (graph_id,graph_revision,graph_json,node_states_json,updated_at)
                 VALUES ('graph',1,'{}','[]',1)",
                [],
            )
            .unwrap();
        insert_execution(&db, "execution-1", 1);

        let evidence = EvidenceSnapshot {
            snapshot_id: "snapshot".into(),
            goal_id: "goal".into(),
            goal_version: 1,
            graph_id: "graph".into(),
            graph_revision: 1,
            executions: vec![ExecutionRef {
                graph_id: "graph".into(),
                graph_revision: 1,
                node_id: "node".into(),
                execution_id: "execution-1".into(),
            }],
            evidence: vec![],
            deliverables: vec![],
            sources: vec![],
        };
        let report = AcceptanceReport {
            report_id: "report-current".into(),
            goal_id: "goal".into(),
            goal_version: 1,
            graph_id: "graph".into(),
            graph_revision: 1,
            snapshot_id: "snapshot".into(),
            status: GoalStatus::Pass,
            results: vec![],
            deliverable_results: vec![],
            human_reviews: vec![],
            checked_at_ms: 1,
        };
        db.save_acceptance_report_if_current(&report, &evidence, 1)
            .unwrap();

        insert_execution(&db, "execution-2", 2);
        let mut stale_report = report;
        stale_report.report_id = "report-stale".into();
        let error = db
            .save_acceptance_report_if_current(&stale_report, &evidence, 2)
            .unwrap_err();
        assert!(matches!(error, ProofLoopDbError::Integrity(_)));
        assert!(db.get_acceptance_report("report-stale").unwrap().is_none());
    }
}
