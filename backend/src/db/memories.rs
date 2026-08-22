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
    #[serde(skip_serializing)]
    pub embedding: Option<String>, // internal only: JSON array of floats, never serialized
    pub metadata: Option<String>, // reserved: extra JSON metadata
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
    /// Lexical/keyword component of the score.
    #[serde(default)]
    pub lexical_score: f64,
    /// Vector/cosine-similarity component of the score.
    #[serde(default)]
    pub vector_score: f64,
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

// Lexical-only scoring weights (Retrieval v2)
const TEXT_WEIGHT: f64 = 0.75;
const CATEGORY_WEIGHT: f64 = 0.10;
const TIME_WEIGHT: f64 = 0.15;

// Hybrid scoring weights (Retrieval v3)
const HYBRID_TEXT_WEIGHT: f64 = 0.50;
const HYBRID_VECTOR_WEIGHT: f64 = 0.30;
const HYBRID_CATEGORY_WEIGHT: f64 = 0.08;
const HYBRID_TIME_WEIGHT: f64 = 0.12;

/// Minimum vector similarity for a memory to enter the candidate set when
/// it has zero lexical overlap with the query.
const MIN_VECTOR_RELEVANCE: f64 = 0.35;

// ── Embedding serialization ──

/// Serialize a vector of f32s to a compact JSON array string.
pub fn serialize_embedding(vector: &[f32]) -> Result<String, String> {
    if vector.is_empty() {
        return Err("empty embedding vector".to_string());
    }
    if vector.iter().any(|v| !v.is_finite()) {
        return Err("embedding contains non-finite values".to_string());
    }
    serde_json::to_string(vector).map_err(|e| e.to_string())
}

/// Parse a stored embedding string back to a Vec<f32>.
/// Returns `None` for any invalid input (no panic).
pub fn parse_embedding(raw: &str) -> Option<Vec<f32>> {
    let parsed: Vec<f32> = serde_json::from_str(raw).ok()?;
    if parsed.is_empty() {
        return None;
    }
    if parsed.iter().any(|v| !v.is_finite()) {
        return None;
    }
    Some(parsed)
}

// ── Cosine similarity ──

