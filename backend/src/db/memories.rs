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
    pub category: String, // fact / preference / knowledge / note
    pub source: String,   // auto / manual / document
    pub source_conversation_id: Option<String>,
    pub embedding: Option<String>, // reserved: JSON array of floats
    pub metadata: Option<String>,  // reserved: extra JSON metadata
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

fn default_category() -> String {
    "fact".to_string()
}
fn default_source() -> String {
    "manual".to_string()
}

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

// ── Ranked Retrieval types ──

/// A memory with a relevance score (not persisted).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScoredMemory {
    #[serde(flatten)]
    pub memory: Memory,
    pub score: f64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RetrieveQuery {
    /// Free-text search query.
    #[serde(default)]
    pub q: String,
    /// Maximum results to return (default 10, max 50).
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Optional category filter.
    #[serde(default)]
    pub category: Option<String>,
}

fn default_top_k() -> usize {
    10
}

const RETRIEVAL_CANDIDATE_LIMIT: usize = 500;
const MAX_TOP_K: usize = 50;

// Final-score component weights
const TEXT_WEIGHT: f64 = 0.75;
const CATEGORY_WEIGHT: f64 = 0.10;
const TIME_WEIGHT: f64 = 0.15;

/// Lightweight category boost multipliers.
fn category_boost(category: &str) -> f64 {
    match category {
        "preference" => 1.10,
        "fact" => 1.05,
        "knowledge" => 1.00,
        "note" => 0.95,
        _ => 1.00,
    }
}

