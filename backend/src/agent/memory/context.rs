// ============================================================
// Memory context builder — query generation + retrieval + bounded
// context injection for the Agent Runtime.
//
// The builder is deterministic and safety-preserving: it derives a retrieval
// query from the task title / description / step instruction, embeds it when an
// embedding model is configured (falling back to lexical retrieval otherwise),
// retrieves and ranks memories via the existing hybrid retriever, and emits a
// char-safe-truncated context string. Raw embedding vectors are never surfaced.
// ============================================================

use serde::Serialize;

use crate::config::types::ModelConfig;
use crate::db::{Database, RetrieveQuery, ScoredMemory};
use crate::llm::client::LlmClient;
use crate::utils::text::truncate_chars;

/// Maximum characters of a generated memory retrieval query.
pub const MAX_MEMORY_QUERY_CHARS: usize = 500;

/// Maximum characters of the injected memory context block.
pub const MAX_MEMORY_CONTEXT_CHARS: usize = 4000;

/// Number of top-ranked memories to retrieve and inject.
pub const DEFAULT_TOP_K: usize = 5;

/// Which retrieval mode produced the results (for observability / tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRetrievalMode {
    Lexical,
    Hybrid,
    LexicalFallback,
}

/// The outcome of a memory retrieval for one task step.
#[derive(Debug, Clone)]
pub struct MemoryContext {
    pub query: String,
    pub mode: MemoryRetrievalMode,
    pub memories: Vec<ScoredMemory>,
    /// Bounded, UI-safe text to inject into the agent's context.
    pub injected_text: String,
}

/// Build a deterministic retrieval query from task + step fields.
///
/// No LLM is involved: the query is a char-safe concatenation of the fields.
pub fn build_memory_query(title: &str, description: &str, instruction: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let title = title.trim();
    let description = description.trim();
    let instruction = instruction.trim();
    if !title.is_empty() {
        parts.push(title);
    }
    if !description.is_empty() {
        parts.push(description);
    }
    if !instruction.is_empty() {
        parts.push(instruction);
    }
    let joined = parts.join(" ");
    truncate_chars(&joined, MAX_MEMORY_QUERY_CHARS)
}

/// Render ranked memories into a bounded, human-readable context block.
///
/// Empty input (or no relevant memories) yields an empty string.
pub fn build_context_text(memories: &[ScoredMemory]) -> String {
    if memories.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = memories
        .iter()
        .map(|scored| {
            let category = scored.memory.category.as_str();
            let content = scored.memory.content.trim();
            if category.is_empty() {
                format!("- {content}")
            } else {
                format!("- [{category}] {content}")
            }
        })
        .collect();
    truncate_chars(&lines.join("\n"), MAX_MEMORY_CONTEXT_CHARS)
}

/// Builds and injects memory context for an agent execution.
pub struct MemoryContextBuilder {
    db: Database,
    llm: LlmClient,
    has_embedding: bool,
}

impl MemoryContextBuilder {
    pub fn new(db: Database, config: &ModelConfig) -> Self {
        let has_embedding = config.has_embedding();
        Self {
            db,
            llm: LlmClient::new(config),
            has_embedding,
        }
    }

    /// Retrieve and build the memory context for a task step.
    ///
    /// Retrieval failures are returned as `Err` so the caller can decide to
    /// degrade gracefully (e.g. continue without memory) rather than failing
    /// the whole task.
    pub async fn build(
        &self,
        title: &str,
        description: &str,
        instruction: &str,
    ) -> Result<MemoryContext, String> {
        let query = build_memory_query(title, description, instruction);

        let mut query_embedding: Option<Vec<f32>> = None;
        let mut mode = MemoryRetrievalMode::Lexical;

        if self.has_embedding && !query.is_empty() {
            match self.llm.embed(&query).await {
                Ok(vector) => {
                    query_embedding = Some(vector);
                    mode = MemoryRetrievalMode::Hybrid;
                }
                Err(error) => {
                    tracing::warn!(error = %error, "memory query embedding failed; lexical fallback");
                    mode = MemoryRetrievalMode::LexicalFallback;
                }
            }
        }

        let memories = self.db.retrieve_memories_hybrid(
            &RetrieveQuery {
                q: query.clone(),
                top_k: DEFAULT_TOP_K,
                category: None,
            },
            query_embedding.as_deref(),
        )?;

        let injected_text = build_context_text(&memories);
        Ok(MemoryContext {
            query,
            mode,
            memories,
            injected_text,
        })
    }
}
