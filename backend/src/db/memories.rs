// ============================================================
// Memory CRUD operations — long-term AI memory persistence
// ============================================================

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::Database;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub content: String,
    pub category: String,   // fact / preference / knowledge / note
    pub source: String,     // auto / manual / document
    pub source_conversation_id: Option<String>,
    pub embedding: Option<String>,  // reserved: JSON array of floats
    pub metadata: Option<String>,   // reserved: extra JSON metadata
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMemoryRequest {
    pub content: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default = "default_source")]
    pub source: String,
    pub source_conversation_id: Option<String>,
    pub metadata: Option<String>,
}

fn default_category() -> String { "fact".to_string() }
fn default_source() -> String { "manual".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateMemoryRequest {
    pub content: Option<String>,
    pub category: Option<String>,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtractRequest {
    pub conversation_id: Option<String>,
    pub max_turns: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryStats {
    pub total: usize,
    pub by_category: Vec<(String, usize)>,
    pub by_source: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryQuery {
    pub category: Option<String>,
    pub source: Option<String>,
    pub q: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl Database {
    pub fn create_memory(&self, req: &CreateMemoryRequest) -> Result<Memory, String> {
        let conn = self.conn();
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO memories (id, content, category, source, source_conversation_id, embedding, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id, req.content, req.category, req.source,
                req.source_conversation_id, Option::<String>::None, req.metadata,
                now, now
            ],
        ).map_err(|e| e.to_string())?;
        Ok(Memory {
            id, content: req.content.clone(), category: req.category.clone(),
            source: req.source.clone(), source_conversation_id: req.source_conversation_id.clone(),
            embedding: None, metadata: req.metadata.clone(), created_at: now, updated_at: now,
        })
    }

    pub fn get_memory(&self, id: &str) -> Result<Memory, String> {
        let conn = self.conn();
        conn.query_row(
            "SELECT id, content, category, source, source_conversation_id, embedding, metadata, created_at, updated_at
             FROM memories WHERE id = ?1",
            params![id],
            |row| {
                Ok(Memory {
                    id: row.get(0)?, content: row.get(1)?, category: row.get(2)?,
                    source: row.get(3)?, source_conversation_id: row.get(4)?,
                    embedding: row.get(5)?, metadata: row.get(6)?,
                    created_at: row.get(7)?, updated_at: row.get(8)?,
                })
            },
        ).map_err(|e| format!("Memory not found: {}", e))
    }

    pub fn list_memories(&self, query: &MemoryQuery) -> Result<Vec<Memory>, String> {
        let conn = self.conn();
        let mut sql = String::from(
            "SELECT id, content, category, source, source_conversation_id, embedding, metadata, created_at, updated_at
             FROM memories WHERE 1=1"
        );
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref cat) = query.category {
            sql.push_str(&format!(" AND category = ?{}", params_vec.len() + 1));
            params_vec.push(Box::new(cat.clone()));
        }
        if let Some(ref src) = query.source {
            sql.push_str(&format!(" AND source = ?{}", params_vec.len() + 1));
            params_vec.push(Box::new(src.clone()));
        }
        if let Some(ref q) = query.q {
            sql.push_str(&format!(" AND content LIKE ?{}", params_vec.len() + 1));
            params_vec.push(Box::new(format!("%{}%", q)));
        }

        sql.push_str(" ORDER BY updated_at DESC");

        let limit = query.limit.unwrap_or(100);
        sql.push_str(&format!(" LIMIT ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(limit as i64));

        if let Some(offset) = query.offset {
            sql.push_str(&format!(" OFFSET ?{}", params_vec.len() + 1));
            params_vec.push(Box::new(offset as i64));
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(Memory {
                id: row.get(0)?, content: row.get(1)?, category: row.get(2)?,
                source: row.get(3)?, source_conversation_id: row.get(4)?,
                embedding: row.get(5)?, metadata: row.get(6)?,
                created_at: row.get(7)?, updated_at: row.get(8)?,
            })
        }).map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn update_memory(&self, id: &str, req: &UpdateMemoryRequest) -> Result<Memory, String> {
        let mut memory = self.get_memory(id)?;
        if let Some(ref content) = req.content { memory.content = content.clone(); }
        if let Some(ref category) = req.category { memory.category = category.clone(); }
        if let Some(ref metadata) = req.metadata { memory.metadata = Some(metadata.clone()); }
        memory.updated_at = chrono::Utc::now().timestamp_millis();

        let conn = self.conn();
        conn.execute(
            "UPDATE memories SET content=?1, category=?2, metadata=?3, updated_at=?4 WHERE id=?5",
            params![memory.content, memory.category, memory.metadata, memory.updated_at, id],
        ).map_err(|e| e.to_string())?;
        Ok(memory)
    }

    pub fn delete_memory(&self, id: &str) -> Result<(), String> {
        let conn = self.conn();
        conn.execute("DELETE FROM memories WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Search memories by keyword (returns up to `limit` most relevant results)
    pub fn search_memories(&self, query: &str, limit: usize) -> Result<Vec<Memory>, String> {
        self.list_memories(&MemoryQuery {
            q: Some(query.to_string()),
            category: None, source: None,
            limit: Some(limit), offset: None,
        })
    }

    /// Get relevant memories for injection into system prompt
    /// Currently keyword-based; reserved for future embedding-based retrieval
    pub fn get_relevant_memories(&self, _context: &str, limit: usize) -> Result<Vec<Memory>, String> {
        // For now, return most recent memories; future: use vector similarity
        self.list_memories(&MemoryQuery {
            q: None, category: None, source: None,
            limit: Some(limit), offset: None,
        })
    }

    /// Get memory statistics
    pub fn get_memory_stats(&self) -> Result<MemoryStats, String> {
        let conn = self.conn();

        let total: usize = conn.query_row(
            "SELECT COUNT(*) FROM memories", [], |row| row.get(0),
        ).map_err(|e| e.to_string())?;

        let mut stmt = conn.prepare(
            "SELECT category, COUNT(*) as cnt FROM memories GROUP BY category ORDER BY cnt DESC"
        ).map_err(|e| e.to_string())?;
        let by_category: Vec<(String, usize)> = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        }).map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;

        let mut stmt2 = conn.prepare(
            "SELECT source, COUNT(*) as cnt FROM memories GROUP BY source ORDER BY cnt DESC"
        ).map_err(|e| e.to_string())?;
        let by_source: Vec<(String, usize)> = stmt2.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        }).map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;

        Ok(MemoryStats { total, by_category, by_source })
    }

    /// Batch delete memories (reserved for future use)
    pub fn delete_memories_by_ids(&self, ids: &[String]) -> Result<usize, String> {
        let conn = self.conn();
        let mut count = 0;
        for id in ids {
            if conn.execute("DELETE FROM memories WHERE id = ?1", params![id]).is_ok() {
                count += 1;
            }
        }
        Ok(count)
    }
}