/// Compute a keyword-overlap relevance score for `content` against `query`.
///
/// Strategy (no external segmenter needed for Chinese):
/// 1. Exact / full-substring match (case-insensitive) → 0.9
/// 2. Character-level overlap (for CJK text)          → up to 0.7
/// 3. Word-token overlap (for ASCII / Latin text)     → up to 0.6
///
/// Returns `0.0` when there is no match at all.
fn compute_text_score(query: &str, content: &str) -> f64 {
    let q = query.trim().to_lowercase();
    let c = content.trim().to_lowercase();
    if q.is_empty() || c.is_empty() {
        return 0.0;
    }

    // 1 — full substring match
    if c.contains(&q) {
        return 0.9;
    }

    // 2 — character-level overlap (handles Chinese, emoji, mixed scripts)
    let q_chars: Vec<char> = q.chars().collect();
    let relevant_q_chars: Vec<char> = q_chars
        .iter()
        .filter(|c| !c.is_ascii_whitespace() && !c.is_ascii_punctuation())
        .copied()
        .collect();
    if relevant_q_chars.is_empty() {
        return 0.0; // query had only whitespace/punctuation
    }
    let mut matched = 0usize;
    for qc in &relevant_q_chars {
        if c.contains(*qc) {
            matched += 1;
        }
    }
    let char_overlap = matched as f64 / relevant_q_chars.len() as f64;

    // 3 — ASCII word-token overlap
    let token_overlap = if q.chars().any(|ch| ch.is_ascii_alphabetic()) {
        let q_tokens: Vec<&str> = q
            .split(|ch: char| ch.is_ascii_whitespace() || ch.is_ascii_punctuation())
            .filter(|t| t.len() >= 2)
            .collect();
        if q_tokens.is_empty() {
            0.0
        } else {
            let matched_tokens = q_tokens.iter().filter(|t| c.contains(*t)).count();
            matched_tokens as f64 / q_tokens.len() as f64
        }
    } else {
        0.0
    };

    // Combine: favour the higher of char-overlap and token-overlap,
    // with char-overlap slightly higher priority for CJK text.
    let best = if char_overlap >= token_overlap {
        char_overlap * 0.7
    } else {
        token_overlap * 0.6
    };

    best.clamp(0.0, 0.85)
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
            id,
            content: req.content.clone(),
            category: req.category.clone(),
            source: req.source.clone(),
            source_conversation_id: req.source_conversation_id.clone(),
            embedding: None,
            metadata: req.metadata.clone(),
            created_at: now,
            updated_at: now,
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

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok(Memory {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    category: row.get(2)?,
                    source: row.get(3)?,
                    source_conversation_id: row.get(4)?,
                    embedding: row.get(5)?,
                    metadata: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn update_memory(&self, id: &str, req: &UpdateMemoryRequest) -> Result<Memory, String> {
        let mut memory = self.get_memory(id)?;
        if let Some(ref content) = req.content {
            memory.content = content.clone();
        }
        if let Some(ref category) = req.category {
            memory.category = category.clone();
        }
        if let Some(ref metadata) = req.metadata {
            memory.metadata = Some(metadata.clone());
        }
        memory.updated_at = chrono::Utc::now().timestamp_millis();

        let conn = self.conn();
        conn.execute(
            "UPDATE memories SET content=?1, category=?2, metadata=?3, updated_at=?4 WHERE id=?5",
            params![
                memory.content,
                memory.category,
                memory.metadata,
                memory.updated_at,
                id
            ],
        )
        .map_err(|e| e.to_string())?;
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
            category: None,
            source: None,
            limit: Some(limit),
            offset: None,
        })
    }

    /// Get relevant memories for injection into system prompt
    /// Currently keyword-based; reserved for future embedding-based retrieval
    pub fn get_relevant_memories(
        &self,
        _context: &str,
        limit: usize,
    ) -> Result<Vec<Memory>, String> {
        // For now, return most recent memories; future: use vector similarity
        self.list_memories(&MemoryQuery {
            q: None,
            category: None,
            source: None,
            limit: Some(limit),
            offset: None,
        })
    }

    /// Get memory statistics
    pub fn get_memory_stats(&self) -> Result<MemoryStats, String> {
        let conn = self.conn();

        let total: usize = conn
            .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;

        let mut stmt = conn.prepare(
            "SELECT category, COUNT(*) as cnt FROM memories GROUP BY category ORDER BY cnt DESC"
        ).map_err(|e| e.to_string())?;
        let by_category: Vec<(String, usize)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        let mut stmt2 = conn
            .prepare(
                "SELECT source, COUNT(*) as cnt FROM memories GROUP BY source ORDER BY cnt DESC",
            )
            .map_err(|e| e.to_string())?;
        let by_source: Vec<(String, usize)> = stmt2
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(MemoryStats {
            total,
            by_category,
            by_source,
        })
    }

    // ── Ranked Retrieval ──

    /// Retrieve memories ranked by relevance to a query.
    ///
    /// Scoring is deterministic and local: keyword overlap, category boost,
    /// and time decay — no embedding or LLM rerank.
    pub fn retrieve_memories(&self, query: &RetrieveQuery) -> Result<Vec<ScoredMemory>, String> {
        let candidates = self.list_memories(&MemoryQuery {
            category: query.category.clone(),
            source: None,
            q: None,
            limit: Some(RETRIEVAL_CANDIDATE_LIMIT),
            offset: None,
        })?;

        let now = chrono::Utc::now().timestamp_millis();
        let mut scored: Vec<ScoredMemory> = candidates
            .into_iter()
            .filter_map(|mem| {
                let text_score = compute_text_score(&query.q, &mem.content);
                if text_score == 0.0 {
                    return None; // no relevance → exclude
                }
                let cat_boost = category_boost(&mem.category);
                let age_days = ((now - mem.updated_at) as f64) / (86_400_000.0);
                let time_score = 1.0 / (1.0 + age_days / 30.0);
                let final_score = text_score * TEXT_WEIGHT
                    + cat_boost * CATEGORY_WEIGHT
                    + time_score * TIME_WEIGHT;
                Some(ScoredMemory {
                    memory: mem,
                    score: (final_score * 1000.0).round() / 1000.0,
                })
            })
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.memory.updated_at.cmp(&a.memory.updated_at))
        });

        let top_k = query.top_k.clamp(1, MAX_TOP_K);
        scored.truncate(top_k);
        Ok(scored)
    }

    /// Batch delete memories (reserved for future use)
    pub fn delete_memories_by_ids(&self, ids: &[String]) -> Result<usize, String> {
        let conn = self.conn();
        let mut count = 0;
        for id in ids {
            if conn
                .execute("DELETE FROM memories WHERE id = ?1", params![id])
                .is_ok()
            {
                count += 1;
            }
        }
        Ok(count)
    }
}
