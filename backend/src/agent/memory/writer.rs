// ============================================================
// Memory writer — validate, persist, and embed candidates.
//
// The write pipeline is the only place a candidate becomes a stored memory.
// It validates against the policy + existing memories, writes valid candidates
// through the existing Memory store, and generates embeddings for future
// retrieval. It never surfaces raw embedding vectors.
// ============================================================

use serde::Serialize;

use super::candidate::MemoryCandidate;
use super::policy::{validate_candidate, MemoryWritePolicy};
use crate::config::types::ModelConfig;
use crate::db::{CreateMemoryRequest, Database, MemoryQuery};
use crate::llm::client::LlmClient;

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
    pub outcomes: Vec<WriteOutcome>,
}

pub struct MemoryWriter {
    db: Database,
    llm: LlmClient,
    policy: MemoryWritePolicy,
    has_embedding: bool,
}

impl MemoryWriter {
    pub fn new(db: Database, config: &ModelConfig, policy: MemoryWritePolicy) -> Self {
        let has_embedding = config.has_embedding();
        Self {
            db,
            llm: LlmClient::new(config),
            policy,
            has_embedding,
        }
    }

    /// Validate and persist each candidate. Embedding generation is best-effort
    /// (a failure there never fails the write of the memory itself).
    pub async fn write(&self, candidates: Vec<MemoryCandidate>) -> WriteReport {
        let existing = self
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
            let outcome = self.write_one(&candidate, &existing).await;
            match &outcome {
                WriteOutcome::Stored { .. } => report.stored += 1,
                WriteOutcome::Rejected { .. } => report.rejected += 1,
                WriteOutcome::Failed { .. } => report.failed += 1,
            }
            report.outcomes.push(outcome);
        }
        report
    }

    async fn write_one(
        &self,
        candidate: &MemoryCandidate,
        existing: &[crate::db::Memory],
    ) -> WriteOutcome {
        if let Err(reason) = validate_candidate(candidate, &self.policy, existing) {
            return WriteOutcome::Rejected {
                reason: reason.to_string(),
            };
        }

        let metadata = serde_json::json!({
            "source_task_id": candidate.source_task_id.as_ref().map(|id| id.as_str()),
            "agent_name": candidate.agent_name,
            "confidence": candidate.confidence,
        })
        .to_string();

        let memory = match self.db.create_memory(&CreateMemoryRequest {
            content: candidate.content.trim().to_string(),
            category: candidate.category.as_str().to_string(),
            source: "auto".to_string(),
            source_conversation_id: None,
            metadata: Some(metadata),
        }) {
            Ok(memory) => memory,
            Err(error) => {
                return WriteOutcome::Failed { error };
            }
        };

        // Best-effort embedding for future hybrid retrieval.
        if self.has_embedding {
            if let Ok(vector) = self.llm.embed(&memory.content).await {
                let _ = self.db.update_memory_embedding(&memory.id, &vector);
            }
        }

        WriteOutcome::Stored {
            memory_id: memory.id,
        }
    }
}
