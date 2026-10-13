//! SQLite persistence for ProofLoop goal versions and drafts.
//!
//! Migration 1014 in the v1 Task World namespace (1000-1999).

mod artifact_lookup;
mod drafts;
mod evaluations;
mod timing;
pub(crate) use timing::ProofLoopTimeRecord;

use rusqlite::{params, OptionalExtension};
use thiserror::Error;

use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::modules::task::proofloop::contracts::{
    AcceptanceReport, DraftStatus, EvidenceSnapshot, GoalContract, GoalTimeUsage, ProofDraft,
    ProofDraftSummary, RepairProposal,
};

const PROOFLOOP_SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS proofloop_goal_versions (
    goal_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version > 0),
    workspace_id TEXT NOT NULL,
    contract_json TEXT NOT NULL,
    confirmed_at INTEGER NOT NULL,
    PRIMARY KEY (goal_id, version)
);
CREATE INDEX IF NOT EXISTS idx_proofloop_goal_versions_workspace
    ON proofloop_goal_versions(workspace_id);

CREATE TABLE IF NOT EXISTS proofloop_drafts (
    draft_id TEXT PRIMARY KEY,
    revision INTEGER NOT NULL CHECK (revision > 0),
    goal_id TEXT NOT NULL,
    goal_version INTEGER NOT NULL CHECK (goal_version > 0),
    graph_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    user_goal TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    status TEXT NOT NULL,
    confirmed_graph_revision INTEGER,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_proofloop_drafts_workspace
    ON proofloop_drafts(workspace_id);";

pub(crate) const PROOFLOOP_SCHEMA_MIGRATION: ProductMigrationSpec = ProductMigrationSpec::new(
    1014,
    "1014_proofloop_drafts_and_goals",
    MigrationOwner::V1TaskWorld,
    PROOFLOOP_SCHEMA_SQL,
);

const PROOFLOOP_EVAL_SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS proofloop_acceptance_reports (
    report_id TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL,
    goal_version INTEGER NOT NULL,
    graph_id TEXT NOT NULL,
    graph_revision INTEGER NOT NULL,
    evidence_json TEXT NOT NULL,
    report_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_proofloop_acceptance_reports_goal
    ON proofloop_acceptance_reports(goal_id);

CREATE TABLE IF NOT EXISTS proofloop_repairs (
    repair_id TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL,
    goal_version INTEGER NOT NULL,
    source_report_id TEXT NOT NULL,
    proposal_json TEXT NOT NULL,
    status TEXT NOT NULL,
    applied_revision INTEGER,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_proofloop_repairs_goal
    ON proofloop_repairs(goal_id);";

pub(crate) const PROOFLOOP_EVAL_SCHEMA_MIGRATION: ProductMigrationSpec = ProductMigrationSpec::new(
    1015,
    "1015_proofloop_evaluations_and_repairs",
    MigrationOwner::V1TaskWorld,
    PROOFLOOP_EVAL_SCHEMA_SQL,
);

#[derive(Debug, Error)]
pub enum DraftWriteConflict {
    #[error("proof draft no longer exists")]
    NotFound,
    #[error("proof draft has already been confirmed")]
    AlreadyConfirmed,
    #[error("stale proof draft revision: expected {expected}, actual {actual}")]
    StaleRevision { expected: u32, actual: u32 },
}

#[derive(Debug, Error)]
pub enum ProofLoopDbError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("json error in {field}: {message}")]
    Json {
        field: &'static str,
        message: String,
    },
    #[error("integrity error: {0}")]
    Integrity(String),
    #[error("proof draft write conflict: {0}")]
    DraftConflict(#[from] DraftWriteConflict),
}

fn status_to_str(status: DraftStatus) -> &'static str {
    match status {
        DraftStatus::Proposed => "proposed",
        DraftStatus::NeedsInput => "needs_input",
        DraftStatus::Invalid => "invalid",
        DraftStatus::ReadyForConfirmation => "ready_for_confirmation",
        DraftStatus::Confirmed => "confirmed",
    }
}

fn status_from_str(s: &str) -> Option<DraftStatus> {
    match s {
        "proposed" => Some(DraftStatus::Proposed),
        "needs_input" => Some(DraftStatus::NeedsInput),
        "invalid" => Some(DraftStatus::Invalid),
        "ready_for_confirmation" => Some(DraftStatus::ReadyForConfirmation),
        "confirmed" => Some(DraftStatus::Confirmed),
        _ => None,
    }
}

impl Database {
    pub fn confirmed_proof_draft_for_goal(
        &self,
        goal_id: &str,
        version: u32,
    ) -> Result<Option<ProofDraft>, ProofLoopDbError> {
        self.initialize_proofloop_schema()?;
        let text: Option<String> = self.conn().query_row("SELECT payload_json FROM proofloop_drafts WHERE goal_id=?1 AND goal_version=?2 AND status='confirmed' ORDER BY updated_at DESC LIMIT 1", params![goal_id, version], |row| row.get(0)).optional()?;
        text.map(|value| {
            serde_json::from_str(&value).map_err(|error| ProofLoopDbError::Json {
                field: "payload_json",
                message: error.to_string(),
            })
        })
        .transpose()
    }

    pub fn confirmed_proof_draft_for_graph(
        &self,
        graph_id: &str,
    ) -> Result<Option<ProofDraft>, ProofLoopDbError> {
        self.initialize_proofloop_schema()?;
        let text: Option<String> = self.conn().query_row("SELECT payload_json FROM proofloop_drafts WHERE graph_id=?1 AND status='confirmed' ORDER BY updated_at DESC LIMIT 1", [graph_id], |row| row.get(0)).optional()?;
        text.map(|value| {
            serde_json::from_str(&value).map_err(|error| ProofLoopDbError::Json {
                field: "payload_json",
                message: error.to_string(),
            })
        })
        .transpose()
    }

    pub(crate) fn confirm_proof_draft_with_graph(
        &self,
        supervisor: &crate::modules::task::TaskSupervisor,
        draft: &ProofDraft,
        now: i64,
    ) -> Result<(), super::TaskWorldPersistenceError> {
        let contract = serde_json::to_string(&draft.contract)
            .map_err(|error| super::TaskWorldPersistenceError::Integrity(error.to_string()))?;
        let payload = serde_json::to_string(draft)
            .map_err(|error| super::TaskWorldPersistenceError::Integrity(error.to_string()))?;
        self.save_task_snapshot_with_executions_and_hook(supervisor, now, "created", &[], |tx| {
            tx.execute("INSERT INTO proofloop_goal_versions (goal_id,version,workspace_id,contract_json,confirmed_at) VALUES (?1,?2,?3,?4,?5)",params![draft.contract.goal_id,draft.contract.version,draft.contract.workspace_id,contract,now])?;
            let changed = tx.execute("UPDATE proofloop_drafts SET status='confirmed',confirmed_graph_revision=?1,payload_json=?2,updated_at=?3 WHERE draft_id=?4 AND revision=?5 AND status!='confirmed'",params![supervisor.graph_revision().value(),payload,now,draft.draft_id,draft.revision])?;
            if changed != 1 { return Err(rusqlite::Error::InvalidParameterName("proofloop_stale_revision".into())); }
            Ok(())
        })
    }

    pub fn repair_rounds_used(&self, goal_id: &str, version: u32) -> Result<u32, ProofLoopDbError> {
        self.initialize_proofloop_eval_schema()?;
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM proofloop_repairs WHERE goal_id=?1 AND goal_version=?2",
            params![goal_id, version],
            |row| row.get(0),
        )?)
    }

    pub fn save_repair_proposal_with_budget(
        &self,
        proposal: &mut RepairProposal,
        contract: &GoalContract,
        usage: &GoalTimeUsage,
        active_started: Option<std::time::Instant>,
        now: i64,
    ) -> Result<(), ProofLoopDbError> {
        self.initialize_proofloop_eval_schema()?;
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let used: u32 = tx.query_row(
            "SELECT COUNT(*) FROM proofloop_repairs WHERE goal_id=?1 AND goal_version=?2",
            params![proposal.goal_id, proposal.goal_version],
            |row| row.get(0),
        )?;
        if used >= contract.budget.max_repair_rounds {
            return Err(ProofLoopDbError::Integrity(
                "proofloop_budget_exhausted".into(),
            ));
        }
        let confirmed_at: i64 = tx.query_row(
            "SELECT confirmed_at FROM proofloop_goal_versions WHERE goal_id=?1 AND version=?2",
            params![proposal.goal_id, proposal.goal_version],
            |row| row.get(0),
        )?;
        let draft_json: String = tx.query_row(
            "SELECT payload_json FROM proofloop_drafts WHERE goal_id=?1 AND goal_version=?2 AND status='confirmed' ORDER BY updated_at DESC LIMIT 1",
            params![proposal.goal_id, proposal.goal_version], |row| row.get(0),
        )?;
        let draft: ProofDraft =
            serde_json::from_str(&draft_json).map_err(|error| ProofLoopDbError::Json {
                field: "payload_json",
                message: error.to_string(),
            })?;
        let now = now.max(chrono::Utc::now().timestamp_millis());
        let mut current_usage = usage.clone();
        if let Some(started) = active_started {
            current_usage.execution_active_ms = current_usage
                .execution_active_ms
                .saturating_add(started.elapsed().as_millis().min(u64::MAX as u128) as u64);
        }
        crate::modules::task::proofloop::budget::remaining_budget(
            contract,
            &draft.planning_usage,
            &current_usage,
            confirmed_at,
            now,
        )
        .map_err(|error| ProofLoopDbError::Integrity(error.code))?;
        proposal.round = used + 1;
        let payload = serde_json::to_string(proposal)
            .map_err(|error| ProofLoopDbError::Integrity(error.to_string()))?;
        tx.execute("INSERT INTO proofloop_repairs (repair_id,goal_id,goal_version,source_report_id,proposal_json,status,created_at) VALUES (?1,?2,?3,?4,?5,'proposed',?6)",params![proposal.repair_id,proposal.goal_id,proposal.goal_version,proposal.source_report_id,payload,now])?;
        tx.commit()?;
        Ok(())
    }

    pub fn repair_applied_revision(
        &self,
        repair_id: &str,
    ) -> Result<Option<u64>, ProofLoopDbError> {
        Ok(self.conn().query_row("SELECT applied_revision FROM proofloop_repairs WHERE repair_id=?1 AND status='applied'",[repair_id],|row|row.get::<_, Option<u64>>(0)).optional()?.flatten())
    }

    pub fn goal_confirmed_at(&self, goal_id: &str, version: u32) -> Result<i64, ProofLoopDbError> {
        Ok(self.conn().query_row(
            "SELECT confirmed_at FROM proofloop_goal_versions WHERE goal_id=?1 AND version=?2",
            params![goal_id, version],
            |row| row.get(0),
        )?)
    }

    pub(crate) fn save_proofloop_repair_execution(
        &self,
        supervisor: &crate::modules::task::TaskSupervisor,
        changed: &[crate::modules::task::NodeExecution],
        execution: &crate::modules::task::NodeExecution,
        repair_id: &str,
        now: i64,
    ) -> Result<(), super::TaskWorldPersistenceError> {
        self.save_task_snapshot_with_executions_and_hook(supervisor, now, "proofloop_repaired", changed, |tx| {
            let updated = tx.execute("UPDATE proofloop_repairs SET status='applied',applied_revision=?1 WHERE repair_id=?2 AND status='proposed'",params![supervisor.graph_revision().value(),repair_id])?;
            if updated != 1 { return Err(rusqlite::Error::InvalidParameterName("repair already applied or missing".into())); }
            super::task_execution::insert_node_execution_on(tx, execution).map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))?;
            Ok(())
        })
    }

    pub fn initialize_proofloop_schema(&self) -> Result<(), ProofLoopDbError> {
        self.apply_product_migration(&PROOFLOOP_SCHEMA_MIGRATION)
            .map_err(ProofLoopDbError::Database)
    }

    pub fn save_goal_version(
        &self,
        contract: &GoalContract,
        confirmed_at: i64,
    ) -> Result<(), ProofLoopDbError> {
        self.initialize_proofloop_schema()?;
        let contract_json =
            serde_json::to_string(contract).map_err(|e| ProofLoopDbError::Json {
                field: "contract_json",
                message: e.to_string(),
            })?;

        let conn = self.conn();
        conn.execute(
            "INSERT INTO proofloop_goal_versions
                (goal_id, version, workspace_id, contract_json, confirmed_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(goal_id, version) DO UPDATE SET
                workspace_id=excluded.workspace_id,
                contract_json=excluded.contract_json,
                confirmed_at=excluded.confirmed_at",
            params![
                contract.goal_id,
                contract.version,
                contract.workspace_id,
                contract_json,
                confirmed_at,
            ],
        )?;
        Ok(())
    }

    pub fn load_goal_version(
        &self,
        goal_id: &str,
        version: u32,
    ) -> Result<Option<GoalContract>, ProofLoopDbError> {
        self.initialize_proofloop_schema()?;
        let conn = self.conn();
        let row: Option<String> = conn
            .query_row(
                "SELECT contract_json FROM proofloop_goal_versions WHERE goal_id=?1 AND version=?2",
                params![goal_id, version],
                |row| row.get(0),
            )
            .optional()?;

        match row {
            Some(json_str) => {
                let contract =
                    serde_json::from_str(&json_str).map_err(|e| ProofLoopDbError::Json {
                        field: "contract_json",
                        message: e.to_string(),
                    })?;
                Ok(Some(contract))
            }
            None => Ok(None),
        }
    }
}
