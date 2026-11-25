//! Profile-local authoritative memory revisions and embedding compare-and-set.
use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database, Memory,
};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod embedding;
mod graph_versions;

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1040,
    "1040_memory_governance",
    MigrationOwner::V1TaskWorld,
    r#"
CREATE TABLE memory_governance (
 memory_id TEXT PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
 revision INTEGER NOT NULL DEFAULT 1, status TEXT NOT NULL DEFAULT 'active',
 scope TEXT NOT NULL DEFAULT 'profile', source_ref TEXT, reason TEXT NOT NULL DEFAULT '',
 embedding_revision INTEGER, embedding_hash TEXT, embedding_model TEXT, embedding_dimension INTEGER,
 embedding_generation INTEGER, valid_from INTEGER, valid_until INTEGER
);
CREATE TABLE memory_versions (
 memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE, revision INTEGER NOT NULL,
 content TEXT NOT NULL, category TEXT NOT NULL, source TEXT NOT NULL, source_ref TEXT,
 scope TEXT NOT NULL, status TEXT NOT NULL, reason TEXT NOT NULL,
 recorded_at INTEGER NOT NULL, valid_from INTEGER, valid_until INTEGER,
 PRIMARY KEY(memory_id,revision)
);
CREATE TABLE memory_embedding_config (id INTEGER PRIMARY KEY CHECK(id=1), model TEXT NOT NULL, generation INTEGER NOT NULL);
INSERT INTO memory_embedding_config VALUES(1,'',0);
INSERT INTO memory_governance(memory_id,source_ref) SELECT id,COALESCE(source_conversation_id,CASE WHEN json_valid(metadata) THEN json_extract(metadata,'$.source_task_id') END) FROM memories;
INSERT INTO memory_versions SELECT m.id,1,m.content,m.category,m.source,g.source_ref,g.scope,g.status,'legacy import; real-world validity unknown',m.updated_at,NULL,NULL FROM memories m JOIN memory_governance g ON g.memory_id=m.id;
CREATE INDEX memory_governance_status_scope ON memory_governance(status,scope);
CREATE TRIGGER memory_governance_created AFTER INSERT ON memories BEGIN
 INSERT INTO memory_governance(memory_id,source_ref) VALUES(NEW.id,COALESCE(NEW.source_conversation_id,CASE WHEN json_valid(NEW.metadata) THEN json_extract(NEW.metadata,'$.source_task_id') END));
 INSERT INTO memory_versions SELECT NEW.id,1,NEW.content,NEW.category,NEW.source,source_ref,'profile','active','created',NEW.updated_at,NULL,NULL FROM memory_governance WHERE memory_id=NEW.id;
END;
CREATE TRIGGER memory_governance_changed AFTER UPDATE OF content,category,metadata,source,source_conversation_id ON memories
WHEN NEW.content IS NOT OLD.content OR NEW.category IS NOT OLD.category OR NEW.metadata IS NOT OLD.metadata OR NEW.source IS NOT OLD.source OR NEW.source_conversation_id IS NOT OLD.source_conversation_id
BEGIN
 UPDATE memory_governance SET revision=revision+1,
 embedding_revision=CASE WHEN NEW.content IS OLD.content AND embedding_revision=revision THEN revision+1 ELSE NULL END,
 embedding_hash=CASE WHEN NEW.content IS OLD.content THEN embedding_hash ELSE NULL END,
 embedding_model=CASE WHEN NEW.content IS OLD.content THEN embedding_model ELSE NULL END,
 embedding_dimension=CASE WHEN NEW.content IS OLD.content THEN embedding_dimension ELSE NULL END,
 embedding_generation=CASE WHEN NEW.content IS OLD.content THEN embedding_generation ELSE NULL END
 WHERE memory_id=NEW.id;
 UPDATE memories SET embedding=NULL WHERE id=NEW.id AND NEW.content IS NOT OLD.content;
 INSERT INTO memory_versions SELECT NEW.id,revision,NEW.content,NEW.category,NEW.source,source_ref,scope,status,reason,NEW.updated_at,valid_from,valid_until FROM memory_governance WHERE memory_id=NEW.id;
