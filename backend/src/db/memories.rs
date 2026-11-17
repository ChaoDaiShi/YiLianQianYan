// ============================================================
// Memory CRUD operations — long-term AI memory persistence
// ============================================================

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Database;

pub(super) mod lexical;

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

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MemoryScope {
    #[default]
    Profile,
    Workspace,
    Conversation,
}

impl MemoryScope {
    fn from_storage(scope: &str) -> Self {
        if scope.starts_with("conversation:") {
            Self::Conversation
        } else if scope.starts_with("workspace:") {
            Self::Workspace
        } else {
            Self::Profile
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Profile => "当前档案",
            Self::Workspace => "本工作空间",
            Self::Conversation => "本对话",
        }
    }
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
    #[serde(skip)]
    pub(crate) revision: i64,
    #[serde(skip)]
    pub(crate) scope: MemoryScope,
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

const MAX_TOP_K: usize = 50;

#[derive(Default)]
pub(crate) struct MemoryRecallScope<'a> {
    pub conversation_id: Option<&'a str>,
    pub workspace_id: Option<&'a str>,
    pub exclude_documents: bool,
    pub exclude_knowledge: bool,
    pub include_conversation_pins: bool,
}

fn read_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
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
}

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
        self.create_memory_with_scope(req, MemoryScope::Profile)
    }

    pub(crate) fn create_memory_with_scope(
        &self,
        req: &CreateMemoryRequest,
        scope: MemoryScope,
    ) -> Result<Memory, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        let memory = Self::create_memory_on(&tx, req, scope, None)?;
        tx.commit().map_err(|error| error.to_string())?;
        Ok(memory)
    }

    pub(super) fn create_memory_on(
        tx: &rusqlite::Transaction<'_>,
        req: &CreateMemoryRequest,
        scope: MemoryScope,
        source_workspace: Option<&str>,
    ) -> Result<Memory, String> {
        if req.content.trim().is_empty() || req.content.chars().count() > 2000 {
            return Err("invalid_memory_content".into());
        }
        if !["fact", "preference", "knowledge", "note"].contains(&req.category.as_str()) {
            return Err("invalid_memory_category".into());
        }
        if !["auto", "manual", "document"].contains(&req.source.as_str()) {
            return Err("invalid_memory_source".into());
        }
        if req.source != "document" && crate::safety::contains_sensitive_content(&req.content) {
            return Err("sensitive_memory_content".into());
        }
        let stored_scope = match scope {
            MemoryScope::Profile => "profile".into(),
            MemoryScope::Conversation => format!(
                "conversation:{}",
                req.source_conversation_id
                    .as_deref()
                    .ok_or("memory_conversation_required")?
            ),
            MemoryScope::Workspace => {
                let workspace: String = tx
                    .query_row(
                        "SELECT id FROM workspaces WHERE status='active' AND id=COALESCE(?1,
                           (SELECT value FROM settings WHERE key='conversation_workspace:'||?2))",
                        params![source_workspace, req.source_conversation_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?
                    .ok_or("memory_workspace_required")?;
                format!("workspace:{workspace}")
            }
        };
        if req.source != "document" {
            let normalized = super::memory_governance::normalize_memory(&req.content);
            let relation =
                super::memory_candidates::relationship::read_metadata(req.metadata.as_deref());
            let mut stmt=tx.prepare("SELECT m.id,m.content,m.category,m.source,m.source_conversation_id,m.embedding,m.metadata,m.created_at,m.updated_at FROM memories m JOIN memory_governance g ON g.memory_id=m.id WHERE m.category=?1 AND g.status='active' AND g.scope=?2 AND m.source!='document' AND NOT EXISTS(SELECT 1 FROM knowledge_document_chunks c WHERE c.memory_id=m.id)").map_err(|e|e.to_string())?;
            let existing = stmt
                .query_map(params![req.category, stored_scope], read_memory)
                .map_err(|e| e.to_string())?;
            for memory in existing {
                let memory = memory.map_err(|e| e.to_string())?;
                if super::memory_candidates::relationship::same_memory(
                    relation.as_ref(),
                    super::memory_governance::normalize_memory(&memory.content) == normalized,
                    memory.metadata.as_deref(),
                ) {
                    return Ok(memory);
                }
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        tx.execute(
            "INSERT INTO memories (id, content, category, source, source_conversation_id, embedding, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id, req.content, req.category, req.source,
                req.source_conversation_id, Option::<String>::None, req.metadata,
                now, now
            ],
        ).map_err(|e| e.to_string())?;
        if scope != MemoryScope::Profile {
            tx.execute(
                "UPDATE memory_governance SET scope=?1 WHERE memory_id=?2",
                params![stored_scope, id],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "UPDATE memory_versions SET scope=?1 WHERE memory_id=?2 AND revision=1",
                params![stored_scope, id],
            )
            .map_err(|error| error.to_string())?;
        }
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

    pub fn find_memory(&self, id: &str) -> Result<Option<Memory>, String> {
        use rusqlite::OptionalExtension;
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
        ).optional().map_err(|e| e.to_string())
    }

    pub fn get_memory(&self, id: &str) -> Result<Memory, String> {
        match self.find_memory(id)? {
            Some(mem) => Ok(mem),
            None => Err(format!("Memory not found: {}", id)),
        }
    }

    pub fn list_memories(&self, query: &MemoryQuery) -> Result<Vec<Memory>, String> {
        self.list_memory_rows(query, false)
    }

    pub fn list_memory_library(&self, query: &MemoryQuery) -> Result<Vec<Memory>, String> {
        self.list_memory_rows(query, true)
    }

    fn list_memory_rows(&self, query: &MemoryQuery, library: bool) -> Result<Vec<Memory>, String> {
        let conn = self.conn();
        Self::list_memory_rows_on(&conn, query, library, None)
    }

    pub(super) fn list_memory_rows_on(
        conn: &rusqlite::Connection,
        query: &MemoryQuery,
        library: bool,
        graph_status: Option<&str>,
    ) -> Result<Vec<Memory>, String> {
        let mut sql = String::from(
            "SELECT id, content, category, source, source_conversation_id, embedding, metadata, created_at, updated_at
             FROM memories WHERE EXISTS(SELECT 1 FROM memory_governance g WHERE g.memory_id=memories.id"
        );
        match graph_status {
            Some("withdrawn") => sql.push_str(" AND g.status='withdrawn')"),
            Some(_) => sql.push_str(" AND g.status='active' AND (g.valid_from IS NULL OR g.valid_from<=CAST((julianday('now')-2440587.5)*86400000 AS INTEGER)) AND (g.valid_until IS NULL OR g.valid_until>CAST((julianday('now')-2440587.5)*86400000 AS INTEGER)))"),
            None => sql.push_str(" AND g.status='active')"),
        }
        if library {
            sql.push_str(" AND NOT EXISTS(SELECT 1 FROM knowledge_document_chunks k WHERE k.memory_id=memories.id)");
        }
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
            .query_map(param_refs.as_slice(), read_memory)
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn update_memory(&self, id: &str, req: &UpdateMemoryRequest) -> Result<Memory, String> {
        self.update_memory_at_revision(id, req, None)
    }

    pub(crate) fn update_memory_at_revision(
        &self,
        id: &str,
        req: &UpdateMemoryRequest,
        expected_revision: Option<i64>,
    ) -> Result<Memory, String> {
        if self.knowledge_chunk_document(id)?.is_some() {
            return Err("knowledge_document_owned_chunk".into());
        }
        if let Some(content) = &req.content {
            if content.trim().is_empty() || content.chars().count() > 2000 {
                return Err("invalid_memory_content".into());
            }
            if crate::safety::contains_sensitive_content(content) {
                return Err("sensitive_memory_content".into());
            }
        }
        if let Some(category) = &req.category {
            if !["fact", "preference", "knowledge", "note"].contains(&category.as_str()) {
                return Err("invalid_memory_category".into());
            }
        }
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let previous = tx
            .query_row(
                "SELECT m.content,m.category,m.metadata FROM memories m
             JOIN memory_governance g ON g.memory_id=m.id WHERE m.id=?1 AND g.status='active'
             AND (?2 IS NULL OR (g.revision=?2 AND (g.valid_from IS NULL OR g.valid_from<=?3)
                                    AND (g.valid_until IS NULL OR g.valid_until>?3)))",
                params![id, expected_revision, chrono::Utc::now().timestamp_millis()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or("memory_revision_conflict")?;
        let content = req
            .content
            .as_deref()
            .filter(|value| value.trim() != previous.0.trim());
        let category = req.category.as_deref().filter(|value| *value != previous.1);
        let metadata = req.metadata.as_deref().filter(|value| {
            if previous.2.as_deref() == Some(*value) {
                return false;
            }
            !previous.2.as_deref().is_some_and(|previous| {
                match (
                    serde_json::from_str::<serde_json::Value>(previous),
                    serde_json::from_str::<serde_json::Value>(value),
                ) {
                    (Ok(previous), Ok(next)) => previous == next,
                    _ => false,
                }
            })
        });
        if content.is_none() && category.is_none() && metadata.is_none() {
            drop(tx);
            drop(conn);
            return self.get_memory(id);
        }
        tx.execute(
            "UPDATE memory_governance SET reason='edited through memory API' WHERE memory_id=?1",
            [id],
        )
        .map_err(|e| e.to_string())?;
        let changed=tx.execute("UPDATE memories SET content=COALESCE(?1,content),category=COALESCE(?2,category),metadata=COALESCE(?3,metadata),updated_at=?4 WHERE id=?5 AND EXISTS(SELECT 1 FROM memory_governance WHERE memory_id=?5 AND status='active')",
            params![content,category,metadata,chrono::Utc::now().timestamp_millis(),id]).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("memory_revision_conflict".into());
        }
        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);
        self.get_memory(id)
    }

    pub fn delete_memory(&self, id: &str) -> Result<(), String> {
        self.delete_memories_by_ids(&[id.to_owned()]).map(|_| ())
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
            .query_row("SELECT COUNT(*) FROM memories WHERE EXISTS(SELECT 1 FROM memory_governance g WHERE g.memory_id=memories.id AND g.status='active')", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;

        let mut stmt = conn.prepare(
            "SELECT category, COUNT(*) as cnt FROM memories WHERE EXISTS(SELECT 1 FROM memory_governance g WHERE g.memory_id=memories.id AND g.status='active') GROUP BY category ORDER BY cnt DESC"
        ).map_err(|e| e.to_string())?;
        let by_category: Vec<(String, usize)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        let mut stmt2 = conn
            .prepare(
                "SELECT source, COUNT(*) as cnt FROM memories WHERE EXISTS(SELECT 1 FROM memory_governance g WHERE g.memory_id=memories.id AND g.status='active') GROUP BY source ORDER BY cnt DESC",
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
                 WHERE (embedding IS NULL OR TRIM(embedding) = '') AND EXISTS(SELECT 1 FROM memory_governance g WHERE g.memory_id=memories.id AND g.status='active')
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
            "SELECT COUNT(*) FROM memories WHERE (embedding IS NULL OR TRIM(embedding) = '') AND EXISTS(SELECT 1 FROM memory_governance g WHERE g.memory_id=memories.id AND g.status='active')",
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
        self.retrieve_memory_candidates(query, query_embedding, None, MemoryRecallScope::default())
    }

    pub fn retrieve_chat_memories(
        &self,
        query: &RetrieveQuery,
        query_embedding: Option<&[f32]>,
    ) -> Result<Vec<ScoredMemory>, String> {
        self.retrieve_memory_candidates(
            query,
            query_embedding,
            None,
            MemoryRecallScope {
                exclude_documents: true,
                ..Default::default()
            },
        )
    }

    /// Scope and embedding identity are enforced before relevance scoring.
    pub fn retrieve_scoped_memories(
        &self,
        query: &RetrieveQuery,
        query_embedding: Option<&[f32]>,
        embedding_model: Option<&str>,
        conversation_id: Option<&str>,
        exclude_documents: bool,
    ) -> Result<Vec<ScoredMemory>, String> {
        self.retrieve_memory_candidates(
            query,
            query_embedding,
            embedding_model,
            MemoryRecallScope {
                conversation_id,
                exclude_documents,
                ..Default::default()
            },
        )
    }

    pub fn retrieve_workspace_memories(
        &self,
        query: &RetrieveQuery,
        query_embedding: Option<&[f32]>,
        embedding_model: Option<&str>,
        workspace_id: &str,
    ) -> Result<Vec<ScoredMemory>, String> {
        self.retrieve_memory_candidates(
            query,
            query_embedding,
            embedding_model,
            MemoryRecallScope {
                workspace_id: Some(workspace_id),
                exclude_documents: true,
                ..Default::default()
            },
        )
    }

    pub(crate) fn retrieve_memory_candidates(
        &self,
        query: &RetrieveQuery,
        query_embedding: Option<&[f32]>,
        embedding_model: Option<&str>,
        scope: MemoryRecallScope<'_>,
    ) -> Result<Vec<ScoredMemory>, String> {
        let lexical_query = lexical::LexicalQuery::new(&query.q);
        if lexical_query.is_empty() && query_embedding.is_none() && !scope.include_conversation_pins
        {
            return Ok(Vec::new());
        }
        let workspace = scope
            .conversation_id
            .map(|id| self.conversation_workspace(id))
            .transpose()?
            .flatten();
        let conversation_scope = scope.conversation_id.map(|id| format!("conversation:{id}"));
        let workspace_scope = workspace
            .as_deref()
            .or(scope.workspace_id)
            .map(|id| format!("workspace:{id}"));
        // Read content and its current governance metadata in the same snapshot.
        let permitted = {
            let conn = self.conn();
            let mut statement = conn.prepare(
                "SELECT m.id,m.content,m.category,m.source,m.source_conversation_id,
                    CASE WHEN ?6 IS NULL OR (g.embedding_revision=g.revision
                        AND g.embedding_model=?6 AND g.embedding_generation IS cfg.generation)
                        THEN m.embedding END,
                    m.metadata,m.created_at,m.updated_at,g.embedding_hash,g.revision,g.scope
                 FROM memories m JOIN memory_governance g ON g.memory_id=m.id
                 LEFT JOIN memory_embedding_config cfg ON cfg.id=1 AND cfg.model=?6
                 WHERE g.status='active' AND (g.scope='profile' OR g.scope=?2 OR g.scope=?3)
                    AND (g.valid_from IS NULL OR g.valid_from<=?7)
                    AND (g.valid_until IS NULL OR g.valid_until>?7)
                    AND (?1 IS NULL OR m.category=?1)
                    AND (NOT ?4 OR NOT EXISTS(SELECT 1 FROM knowledge_document_chunks c WHERE c.memory_id=m.id))
                    AND (NOT ?5 OR m.category!='knowledge')",
            ).map_err(|error| error.to_string())?;
            let rows = statement
                .query_map(
                    params![
                        query.category,
                        conversation_scope,
                        workspace_scope,
                        scope.exclude_documents,
                        scope.exclude_knowledge,
                        embedding_model,
                        chrono::Utc::now().timestamp_millis(),
                    ],
                    |row| {
                        let mut memory = read_memory(row)?;
                        if embedding_model.is_some() && memory.embedding.is_some() {
                            let hash: Option<String> = row.get(9)?;
                            if hash.as_deref()
                                != Some(
                                    super::memory_governance::content_hash(&memory.content)
                                        .as_str(),
                                )
                            {
                                memory.embedding = None;
                            }
                        }
                        let scope = MemoryScope::from_storage(&row.get::<_, String>(11)?);
                        Ok((memory, row.get::<_, i64>(10)?, scope))
                    },
                )
                .map_err(|error| error.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?
        };
        let now = chrono::Utc::now().timestamp_millis();
        let use_hybrid = query_embedding.is_some();
        let q_embed = query_embedding;

        let mut scored: Vec<ScoredMemory> = permitted
            .into_iter()
            .filter_map(|(mem, revision, memory_scope)| {
                let text_score = lexical_query.score(&mem.content);

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
                let relevant = text_score > 0.0 || vector_score >= MIN_VECTOR_RELEVANCE;
                let pinned =
                    scope.include_conversation_pins && memory_scope == MemoryScope::Conversation;
                if !relevant && !pinned {
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
                    score: if relevant {
                        (final_score * 1000.0).round() / 1000.0
                    } else {
                        0.0
                    },
                    lexical_score: text_score,
                    vector_score,
                    revision,
                    scope: memory_scope,
                })
            })
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.scope.cmp(&a.scope))
                .then_with(|| b.memory.updated_at.cmp(&a.memory.updated_at))
        });

        let top_k = query.top_k.clamp(1, MAX_TOP_K);
        scored.truncate(top_k);
        Ok(scored)
    }

    /// Delete the requested memories through the same purge transaction.
    pub fn delete_memories_by_ids(&self, ids: &[String]) -> Result<usize, String> {
        let identity = self
            .get_or_create_cloud_identity()
            .map_err(|e| e.to_string())?;
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let mut existing = Vec::new();
        {
            let mut query = tx
                .prepare("SELECT 1 FROM memories WHERE id=?1")
                .map_err(|e| e.to_string())?;
            for id in ids {
                if query
                    .query_row([id], |_| Ok(()))
                    .optional()
                    .map_err(|e| e.to_string())?
                    .is_some()
                {
                    existing.push(id.as_str());
                }
            }
        }
        self.record_memory_purge(super::memory_purge_journal::PurgeKind::Memory, &existing)
            .map_err(|e| e.to_string())?;
        let mut count = 0;
        for id in ids {
            count += delete_memory_on(&tx, id, &identity.device_id).map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(count)
    }
}

pub(super) fn delete_memory_on(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    device: &str,
) -> rusqlite::Result<usize> {
    let removed = tx.execute("DELETE FROM memories WHERE id=?1", [id])?;
    if removed > 0 {
        super::memory_activity::record_memory_removal(tx, chrono::Utc::now().timestamp_millis())?;
        super::cloud::capture::record_local_deletion_tx(tx, "memory", id, device)?;
    }
    super::connectors::remove_connector_entity_on(tx, "memory", id)?;
    super::connectors::remove_connector_entity_on(tx, "knowledge", id)?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::{CreateMemoryRequest, RetrieveQuery, UpdateMemoryRequest};
    use crate::db::Database;

    #[test]
    fn recall_uses_words_and_phrases_instead_of_scattered_characters() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        for content in [
            "用户偏好导出 CSV 使用 UTF-8",
            "出口审批流程",
            "rustic 风格",
            "Rust 安全执行",
        ] {
            db.create_memory(&CreateMemoryRequest {
                content: content.into(),
                category: "note".into(),
                source: "manual".into(),
                source_conversation_id: None,
                metadata: None,
            })
            .unwrap();
        }
        for (q, expected) in [
            ("导出 CSV 的配置", "用户偏好导出 CSV 使用 UTF-8"),
            ("Rust", "Rust 安全执行"),
        ] {
            let results = db
                .retrieve_memories(&RetrieveQuery {
                    q: q.into(),
                    top_k: 10,
                    category: None,
                })
                .unwrap();
            assert_eq!(
                results
                    .iter()
                    .map(|item| item.memory.content.as_str())
                    .collect::<Vec<_>>(),
                vec![expected],
                "{q}"
            );
        }
    }

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

pub(super) fn keyword_score(query: &str, content: &str) -> f64 {
    compute_text_score(query, content)
}
