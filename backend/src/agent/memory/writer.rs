// ============================================================
// Memory writer — validate, persist, and embed candidates.
//
// The write pipeline is the only place a candidate becomes a stored memory.
// It validates against the policy + existing memories (including candidates
// stored earlier in the same batch), writes valid candidates through the
// existing Memory store, and generates embeddings for future retrieval. An
// embedding failure never fails the persistence itself — it is tracked
// separately as `embedding_failed`.
// ============================================================

use serde::Serialize;

use super::candidate::MemoryCandidate;
use super::policy::{validate_candidate, MemoryWritePolicy};
use crate::config::types::ModelConfig;
use crate::db::{CreateMemoryRequest, Database, Memory, MemoryQuery};
use crate::llm::client::LlmClient;
use crate::secret::SecretResolver;
use std::sync::Arc;

/// Outcome of attempting to persist a single candidate.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WriteOutcome {
    Stored { memory_id: String },
    Rejected { reason: String },
    Failed { error: String },
}

/// Result of a full write pass.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WriteReport {
    pub stored: usize,
    pub rejected: usize,
    pub failed: usize,
    /// Number of stored memories whose embedding generation failed. The memory
    /// itself is still persisted (best-effort embedding).
    pub embedding_failed: usize,
    pub outcomes: Vec<WriteOutcome>,
}

/// Aggregate report for a full reflection → write learning pass.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MemoryLearningReport {
    pub generated: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub persisted: usize,
    pub embedding_failed: usize,
}

impl MemoryLearningReport {
    pub fn summarize(candidates: usize, report: &WriteReport) -> Self {
        Self {
            generated: candidates,
            accepted: report.stored + report.failed,
            rejected: report.rejected,
            persisted: report.stored,
            embedding_failed: report.embedding_failed,
        }
    }
}

pub struct MemoryWriter {
    db: Database,
    llm: LlmClient,
    policy: MemoryWritePolicy,
    has_embedding: bool,
}

impl MemoryWriter {
    pub fn new(
        db: Database,
        config: &ModelConfig,
        policy: MemoryWritePolicy,
        resolver: Arc<SecretResolver>,
    ) -> Self {
        let has_embedding = config.has_embedding();
        Self {
            db,
            llm: LlmClient::new(config, resolver),
            policy,
            has_embedding,
        }
    }

    /// Validate and persist each candidate, deduplicating against both existing
    /// memories and candidates stored earlier in this same batch.
    pub async fn write(&self, candidates: Vec<MemoryCandidate>) -> WriteReport {
        let mut existing = self
            .db
            .list_memories(&MemoryQuery {
                category: None,
                source: None,
                q: None,
                limit: Some(2000),
                offset: None,
            })
            .unwrap_or_default();

        let mut report = WriteReport::default();
        for candidate in candidates {
            if let Err(reason) = validate_candidate(&candidate, &self.policy, &existing) {
                report.rejected += 1;
                report.outcomes.push(WriteOutcome::Rejected {
                    reason: reason.to_string(),
                });
                continue;
            }

            match self.persist(&candidate).await {
                Ok((memory, embedding_failed)) => {
                    report.stored += 1;
                    if embedding_failed {
                        report.embedding_failed += 1;
                    }
                    report.outcomes.push(WriteOutcome::Stored {
                        memory_id: memory.id.clone(),
                    });
                    // Add to the dedup set so later candidates in this batch
                    // see it.
                    existing.push(memory);
                }
                Err(error) => {
                    report.failed += 1;
                    report.outcomes.push(WriteOutcome::Failed { error });
                }
            }
        }
        report
    }

    /// Write one candidate and (best-effort) embed it. Returns the stored
    /// memory plus whether embedding failed.
    async fn persist(&self, candidate: &MemoryCandidate) -> Result<(Memory, bool), String> {
        let metadata = serde_json::json!({
            "source_task_id": candidate.source_task_id.as_ref().map(|id| id.as_str()),
            "agent_name": candidate.agent_name,
            "confidence": candidate.confidence,
        })
        .to_string();

        let memory = self.db.create_memory(&CreateMemoryRequest {
            content: candidate.content.trim().to_string(),
            category: candidate.category.as_str().to_string(),
            source: "auto".to_string(),
            source_conversation_id: None,
            metadata: Some(metadata),
        })?;

        let mut embedding_failed = false;
        if self.has_embedding {
            match self.llm.embed(&memory.content).await {
                Ok(vector) => {
                    let _ = self.db.update_memory_embedding(&memory.id, &vector);
                }
                Err(_) => {
                    embedding_failed = true;
                }
            }
        }

        Ok((memory, embedding_failed))
    }
}
