//! SQLite persistence for ProofLoop goal versions and drafts.
//!
//! Migration 1014 in the v1 Task World namespace (1000-1999).

use rusqlite::{params, OptionalExtension};
use thiserror::Error;

use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::modules::task::proofloop::contracts::{
    DraftStatus, GoalContract, ProofDraft, ProofDraftSummary,
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

    pub fn save_proof_draft(
        &self,
        draft: &ProofDraft,
        updated_at: i64,
    ) -> Result<(), ProofLoopDbError> {
        self.initialize_proofloop_schema()?;
        let payload_json = serde_json::to_string(draft).map_err(|e| ProofLoopDbError::Json {
            field: "payload_json",
            message: e.to_string(),
        })?;

        let status_str = status_to_str(draft.status);
        let confirmed_rev = draft.confirmed_graph_revision.map(|r| r as i64);

        let conn = self.conn();
        conn.execute(
            "INSERT INTO proofloop_drafts
                (draft_id, revision, goal_id, goal_version, graph_id, workspace_id, user_goal, payload_json, status, confirmed_graph_revision, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(draft_id) DO UPDATE SET
                revision=excluded.revision,
                goal_id=excluded.goal_id,
                goal_version=excluded.goal_version,
                graph_id=excluded.graph_id,
                workspace_id=excluded.workspace_id,
                user_goal=excluded.user_goal,
                payload_json=excluded.payload_json,
                status=excluded.status,
                confirmed_graph_revision=excluded.confirmed_graph_revision,
                updated_at=excluded.updated_at",
            params![
                draft.draft_id,
                draft.revision,
                draft.contract.goal_id,
                draft.contract.version,
                draft.graph_id,
                draft.contract.workspace_id,
                draft.contract.user_goal,
                payload_json,
                status_str,
                confirmed_rev,
                updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn load_proof_draft(&self, draft_id: &str) -> Result<Option<ProofDraft>, ProofLoopDbError> {
        self.initialize_proofloop_schema()?;
        let conn = self.conn();
        let row: Option<String> = conn
            .query_row(
                "SELECT payload_json FROM proofloop_drafts WHERE draft_id=?1",
                params![draft_id],
                |row| row.get(0),
            )
            .optional()?;

        match row {
            Some(json_str) => {
                let draft =
                    serde_json::from_str(&json_str).map_err(|e| ProofLoopDbError::Json {
                        field: "payload_json",
                        message: e.to_string(),
                    })?;
                Ok(Some(draft))
            }
            None => Ok(None),
        }
    }

    pub fn list_proof_drafts(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<ProofDraftSummary>, ProofLoopDbError> {
        self.initialize_proofloop_schema()?;
        let conn = self.conn();

        let mut statement = match workspace_id {
            Some(_) => conn.prepare(
                "SELECT draft_id, revision, graph_id, goal_id, user_goal, status
                 FROM proofloop_drafts WHERE workspace_id=?1 ORDER BY updated_at DESC",
            )?,
            None => conn.prepare(
                "SELECT draft_id, revision, graph_id, goal_id, user_goal, status
                 FROM proofloop_drafts ORDER BY updated_at DESC",
            )?,
        };

        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<ProofDraftSummary> {
            let draft_id: String = row.get(0)?;
            let revision: u32 = row.get(1)?;
            let graph_id: String = row.get(2)?;
            let goal_id: String = row.get(3)?;
            let user_goal: String = row.get(4)?;
            let status_str: String = row.get(5)?;
            let status = status_from_str(&status_str).unwrap_or(DraftStatus::Proposed);

            Ok(ProofDraftSummary {
                draft_id,
                revision,
                graph_id,
                goal_id,
                user_goal,
                status,
            })
        };

        let rows = match workspace_id {
            Some(ws) => statement
                .query_map([ws], map_row)?
                .collect::<Result<Vec<_>, _>>()?,
            None => statement
                .query_map([], map_row)?
                .collect::<Result<Vec<_>, _>>()?,
        };

        Ok(rows)
    }
}
