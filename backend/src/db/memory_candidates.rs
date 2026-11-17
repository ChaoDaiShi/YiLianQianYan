//! Review queue for proposals that have not been confirmed as memories.
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use super::{
    memory_governance::CorrectMemoryRequest,
    memory_governance::{content_hash, normalize_memory},
    memory_graph::{resolve_correction, CorrectionSource},
    migrations::{MigrationOwner, ProductMigrationSpec},
    CreateMemoryRequest, Database, Memory, MemoryScope,
};

mod proofloop;
pub(super) mod relationship;
mod saved_text;
pub(super) use proofloop::{live_report_exists, stage_verified_report};
pub(crate) use relationship::{MemoryRelationship, RelationshipCertainty};
pub(crate) use saved_text::{ExtractedMemoryRelationship, SavedTextSource};

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1087,
    "1087_memory_candidates",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE memory_candidates (
        id TEXT PRIMARY KEY,
        category TEXT NOT NULL,
        content TEXT,
        fingerprint TEXT NOT NULL UNIQUE,
        conversation_id TEXT,
        message_id TEXT,
        task_id TEXT,
        task_execution_id TEXT,
        workspace_id TEXT,
        agent_name TEXT,
        created_at INTEGER NOT NULL,
        decision TEXT CHECK(decision IN ('accepted','dismissed')),
        decided_at INTEGER
     );
     CREATE INDEX memory_candidates_pending ON memory_candidates(created_at DESC,id) WHERE decision IS NULL;
     CREATE TABLE memory_candidate_removed_activity (message_id TEXT PRIMARY KEY);
     CREATE TRIGGER memory_candidates_activity_purge BEFORE DELETE ON memory_activity WHEN OLD.message_id IS NOT NULL BEGIN
        INSERT OR IGNORE INTO memory_candidate_removed_activity VALUES(OLD.message_id);
        UPDATE memory_candidates SET content=NULL,decision='dismissed',decided_at=unixepoch()*1000
        WHERE message_id=OLD.message_id AND decision IS NULL;
     END;",
);

const PROOFLOOP_SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1089,
    "1089_memory_candidate_proofloop_sources",
    MigrationOwner::V1TaskWorld,
    "ALTER TABLE memory_candidates ADD COLUMN graph_id TEXT;
     ALTER TABLE memory_candidates ADD COLUMN report_id TEXT;",
);

const RELATIONSHIP_SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1090,
    "1090_memory_candidate_relationship",
    MigrationOwner::V1TaskWorld,
    "ALTER TABLE memory_candidates ADD COLUMN relationship_json TEXT;
     DROP TRIGGER memory_candidates_activity_purge;
     CREATE TRIGGER memory_candidates_activity_purge BEFORE DELETE ON memory_activity WHEN OLD.message_id IS NOT NULL BEGIN
        INSERT OR IGNORE INTO memory_candidate_removed_activity VALUES(OLD.message_id);
        UPDATE memory_candidates SET content=NULL,relationship_json=NULL,decision='dismissed',decided_at=unixepoch()*1000
        WHERE message_id=OLD.message_id AND decision IS NULL;
     END;",
);

#[derive(Clone)]
pub(crate) enum CandidateSource {
    Message {
        conversation_id: String,
        message_id: String,
    },
    Task {
        task_id: String,
        execution_id: String,
        agent_name: String,
    },
}

pub(crate) struct CandidateDraft {
    pub category: String,
    pub content: String,
    pub source: CandidateSource,
    pub relationship: Option<MemoryRelationship>,
}

#[derive(Debug, Serialize)]
pub struct CandidateReview {
    pub id: String,
    pub category: String,
    pub content: String,
    pub created_at: i64,
    pub source_kind: &'static str,
    pub source_title: Option<String>,
    pub source_excerpt: Option<String>,
    pub source_conversation_id: Option<String>,
    pub source_message_id: Option<String>,
    pub source_task_id: Option<String>,
    pub source_execution_id: Option<String>,
    pub source_graph_id: Option<String>,
    pub source_report_id: Option<String>,
    pub source_agent: Option<String>,
    pub workspace_id: Option<String>,
    pub source_removed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_memory_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_memory_revision: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_memory_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) relationship: Option<MemoryRelationship>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) correction: Option<CorrectionSource>,
}

#[derive(Deserialize)]
pub struct AcceptCandidate {
    pub content: String,
    pub category: String,
    pub(crate) scope: MemoryScope,
    pub(crate) relationship: Option<MemoryRelationship>,
}

