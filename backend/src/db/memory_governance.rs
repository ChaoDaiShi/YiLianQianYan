//! Profile-local authoritative memory revisions and embedding compare-and-set.
use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database, Memory,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

impl Database {
    pub fn memory_embedding_generation(&self, model: &str) -> Result<Option<i64>, String> {
        self.conn()
            .query_row(
                "SELECT generation FROM memory_embedding_config WHERE id=1 AND model=?1",
                [model],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }
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
        self.conn().query_row("SELECT EXISTS(SELECT 1 FROM memory_governance WHERE memory_id=?1 AND status='active' AND (scope='profile' OR scope=?2 OR scope=?3))",params![id,scope,workspace],|r|r.get(0)).map_err(|e|e.to_string())
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
            let mut stmt=conn.prepare("SELECT m.id FROM memories m JOIN memory_governance g ON g.memory_id=m.id WHERE g.status='active' AND (m.embedding IS NULL OR TRIM(m.embedding)='' OR g.embedding_revision IS NOT g.revision OR g.embedding_model IS NOT ?1 OR g.embedding_generation IS NOT (SELECT generation FROM memory_embedding_config WHERE id=1 AND model=?1)) ORDER BY m.created_at ASC LIMIT ?2").map_err(|e|e.to_string())?;
            let rows = stmt
                .query_map(params![model, limit as i64], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
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

    pub fn memory_history(&self, id: &str) -> Result<Vec<MemoryVersion>, String> {
        let conn = self.conn();
        let mut stmt=conn.prepare("SELECT revision,content,category,source,source_ref,scope,status,reason,recorded_at,valid_from,valid_until FROM memory_versions WHERE memory_id=?1 ORDER BY revision DESC").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map([id], |r| {
                Ok(MemoryVersion {
                    revision: r.get(0)?,
                    content: r.get(1)?,
                    category: r.get(2)?,
                    source: r.get(3)?,
                    source_ref: r.get(4)?,
                    scope: r.get(5)?,
                    status: r.get(6)?,
                    reason: r.get(7)?,
                    recorded_at: r.get(8)?,
                    valid_from: r.get(9)?,
                    valid_until: r.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn correct_memory(&self, id: &str, req: &CorrectMemoryRequest) -> Result<Memory, String> {
        let content = req.content.trim();
        if content.is_empty() || content.chars().count() > 2000 || req.reason.trim().is_empty() {
            return Err("invalid_memory_correction".into());
        }
        if crate::safety::contains_sensitive_content(content) {
            return Err("sensitive_memory_content".into());
        }
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
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
        let changed=tx.execute("UPDATE memory_governance SET reason=?1,scope=COALESCE(?2,scope),source_ref=COALESCE(?3,source_ref) WHERE memory_id=?4 AND revision=?5 AND status='active'",params![req.reason,req.scope,req.source_ref,id,req.expected_revision]).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("memory_revision_conflict".into());
        }
        let now = chrono::Utc::now().timestamp_millis();
        // Explicit corrections always produce a revision, even when only provenance/scope changes.
        tx.execute(
            "UPDATE memories SET content=?1,updated_at=?2 WHERE id=?3",
            params![content, now, id],
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
        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);
        self.get_memory(id)
    }

    pub fn withdraw_memory(
        &self,
        id: &str,
        req: &WithdrawMemoryRequest,
    ) -> Result<MemoryGovernance, String> {
        if req.reason.trim().is_empty() {
            return Err("invalid_memory_reason".into());
        }
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
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

    pub fn begin_memory_embedding(
        &self,
        memory: &Memory,
        model: &str,
    ) -> Result<MemoryEmbeddingTicket, String> {
        if model.trim().is_empty() {
            return Err("embedding_model_required".into());
        }
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let row:Option<(i64,String)>=tx.query_row("SELECT g.revision,m.content FROM memory_governance g JOIN memories m ON m.id=g.memory_id WHERE m.id=?1 AND g.status='active'",[&memory.id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
        let (revision, content) = row.ok_or("memory_revision_conflict")?;
        if content != memory.content {
            return Err("memory_revision_conflict".into());
        }
        tx.execute("UPDATE memory_embedding_config SET model=?1,generation=generation+1 WHERE id=1 AND model!=?1",[model]).map_err(|e|e.to_string())?;
        let generation = tx
            .query_row(
                "SELECT generation FROM memory_embedding_config WHERE id=1",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(MemoryEmbeddingTicket {
            memory_id: memory.id.clone(),
            revision,
            content_hash: content_hash(&content),
            model: model.into(),
            generation,
        })
    }

    pub fn complete_memory_embedding(
        &self,
        ticket: &MemoryEmbeddingTicket,
        embedding: &[f32],
    ) -> Result<(), String> {
        let json = super::memories::serialize_embedding(embedding)?;
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let content: Option<String> = tx
            .query_row(
                "SELECT content FROM memories WHERE id=?1",
                [&ticket.memory_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if content.as_deref().map(content_hash).as_deref() != Some(ticket.content_hash.as_str()) {
            return Err("stale_embedding_result".into());
        }
        let changed=tx.execute("UPDATE memory_governance SET embedding_revision=?1,embedding_hash=?2,embedding_model=?3,embedding_dimension=?4,embedding_generation=?5 WHERE memory_id=?6 AND revision=?1 AND status='active' AND EXISTS(SELECT 1 FROM memory_embedding_config WHERE id=1 AND model=?3 AND generation=?5)",params![ticket.revision,ticket.content_hash,ticket.model,embedding.len() as i64,ticket.generation,ticket.memory_id]).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("stale_embedding_result".into());
        }
        let changed = tx
            .execute(
                "UPDATE memories SET embedding=?1 WHERE id=?2",
                params![json, ticket.memory_id],
            )
            .map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("stale_embedding_result".into());
        }
        tx.commit().map_err(|e| e.to_string())
    }
}
