//! Acceptance and repair record persistence.
use super::*;

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