const SELECT: &str = "SELECT q.id,q.category,q.content,q.created_at,q.conversation_id,q.message_id,q.task_id,q.task_execution_id,q.workspace_id,q.agent_name,
    CASE WHEN q.message_id IS NOT NULL AND m.id IS NOT NULL THEN c.title WHEN e.id IS NOT NULL THEN t.title END,
    CASE WHEN q.message_id IS NOT NULL AND c.id IS NOT NULL THEN substr(m.content,1,1000) END,
    CASE WHEN q.message_id IS NOT NULL THEN m.id IS NULL OR c.id IS NULL ELSE e.id IS NULL OR t.id IS NULL END,
    q.graph_id,q.report_id,q.relationship_json
    FROM memory_candidates q
    LEFT JOIN messages m ON m.id=q.message_id AND m.conversation_id=q.conversation_id AND m.role='user'
    LEFT JOIN conversations c ON c.id=m.conversation_id
    LEFT JOIN task_executions e ON e.id=q.task_execution_id AND e.task_id=q.task_id
    LEFT JOIN tasks t ON t.id=e.task_id
    WHERE q.decision IS NULL";

fn read_candidate(row: &rusqlite::Row<'_>) -> rusqlite::Result<CandidateReview> {
    let message_id: Option<String> = row.get(5)?;
    let graph_id: Option<String> = row.get(13)?;
    let source_removed: bool = graph_id.is_none() && row.get(12)?;
    Ok(CandidateReview {
        id: row.get(0)?,
        category: row.get(1)?,
        content: row.get(2)?,
        created_at: row.get(3)?,
        source_kind: if message_id.is_some() {
            "user_message"
        } else if graph_id.is_some() {
            "proofloop_acceptance"
        } else {
            "task_reflection"
        },
        source_title: row.get(10)?,
        source_excerpt: row.get(11)?,
        source_conversation_id: if source_removed { None } else { row.get(4)? },
        source_message_id: if source_removed { None } else { message_id },
        source_task_id: if source_removed { None } else { row.get(6)? },
        source_execution_id: if source_removed { None } else { row.get(7)? },
        source_graph_id: graph_id,
        source_report_id: row.get(14)?,
        workspace_id: row.get(8)?,
        source_agent: row.get(9)?,
        source_removed,
        source_memory_id: None,
        source_memory_revision: None,
        source_memory_scope: None,
        correction: None,
        relationship: row
            .get::<_, Option<String>>(15)?
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    15,
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?,
    })
}

fn resolve_candidate_graph(
    conn: &rusqlite::Connection,
    candidate: &mut CandidateReview,
) -> rusqlite::Result<()> {
    if let Some((graph, report)) = candidate
        .source_graph_id
        .as_deref()
        .zip(candidate.source_report_id.as_deref())
    {
        if !live_report_exists(conn, graph, report)? {
            candidate.source_removed = true;
            candidate.source_graph_id = None;
            candidate.source_report_id = None;
        }
    }
    Ok(())
}

impl Database {
    pub(super) fn initialize_memory_candidates(&self) -> rusqlite::Result<()> {
        self.apply_product_migrations(&[
            SCHEMA,
            PROOFLOOP_SCHEMA,
            RELATIONSHIP_SCHEMA,
            saved_text::SCHEMA,
        ])
    }

    pub(crate) fn removed_memory_candidate_sources(
        &self,
        conversation: &str,
    ) -> rusqlite::Result<std::collections::HashSet<String>> {
        let conn = self.conn();
        let mut query = conn.prepare("SELECT m.id FROM messages m JOIN memory_candidate_removed_activity a ON a.message_id=m.id WHERE m.conversation_id=?1")?;
        let rows = query.query_map([conversation], |row| row.get(0))?;
        rows.collect()
    }

