//! Draft insert and compare-and-swap persistence.
use super::*;

impl Database {
    pub fn insert_proof_draft(
        &self,
        draft: &ProofDraft,
        updated_at: i64,
    ) -> Result<(), ProofLoopDbError> {
        self.initialize_proofloop_schema()?;
        if draft.status == DraftStatus::Confirmed || draft.confirmed_graph_revision.is_some() {
            return Err(ProofLoopDbError::Integrity(
                "new proof drafts cannot start in a confirmed state".into(),
            ));
        }
        let payload_json = serde_json::to_string(draft).map_err(|e| ProofLoopDbError::Json {
            field: "payload_json",
            message: e.to_string(),
        })?;

        let status_str = status_to_str(draft.status);
        let conn = self.conn();
        conn.execute(
            "INSERT INTO proofloop_drafts
                (draft_id, revision, goal_id, goal_version, graph_id, workspace_id, user_goal, payload_json, status, confirmed_graph_revision, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10)",
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
                updated_at,
            ],
        )?;
        Ok(())
    }

    /// Compatibility name for creating a new draft; existing rows are never overwritten.
    pub fn save_proof_draft(
        &self,
        draft: &ProofDraft,
        updated_at: i64,
    ) -> Result<(), ProofLoopDbError> {
        self.insert_proof_draft(draft, updated_at)
    }

    pub fn update_proof_draft_if_current(
        &self,
        expected_revision: u32,
        draft: &ProofDraft,
        updated_at: i64,
    ) -> Result<(), ProofLoopDbError> {
        self.initialize_proofloop_schema()?;
        let next_revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| ProofLoopDbError::Integrity("proof draft revision overflow".into()))?;
        if draft.revision != next_revision {
            return Err(ProofLoopDbError::Integrity(format!(
                "updated proof draft revision must be {next_revision}, got {}",
                draft.revision
            )));
        }
        if draft.status == DraftStatus::Confirmed || draft.confirmed_graph_revision.is_some() {
            return Err(ProofLoopDbError::Integrity(
                "ordinary proof draft updates cannot set confirmation state".into(),
            ));
        }

        let payload_json = serde_json::to_string(draft).map_err(|e| ProofLoopDbError::Json {
            field: "payload_json",
            message: e.to_string(),
        })?;
        let status_str = status_to_str(draft.status);

        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current: Option<(u32, String, String, Option<i64>)> = tx
            .query_row(
                "SELECT revision, graph_id, status, confirmed_graph_revision
                 FROM proofloop_drafts WHERE draft_id=?1",
                [&draft.draft_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((actual_revision, current_graph_id, current_status, confirmed_revision)) = current
        else {
            return Err(DraftWriteConflict::NotFound.into());
        };
        if current_status == "confirmed" || confirmed_revision.is_some() {
            return Err(DraftWriteConflict::AlreadyConfirmed.into());
        }
        if actual_revision != expected_revision {
            return Err(DraftWriteConflict::StaleRevision {
                expected: expected_revision,
                actual: actual_revision,
            }
            .into());
        }
        if draft.graph_id != current_graph_id {
            return Err(ProofLoopDbError::Integrity(
                "ordinary proof draft updates cannot change graph_id".into(),
            ));
        }

        let changed = tx.execute(
            "UPDATE proofloop_drafts
             SET revision=?1, goal_id=?2, goal_version=?3,
                 workspace_id=?4, user_goal=?5, payload_json=?6,
                 status=?7, updated_at=?8
             WHERE draft_id=?9
               AND revision=?10
               AND status!='confirmed'
               AND confirmed_graph_revision IS NULL
               AND graph_id=?11",
            params![
                draft.revision,
                draft.contract.goal_id,
                draft.contract.version,
                draft.contract.workspace_id,
                draft.contract.user_goal,
                payload_json,
                status_str,
                updated_at,
                draft.draft_id,
                expected_revision,
                current_graph_id,
            ],
        )?;
        if changed == 1 {
            tx.commit()?;
            return Ok(());
        }

        let current: Option<(u32, String, String, Option<i64>)> = tx
            .query_row(
                "SELECT revision, graph_id, status, confirmed_graph_revision
                 FROM proofloop_drafts WHERE draft_id=?1",
                [&draft.draft_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        match current {
            None => Err(DraftWriteConflict::NotFound.into()),
            Some((_, _, status, Some(_))) if status != "confirmed" => {
                Err(DraftWriteConflict::AlreadyConfirmed.into())
            }
            Some((_, _, status, _)) if status == "confirmed" => {
                Err(DraftWriteConflict::AlreadyConfirmed.into())
            }
            Some((actual, _, _, _)) if actual != expected_revision => {
                Err(DraftWriteConflict::StaleRevision {
                    expected: expected_revision,
                    actual,
                }
                .into())
            }
            Some((_, graph_id, _, _)) if graph_id != draft.graph_id => {
                Err(ProofLoopDbError::Integrity(
                    "ordinary proof draft updates cannot change graph_id".into(),
                ))
            }
            Some((actual, _, _, _)) => Err(ProofLoopDbError::Integrity(format!(
                "proof draft update matched no row at revision {actual}"
            ))),
        }
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
