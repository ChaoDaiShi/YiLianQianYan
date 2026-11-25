use super::{content_hash, MemoryEmbeddingTicket};
use crate::db::{memories, Database, Memory};
use rusqlite::{params, OptionalExtension};

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
        let json = memories::serialize_embedding(embedding)?;
        if embedding.iter().all(|&value| value == 0.0) {
            return Err("zero_memory_embedding".into());
        }
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