    pub(crate) fn stage_memory_candidates(
        &self,
        drafts: Vec<CandidateDraft>,
    ) -> Result<Vec<String>, String> {
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let mut created = Vec::new();
        for draft in drafts {
            if draft
                .category
                .parse::<crate::agent::memory::MemoryCategory>()
                .is_err()
            {
                continue;
            }
            let relation = match draft.relationship {
                Some(relation) => match relation.normalized() {
                    Some(valid) => Some(valid),
                    None => continue,
                },
                None => None,
            };
            let encoded = relation.as_ref().map(MemoryRelationship::content);
            let content = encoded.as_deref().unwrap_or(draft.content.trim());
            if content.is_empty()
                || content.chars().count() > 2000
                || crate::safety::contains_sensitive_content(content)
            {
                continue;
            }
            let (conversation_id, message_id, task_id, execution_id, workspace, agent) =
                match &draft.source {
                    CandidateSource::Message {
                        conversation_id,
                        message_id,
                    } => {
                        let source: Option<(String, Option<String>)> = tx.query_row(
                        "SELECT m.content,(SELECT value FROM settings WHERE key='conversation_workspace:'||m.conversation_id)
                         FROM messages m JOIN conversations c ON c.id=m.conversation_id
                         WHERE m.id=?1 AND m.conversation_id=?2 AND m.role='user'
                           AND NOT EXISTS(SELECT 1 FROM memory_candidate_removed_activity a WHERE a.message_id=m.id)",
                        params![message_id, conversation_id], |row| Ok((row.get(0)?, row.get(1)?)),
                    ).optional().map_err(|error| error.to_string())?;
                        let Some((source, workspace)) = source else {
                            continue;
                        };
                        if crate::safety::contains_sensitive_content(&source) {
                            continue;
                        }
                        (
                            Some(conversation_id),
                            Some(message_id),
                            None,
                            None,
                            workspace,
                            None,
                        )
                    }
                    CandidateSource::Task {
                        task_id,
                        execution_id,
                        agent_name,
                    } => {
                        if relation.is_some() {
                            continue;
                        }
                        let workspace: Option<String> = tx.query_row(
                        "SELECT t.workspace_id FROM task_executions e JOIN tasks t ON t.id=e.task_id
                         WHERE e.id=?1 AND t.id=?2 AND e.status IN ('completed','failed')",
                        params![execution_id, task_id], |row| row.get(0),
                    ).optional().map_err(|error| error.to_string())?;
                        let Some(workspace) = workspace else {
                            continue;
                        };
                        if !matches!(draft.category.as_str(), "knowledge" | "note") {
                            continue;
                        }
                        (
                            None,
                            None,
                            Some(task_id),
                            Some(execution_id),
                            Some(workspace),
                            Some(agent_name),
                        )
                    }
                };
            let identity = relation
                .as_ref()
                .map(MemoryRelationship::identity)
                .unwrap_or_else(|| serde_json::json!(normalize_memory(content)));
            let fingerprint = content_hash(
                &serde_json::json!([
                    conversation_id,
                    message_id,
                    task_id,
                    draft.category,
                    identity
                ])
                .to_string(),
            );
            let id = uuid::Uuid::new_v4().to_string();
            let relationship_json = relation
                .map(|relation| serde_json::to_string(&relation))
                .transpose()
                .map_err(|error| error.to_string())?;
            if tx.execute("INSERT INTO memory_candidates(id,category,content,fingerprint,conversation_id,message_id,task_id,task_execution_id,workspace_id,agent_name,created_at,relationship_json)
                VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) ON CONFLICT(fingerprint) DO NOTHING",
                params![id,draft.category,content,fingerprint,conversation_id,message_id,task_id,execution_id,workspace,agent,chrono::Utc::now().timestamp_millis(),relationship_json],
            ).map_err(|error| error.to_string())? > 0 {
                created.push(id);
            }
        }
        tx.commit().map_err(|error| error.to_string())?;
        Ok(created)
    }

    pub(crate) fn memory_candidates(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<CandidateReview>, String> {
        let conn = self.conn();
        let mut query = conn
            .prepare(&format!(
                "{SELECT} ORDER BY q.created_at DESC,q.id DESC LIMIT ?1 OFFSET ?2"
            ))
            .map_err(|error| error.to_string())?;
        let rows = query
            .query_map(
                params![limit.min(101) as i64, offset.min(i64::MAX as usize) as i64],
                read_candidate,
            )
            .map_err(|error| error.to_string())?;
        let mut candidates = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        for candidate in &mut candidates {
            resolve_candidate_graph(&conn, candidate).map_err(|error| error.to_string())?;
            resolve_correction(&conn, candidate)?;
            saved_text::resolve_saved_text(&conn, candidate)?;
        }
        Ok(candidates)
    }

    #[cfg(test)]
    pub(crate) fn accept_memory_candidate(
        &self,
        id: &str,
        reviewed: AcceptCandidate,
    ) -> Result<Memory, String> {
        self.accept_memory_candidate_with_kind(id, reviewed)
            .map(|(memory, _)| memory)
    }

    pub(crate) fn accept_memory_candidate_with_kind(
        &self,
        id: &str,
        reviewed: AcceptCandidate,
    ) -> Result<(Memory, bool), String> {
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let mut candidate = tx
            .query_row(&format!("{SELECT} AND q.id=?1"), [id], read_candidate)
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or("memory_candidate_not_pending")?;
        resolve_candidate_graph(&tx, &mut candidate).map_err(|error| error.to_string())?;
        resolve_correction(&tx, &mut candidate)?;
        saved_text::resolve_saved_text(&tx, &mut candidate)?;
        if candidate.source_kind == "memory_version" {
            let source = saved_text::accept_saved_text(&tx, id, reviewed.relationship)?;
            tx.execute("UPDATE memory_candidates SET decision='accepted',decided_at=?2,content=NULL,relationship_json=NULL WHERE id=?1",
                params![id,chrono::Utc::now().timestamp_millis()]).map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())?;
            drop(conn);
            return self.get_memory(&source).map(|memory| (memory, false));
        }
        if candidate.source_removed {
            return Err("memory_candidate_source_removed".into());
        }
        if let Some(correction) = candidate.correction {
            Database::correct_memory_on(
                &tx,
                &correction.target_memory_id,
                &CorrectMemoryRequest {
                    content: reviewed.content,
                    expected_revision: correction.target_revision,
                    reason: correction.reason,
                    scope: None,
                    source_ref: None,
                },
                None,
            )?;
            super::memory_graph::record_reviewed_correction_on(
                &tx,
                id,
                &correction.target_memory_id,
                correction.target_revision,
            )?;
            tx.execute("UPDATE memory_candidates SET decision='accepted',decided_at=?2,content=NULL WHERE id=?1",
                params![id,chrono::Utc::now().timestamp_millis()]).map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())?;
            drop(conn);
            return self
                .get_memory(&correction.target_memory_id)
                .map(|memory| (memory, true));
        }
        if candidate.source_kind != "user_message"
            && !matches!(reviewed.category.as_str(), "knowledge" | "note")
        {
            return Err("invalid_memory_category".into());
        }
        if reviewed.scope == MemoryScope::Workspace && candidate.workspace_id.is_none() {
            return Err("memory_workspace_required".into());
        }
        let relationship = match (candidate.relationship, reviewed.relationship) {
            (Some(_), Some(relation)) => {
                Some(relation.normalized().ok_or("invalid_memory_relationship")?)
            }
            (None, None) => None,
            _ => return Err("invalid_memory_relationship".into()),
        };
        let mut metadata = serde_json::json!({
            "candidate_id": candidate.id, "source_kind": candidate.source_kind,
            "source_message_id": candidate.source_message_id, "source_task_id": candidate.source_task_id,
            "source_execution_id": candidate.source_execution_id, "source_agent": candidate.source_agent,
            "source_graph_id": candidate.source_graph_id, "source_report_id": candidate.source_report_id,
            "source_workspace_id": candidate.workspace_id,
        });
        let (content, category) = if let Some(relationship) = relationship {
            let content = relationship.content();
            metadata["relationship"] = serde_json::json!(relationship);
            (content, "fact".into())
        } else {
            (reviewed.content.trim().into(), reviewed.category)
        };
        let memory = Self::create_memory_on(
            &tx,
            &CreateMemoryRequest {
                content,
                category,
                source: "manual".into(),
                source_conversation_id: candidate.source_conversation_id,
                metadata: Some(metadata.to_string()),
            },
            reviewed.scope,
            candidate.workspace_id.as_deref(),
        )?;
        tx.execute("UPDATE memory_candidates SET decision='accepted',decided_at=?2,content=NULL,relationship_json=NULL WHERE id=?1", params![id,chrono::Utc::now().timestamp_millis()]).map_err(|error| error.to_string())?;
        tx.commit().map_err(|error| error.to_string())?;
        Ok((memory, true))
    }

    pub(crate) fn dismiss_memory_candidate(&self, id: &str) -> Result<bool, String> {
        self.conn().execute("UPDATE memory_candidates SET decision='dismissed',decided_at=?2,content=NULL,relationship_json=NULL WHERE id=?1 AND decision IS NULL", params![id,chrono::Utc::now().timestamp_millis()])
            .map(|count| count > 0).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests;