END;
CREATE TRIGGER memory_governance_deleted AFTER DELETE ON memories BEGIN
 DELETE FROM memory_versions WHERE memory_id=OLD.id;
 DELETE FROM memory_governance WHERE memory_id=OLD.id;
END;
"#,
);

#[derive(Debug, Clone, Serialize)]
pub struct MemoryGovernance {
    pub memory_id: String,
    pub revision: i64,
    pub status: String,
    pub scope: String,
    pub source_ref: Option<String>,
    pub reason: String,
    pub content_hash: String,
    #[serde(skip_serializing)]
    pub stored_embedding_hash: Option<String>,
    pub embedding_revision: Option<i64>,
    pub embedding_model: Option<String>,
    pub embedding_dimension: Option<i64>,
    pub embedding_generation: Option<i64>,
    pub valid_from: Option<i64>,
    pub valid_until: Option<i64>,
}

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize)]
pub struct MemoryVersion {
    pub version_id: String,
    pub layer: usize,
    pub baseline: bool,
    pub relationship_known: bool,
    pub relationship: Option<serde_json::Value>,
    pub source_conversation_id: Option<String>,
    pub source_message_id: Option<String>,
    pub source_removed: bool,
    pub revision: i64,
    pub content: String,
    pub category: String,
    pub source: String,
    pub source_ref: Option<String>,
    pub scope: String,
    pub status: String,
    pub reason: String,
    pub recorded_at: i64,
    pub valid_from: Option<i64>,
    pub valid_until: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CorrectMemoryRequest {
    pub content: String,
    pub expected_revision: i64,
    pub reason: String,
    pub scope: Option<String>,
    pub source_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WithdrawMemoryRequest {
    pub expected_revision: i64,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreMemoryVersion {
    pub expected_revision: i64,
    pub version_id: String,
}

#[derive(Debug, Clone)]
pub struct MemoryEmbeddingTicket {
    pub memory_id: String,
    pub revision: i64,
    pub content_hash: String,
    pub model: String,
    pub generation: i64,
}

pub fn content_hash(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

/// A usable embedding must represent the current exact text and have a nonzero direction.
pub(crate) fn current_embedding_vector(
    content: &str,
    serialized: Option<&str>,
    hash: Option<&str>,
    dimension: Option<usize>,
) -> Option<Vec<f32>> {
    let vector = super::memories::parse_embedding(serialized?)?;
    (Some(vector.len()) == dimension
        && vector.iter().any(|&value| value != 0.0)
        && hash == Some(content_hash(content).as_str()))
    .then_some(vector)
}
/// Endpoint fingerprint separates equal model names served by different providers.
/// Persist no API credentials or endpoint query strings in memory metadata.
pub fn embedding_identity(config: &crate::config::types::ModelConfig) -> String {
    format!(
        "{}@{:x}",
        config.embedding_model,
        Sha256::digest(config.embedding_base_url.trim_end_matches('/').as_bytes())
    )
}
pub fn normalize_memory(content: &str) -> String {
    content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn require_chat_memory_scope(
    tx: &Transaction<'_>,
    id: &str,
    conversation_id: Option<&str>,
) -> Result<(), String> {
    let Some(conversation_id) = conversation_id else {
        return Ok(());
    };
    let available: bool = tx
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM memory_governance g
                JOIN conversations c ON c.id=?2
                LEFT JOIN settings w ON w.key='conversation_workspace:'||c.id
                WHERE g.memory_id=?1 AND g.status='active'
                  AND (g.valid_from IS NULL OR g.valid_from<=?3)
                  AND (g.valid_until IS NULL OR g.valid_until>?3)
                  AND (g.scope='profile' OR g.scope='conversation:'||c.id
                       OR g.scope='workspace:'||w.value)
            )",
            params![id, conversation_id, chrono::Utc::now().timestamp_millis()],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !available {
        return Err("memory_not_in_scope".into());
    }
    Ok(())
}

impl Database {
    pub fn memory_is_available(
        &self,
        id: &str,
        conversation_id: Option<&str>,
    ) -> Result<bool, String> {
        self.memory_is_available_in_workspace(id, conversation_id, None)
    }
    pub fn memory_is_available_in_workspace(
        &self,
        id: &str,
        conversation_id: Option<&str>,
        workspace_id: Option<&str>,
    ) -> Result<bool, String> {
        let scope = conversation_id.map(|id| format!("conversation:{id}"));
        let workspace = workspace_id.map(|id| format!("workspace:{id}"));
        self.conn().query_row("SELECT EXISTS(SELECT 1 FROM memory_governance WHERE memory_id=?1 AND status='active' AND (scope='profile' OR scope=?2 OR scope=?3) AND (valid_from IS NULL OR valid_from<=?4) AND (valid_until IS NULL OR valid_until>?4))",params![id,scope,workspace,chrono::Utc::now().timestamp_millis()],|r|r.get(0)).map_err(|e|e.to_string())
    }
    pub fn memory_duplicate_exists(&self, category: &str, content: &str) -> Result<bool, String> {
        let conn = self.conn();
        let mut stmt=conn.prepare("SELECT m.content FROM memories m JOIN memory_governance g ON g.memory_id=m.id WHERE m.category=?1 AND g.status='active' AND g.scope='profile' AND m.source!='document' AND NOT EXISTS(SELECT 1 FROM knowledge_document_chunks c WHERE c.memory_id=m.id)").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map([category], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let normalized = normalize_memory(content);
        for row in rows {
            if normalize_memory(&row.map_err(|e| e.to_string())?) == normalized {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn withdrawn_memories(&self) -> Result<Vec<Memory>, String> {
        let ids = {
            let conn = self.conn();
            let mut stmt=conn.prepare("SELECT memory_id FROM memory_governance WHERE status='withdrawn' ORDER BY memory_id").map_err(|e|e.to_string())?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };
        ids.iter().map(|id| self.get_memory(id)).collect()
    }

    /// Includes vectors made by a different model; dimension equality is insufficient.
    pub fn memories_needing_embedding(
        &self,
        model: &str,
        limit: usize,
    ) -> Result<Vec<Memory>, String> {
        let ids = {
            let conn = self.conn();
            let mut stmt = conn
                .prepare(
                    "SELECT m.id,m.content,g.embedding_hash,g.embedding_dimension,
                        CASE WHEN g.embedding_revision=g.revision AND g.embedding_model=?1
                                  AND g.embedding_generation=cfg.generation THEN m.embedding END
                 FROM memories m JOIN memory_governance g ON g.memory_id=m.id
                 LEFT JOIN memory_embedding_config cfg ON cfg.id=1 AND cfg.model=?1
                 WHERE g.status='active' AND (g.valid_from IS NULL OR g.valid_from<=?2)
                   AND (g.valid_until IS NULL OR g.valid_until>?2)
                 ORDER BY m.created_at,m.id",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(
                    params![model, chrono::Utc::now().timestamp_millis()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<i64>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                        ))
                    },
                )
                .map_err(|e| e.to_string())?;
            let mut pending = Vec::new();
            for row in rows {
                if pending.len() >= limit {
                    break;
                }
                let (id, content, hash, dimension, embedding) = row.map_err(|e| e.to_string())?;
                if current_embedding_vector(
                    &content,
                    embedding.as_deref(),
                    hash.as_deref(),
                    dimension.and_then(|value| usize::try_from(value).ok()),
                )
                .is_none()
                {
                    pending.push(id);
                }
            }
            pending
        };
        ids.iter().map(|id| self.get_memory(id)).collect()
    }

    pub fn migrate_memory_governance(&self) -> Result<(), String> {
        self.apply_product_migration(&SCHEMA)
            .map_err(|e| e.to_string())
    }

    pub fn memory_governance(&self, id: &str) -> Result<MemoryGovernance, String> {
        self.conn().query_row("SELECT g.memory_id,g.revision,g.status,g.scope,g.source_ref,g.reason,m.content,g.embedding_revision,g.embedding_model,g.embedding_dimension,g.embedding_generation,g.valid_from,g.valid_until,g.embedding_hash FROM memory_governance g JOIN memories m ON m.id=g.memory_id WHERE g.memory_id=?1", [id], |r| Ok(MemoryGovernance {
            memory_id:r.get(0)?,revision:r.get(1)?,status:r.get(2)?,scope:r.get(3)?,source_ref:r.get(4)?,reason:r.get(5)?,content_hash:content_hash(&r.get::<_,String>(6)?),embedding_revision:r.get(7)?,embedding_model:r.get(8)?,embedding_dimension:r.get(9)?,embedding_generation:r.get(10)?,valid_from:r.get(11)?,valid_until:r.get(12)?,stored_embedding_hash:r.get(13)?,
        })).map_err(|e| e.to_string())
    }

    pub fn correct_memory(&self, id: &str, req: &CorrectMemoryRequest) -> Result<Memory, String> {
        self.correct_memory_scoped(id, req, None)
    }

    pub(crate) fn correct_memory_scoped(
        &self,
        id: &str,
        req: &CorrectMemoryRequest,
        conversation_id: Option<&str>,
    ) -> Result<Memory, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        Self::correct_memory_on(&tx, id, req, conversation_id)?;
        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);
        self.get_memory(id)
    }

    pub(super) fn correct_memory_on(
        tx: &Transaction<'_>,
        id: &str,
        req: &CorrectMemoryRequest,
        conversation_id: Option<&str>,
    ) -> Result<(), String> {
        Self::apply_memory_correction_on(tx, id, req, conversation_id, false)
    }

    pub fn restore_memory_version(
        &self,
        id: &str,
        req: &RestoreMemoryVersion,
    ) -> Result<Memory, String> {
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let version = Self::memory_histories_on(&tx, &[id.to_owned()])?
            .remove(id)
            .and_then(|history| {
                history.into_iter().find(|v| {
                    v.version_id == req.version_id
                        && v.status == "active"
                        && v.revision < req.expected_revision
                })
            })
            .ok_or("memory_version_unavailable")?;
        let (category, metadata): (String, Option<String>) = tx
            .query_row(
                "SELECT category,metadata FROM memories WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| error.to_string())?;
        if category != version.category
            || version.relationship.is_some()
            || super::memory_candidates::relationship::read_metadata(metadata.as_deref()).is_some()
        {
            return Err("memory_version_requires_reviewed_correction".into());
        }
        Self::apply_memory_correction_on(
            &tx,
            id,
            &CorrectMemoryRequest {
                content: version.content,
                expected_revision: req.expected_revision,
                reason: format!("恢复历史版本 v{}", version.revision),
                scope: None,
                source_ref: version.source_ref,
            },
            None,
            true,
        )?;
        tx.commit().map_err(|error| error.to_string())?;
        drop(conn);
        self.get_memory(id)
    }

    fn apply_memory_correction_on(
        tx: &Transaction<'_>,
        id: &str,
        req: &CorrectMemoryRequest,
        conversation_id: Option<&str>,
        restore: bool,
    ) -> Result<(), String> {
        let content = req.content.trim();
        if content.is_empty() || content.chars().count() > 2000 || req.reason.trim().is_empty() {
            return Err("invalid_memory_correction".into());
        }
        if crate::safety::contains_sensitive_content(content) {
            return Err("sensitive_memory_content".into());
        }
        require_chat_memory_scope(tx, id, conversation_id)?;
        let owned: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM knowledge_document_chunks WHERE memory_id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if owned {
            return Err("knowledge_document_owned_chunk".into());
        }
        let (old_content, scope, source_ref): (String, String, Option<String>) = tx
            .query_row(
                "SELECT m.content,g.scope,g.source_ref FROM memory_governance g
                 JOIN memories m ON m.id=g.memory_id
                 WHERE g.memory_id=?1 AND g.revision=?2 AND (g.status='active' OR (?3 AND g.status='withdrawn'))",
                params![id, req.expected_revision, restore],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|err| err.to_string())?
            .ok_or("memory_revision_conflict")?;
        if !restore
            && old_content.trim() == content
            && req
                .scope
                .as_ref()
                .is_none_or(|requested| requested == &scope)
            && req
                .source_ref
                .as_ref()
                .is_none_or(|requested| Some(requested) == source_ref.as_ref())
        {
            return Ok(());
        }
        if let Some(scope) = req.scope.as_deref() {
            if scope != "profile" {
                let exists:bool=if let Some(conversation)=scope.strip_prefix("conversation:") {
                    tx.query_row("SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1)",[conversation],|r|r.get(0))
                } else if let Some(workspace)=scope.strip_prefix("workspace:") {
                    tx.query_row("SELECT EXISTS(SELECT 1 FROM workspaces WHERE id=?1 AND status='active')",[workspace],|r|r.get(0))
                } else {return Err("invalid_memory_scope".into());}.map_err(|e|e.to_string())?;
                if !exists {
                    return Err("invalid_memory_scope".into());
                }
            }
        }
        let changed=tx.execute("UPDATE memory_governance SET reason=?1,scope=COALESCE(?2,scope),source_ref=CASE WHEN ?6 THEN ?3 ELSE COALESCE(?3,source_ref) END,valid_until=CASE WHEN status='withdrawn' THEN NULL ELSE valid_until END,status='active' WHERE memory_id=?4 AND revision=?5 AND (status='active' OR (?6 AND status='withdrawn'))",params![req.reason,req.scope,req.source_ref,id,req.expected_revision,restore]).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("memory_revision_conflict".into());
        }
        let now = chrono::Utc::now().timestamp_millis();
        // Actual scope or provenance changes also count when the content is unchanged.
        tx.execute(
            "UPDATE memories SET content=?1,updated_at=?2 WHERE id=?3",
            params![
                if old_content.trim() == content {
                    &old_content
                } else {
                    content
                },
                now,
                id
            ],
        )
        .map_err(|e| e.to_string())?;
        let current: i64 = tx
            .query_row(
                "SELECT revision FROM memory_governance WHERE memory_id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if current == req.expected_revision {
            tx.execute("UPDATE memory_governance SET revision=revision+1,embedding_revision=NULL,embedding_hash=NULL,embedding_model=NULL,embedding_dimension=NULL,embedding_generation=NULL WHERE memory_id=?1",[id]).map_err(|e|e.to_string())?;
            tx.execute("UPDATE memories SET embedding=NULL WHERE id=?1", [id])
                .map_err(|e| e.to_string())?;
            tx.execute("INSERT INTO memory_versions SELECT m.id,g.revision,m.content,m.category,m.source,g.source_ref,g.scope,g.status,g.reason,?2,g.valid_from,g.valid_until FROM memories m JOIN memory_governance g ON g.memory_id=m.id WHERE m.id=?1",params![id,now]).map_err(|e|e.to_string())?;
        }
        Ok(())
    }

    pub fn withdraw_memory(
        &self,
        id: &str,
        req: &WithdrawMemoryRequest,
    ) -> Result<MemoryGovernance, String> {
        self.withdraw_memory_scoped(id, req, None)
    }

    pub(crate) fn withdraw_memory_scoped(
        &self,
        id: &str,
        req: &WithdrawMemoryRequest,
        conversation_id: Option<&str>,
    ) -> Result<MemoryGovernance, String> {
        if req.reason.trim().is_empty() {
            return Err("invalid_memory_reason".into());
        }
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        require_chat_memory_scope(&tx, id, conversation_id)?;
        let owned: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM knowledge_document_chunks WHERE memory_id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if owned {
            return Err("knowledge_document_owned_chunk".into());
        }
        let now = chrono::Utc::now().timestamp_millis();
        let changed=tx.execute("UPDATE memory_governance SET status='withdrawn',revision=revision+1,reason=?1,valid_until=?2,embedding_revision=NULL,embedding_hash=NULL,embedding_model=NULL,embedding_dimension=NULL,embedding_generation=NULL WHERE memory_id=?3 AND revision=?4 AND status='active'",params![req.reason,now,id,req.expected_revision]).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("memory_revision_conflict".into());
        }
        tx.execute(
            "UPDATE memories SET embedding=NULL,updated_at=?1 WHERE id=?2",
            params![now, id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO memory_versions SELECT m.id,g.revision,m.content,m.category,m.source,g.source_ref,g.scope,g.status,g.reason,?2,g.valid_from,g.valid_until FROM memories m JOIN memory_governance g ON g.memory_id=m.id WHERE m.id=?1",params![id,now]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);
        self.memory_governance(id)
    }
}