/// Compute cosine similarity between two equal-length vectors.
/// Returns `None` on dimension mismatch, zero vector, or non-finite values.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> Option<f64> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    if a.iter().any(|v| !v.is_finite()) || b.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let (dot, norm_a, norm_b) =
        a.iter()
            .zip(b.iter())
            .fold((0.0_f64, 0.0_f64, 0.0_f64), |(d, na, nb), (x, y)| {
                let x = *x as f64;
                let y = *y as f64;
                (d + x * y, na + x * x, nb + y * y)
            });
    if norm_a == 0.0 || norm_b == 0.0 {
        return None;
    }
    Some(dot / (norm_a.sqrt() * norm_b.sqrt()))
}

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
        let content_changed = req
            .content
            .as_ref()
            .map_or(false, |content| content != &memory.content);
        if content_changed {
            memory.embedding = None;
        }
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
            "UPDATE memories SET content=?1, category=?2, metadata=?3, embedding=?4, updated_at=?5 WHERE id=?6",
            params![
                memory.content,
                memory.category,
                memory.metadata,
                memory.embedding,
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

    /// Update only the `embedding` column for an existing memory.
    /// Does not touch content, category, or updated_at.
    pub fn update_memory_embedding(&self, id: &str, embedding: &[f32]) -> Result<(), String> {
        let json = serialize_embedding(embedding)?;
        let conn = self.conn();
        conn.execute(
            "UPDATE memories SET embedding = ?1 WHERE id = ?2",
            rusqlite::params![json, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// List memories that are still missing an embedding (NULL or empty).
    /// Ordered by `created_at ASC` so older memories are backfilled first.
    pub fn list_memories_without_embedding(&self, limit: usize) -> Result<Vec<Memory>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, content, category, source, source_conversation_id, embedding, metadata, created_at, updated_at
                 FROM memories
                 WHERE embedding IS NULL OR TRIM(embedding) = ''
                 ORDER BY created_at ASC
                 LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([limit as i64], |row| {
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

    /// Count memories still missing an embedding (NULL or empty).
    pub fn count_memories_without_embedding(&self) -> Result<usize, String> {
        let conn = self.conn();
        conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE embedding IS NULL OR TRIM(embedding) = ''",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())
    }

    // ── Ranked Retrieval ──

    /// Lexical-only retrieval (Retrieval v2).
    /// Delegates to `retrieve_memories_hybrid` with `query_embedding = None`.
    pub fn retrieve_memories(&self, query: &RetrieveQuery) -> Result<Vec<ScoredMemory>, String> {
        self.retrieve_memories_hybrid(query, None)
    }

    /// Hybrid retrieval combining lexical, vector (cosine), category, and
    /// time-decay scores.
    ///
    /// When `query_embedding` is `None` the behaviour is identical to
    /// lexical-only Retrieval v2.
    pub fn retrieve_memories_hybrid(
        &self,
        query: &RetrieveQuery,
        query_embedding: Option<&[f32]>,
    ) -> Result<Vec<ScoredMemory>, String> {
        let candidates = self.list_memories(&MemoryQuery {
            category: query.category.clone(),
            source: None,
            q: None,
            limit: Some(RETRIEVAL_CANDIDATE_LIMIT),
            offset: None,
        })?;

        let now = chrono::Utc::now().timestamp_millis();
        let use_hybrid = query_embedding.is_some();
        let q_embed = query_embedding;

        let mut scored: Vec<ScoredMemory> = candidates
            .into_iter()
            .filter_map(|mem| {
                let text_score = compute_text_score(&query.q, &mem.content);

                // Vector score
                let vector_score = if use_hybrid {
                    q_embed
                        .and_then(|qe| {
                            mem.embedding
                                .as_deref()
                                .and_then(parse_embedding)
                                .and_then(|me| cosine_similarity(qe, &me))
                        })
                        .map(|cos| cos.clamp(0.0, 1.0))
                        .unwrap_or(0.0)
                } else {
                    0.0
                };

                // Candidate filtering
                if text_score == 0.0 && vector_score < MIN_VECTOR_RELEVANCE {
                    return None;
                }

                let cat_boost = category_boost(&mem.category);
                let age_days = ((now - mem.updated_at) as f64) / (86_400_000.0);
                let time_score = 1.0 / (1.0 + age_days / 30.0);

                let final_score = if use_hybrid {
                    text_score * HYBRID_TEXT_WEIGHT
                        + vector_score * HYBRID_VECTOR_WEIGHT
                        + cat_boost * HYBRID_CATEGORY_WEIGHT
                        + time_score * HYBRID_TIME_WEIGHT
                } else {
                    text_score * TEXT_WEIGHT
                        + cat_boost * CATEGORY_WEIGHT
                        + time_score * TIME_WEIGHT
                };

                Some(ScoredMemory {
                    memory: mem,
                    score: (final_score * 1000.0).round() / 1000.0,
                    lexical_score: text_score,
                    vector_score,
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

#[cfg(test)]
mod tests {
    use super::{CreateMemoryRequest, UpdateMemoryRequest};
    use crate::db::Database;

    fn test_database() -> (Database, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "yilianqianyan-memory-test-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(&path).expect("temporary memory database should open");
        (db, path)
    }

    fn create_embedded_memory(db: &Database) -> String {
        let memory = db
            .create_memory(&CreateMemoryRequest {
                content: "content A".to_string(),
                category: "fact".to_string(),
                source: "manual".to_string(),
                source_conversation_id: None,
                metadata: Some(r#"{"source":"test"}"#.to_string()),
            })
            .expect("memory should be created");
        db.update_memory_embedding(&memory.id, &[0.1, 0.2, 0.3])
            .expect("embedding should be stored");
        memory.id
    }

    #[test]
    fn content_change_invalidates_embedding_and_marks_memory_for_reindex() {
        let (db, path) = test_database();
        let id = create_embedded_memory(&db);

        let updated = db
            .update_memory(
                &id,
                &UpdateMemoryRequest {
                    content: Some("content B".to_string()),
                    category: None,
                    metadata: None,
                },
            )
            .expect("memory should update");

        assert_eq!(updated.content, "content B");
        assert_eq!(updated.embedding, None);
        assert_eq!(db.get_memory(&id).unwrap().embedding, None);
        assert_eq!(db.list_memories_without_embedding(10).unwrap().len(), 1);
        assert_eq!(db.count_memories_without_embedding().unwrap(), 1);

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn category_only_update_preserves_embedding() {
        let (db, path) = test_database();
        let id = create_embedded_memory(&db);

        db.update_memory(
            &id,
            &UpdateMemoryRequest {
                content: None,
                category: Some("preference".to_string()),
                metadata: None,
            },
        )
        .expect("category should update");

        assert_eq!(
            db.get_memory(&id).unwrap().embedding,
            Some("[0.1,0.2,0.3]".to_string())
        );
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn metadata_only_update_preserves_embedding() {
        let (db, path) = test_database();
        let id = create_embedded_memory(&db);

        db.update_memory(
            &id,
            &UpdateMemoryRequest {
                content: None,
                category: None,
                metadata: Some(r#"{"source":"updated"}"#.to_string()),
            },
        )
        .expect("metadata should update");

        assert_eq!(
            db.get_memory(&id).unwrap().embedding,
            Some("[0.1,0.2,0.3]".to_string())
        );
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn same_content_update_preserves_embedding() {
        let (db, path) = test_database();
        let id = create_embedded_memory(&db);

        db.update_memory(
            &id,
            &UpdateMemoryRequest {
                content: Some("content A".to_string()),
                category: None,
                metadata: None,
            },
        )
        .expect("same content should update");

        assert_eq!(
            db.get_memory(&id).unwrap().embedding,
            Some("[0.1,0.2,0.3]".to_string())
        );
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn content_change_does_not_call_embedding_provider() {
        let (db, path) = test_database();
        let id = create_embedded_memory(&db);

        db.update_memory(
            &id,
            &UpdateMemoryRequest {
                content: Some("content B".to_string()),
                category: None,
                metadata: None,
            },
        )
        .expect("content should update without a provider");

        assert_eq!(db.get_memory(&id).unwrap().embedding, None);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn unchanged_content_is_not_listed_as_missing_embedding() {
        let (db, path) = test_database();
        let id = create_embedded_memory(&db);

        db.update_memory(
            &id,
            &UpdateMemoryRequest {
                content: Some("content A".to_string()),
                category: None,
                metadata: None,
            },
        )
        .expect("same content should update");

        assert!(db.list_memories_without_embedding(10).unwrap().is_empty());
        assert_eq!(db.count_memories_without_embedding().unwrap(), 0);
        std::fs::remove_file(path).ok();
    }
}
