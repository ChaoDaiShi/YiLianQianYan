// ============================================================
// Memory reflection — turn a completed/failed task into bounded,
// deterministic memory candidates.
//
// Reflection is WHAT-to-remember, not a write. It is deliberately
// deterministic: it never calls an LLM, never calls a tool, and never re-reads
// previously-injected memory text. It only derives candidates from the current
// task execution's *new* data (agent result summaries, artifact summaries, and
// the task outcome). User preferences are NOT inferred here — those remain the
// responsibility of Chat Memory Extraction.
// ============================================================

use async_trait::async_trait;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::candidate::{MemoryCandidate, MemoryCategory};
use crate::task::model::{TaskId, TaskStatus};
use crate::utils::text::truncate_chars;

/// Hard cap on the number of candidates produced per task execution.
pub const MAX_MEMORY_CANDIDATES_PER_EXECUTION: usize = 5;

/// Maximum characters of a single derived candidate content.
const MAX_CANDIDATE_CONTENT_CHARS: usize = 1500;

/// Inputs to reflection. Contains no secrets and no injected-memory text.
#[derive(Debug, Clone)]
pub struct ReflectionInput {
    pub task_id: TaskId,
    pub task_title: String,
    pub task_description: String,
    pub task_status: TaskStatus,
    pub agent_name: String,
    /// Bounded result summaries of the task's agent executions.
    pub agent_result_summaries: Vec<String>,
    /// Bounded artifact summaries produced by this task (name/type/summary only).
    pub artifact_summaries: Vec<String>,
    /// Sanitized failure reason (only meaningful for `Failed` tasks).
    pub error_summary: Option<String>,
}

#[derive(Debug, Error)]
pub enum ReflectorError {
    #[error("reflection produced no usable candidates: {0}")]
    InvalidOutput(String),
}

#[async_trait]
pub trait MemoryReflector: Send + Sync {
    async fn reflect(
        &self,
        input: ReflectionInput,
        cancel: &CancellationToken,
    ) -> Result<Vec<MemoryCandidate>, ReflectorError>;
}

/// Deterministic, LLM-free reflector. Never performs a network call.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeterministicMemoryReflector;

impl DeterministicMemoryReflector {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl MemoryReflector for DeterministicMemoryReflector {
    async fn reflect(
        &self,
        input: ReflectionInput,
        cancel: &CancellationToken,
    ) -> Result<Vec<MemoryCandidate>, ReflectorError> {
        if cancel.is_cancelled() {
            return Ok(Vec::new());
        }
        Ok(deterministic_reflect(&input))
    }
}

/// Pure, deterministic candidate derivation (no I/O).
fn deterministic_reflect(input: &ReflectionInput) -> Vec<MemoryCandidate> {
    let mut candidates: Vec<MemoryCandidate> = Vec::new();

    match input.task_status {
        TaskStatus::Completed => {
            // Task experience (knowledge) from agent results or the description.
            let results = input
                .agent_result_summaries
                .iter()
                .map(|s| truncate_chars(s, 300))
                .collect::<Vec<_>>()
                .join("；");
            let experience = if !results.is_empty() {
                format!("任务《{}》完成，关键结果：{}", input.task_title, results)
            } else if !input.task_description.trim().is_empty() {
                format!(
                    "任务《{}》完成：{}",
                    input.task_title,
                    input.task_description.trim()
                )
            } else {
                String::new()
            };
            if !experience.is_empty() {
                push_candidate(
                    &mut candidates,
                    MemoryCategory::Knowledge,
                    experience,
                    input,
                    1.0,
                );
            }

            // Artifact knowledge (bounded by the execution-wide cap).
            for summary in &input.artifact_summaries {
                if candidates.len() >= MAX_MEMORY_CANDIDATES_PER_EXECUTION {
                    break;
                }
                if summary.trim().is_empty() {
                    continue;
                }
                let content = format!("任务《{}》产生产物：{}", input.task_title, summary.trim());
                push_candidate(
                    &mut candidates,
                    MemoryCategory::Knowledge,
                    content,
                    input,
                    0.8,
                );
            }
        }
        TaskStatus::Failed => {
            if let Some(reason) = input.error_summary.as_deref() {
                if !reason.trim().is_empty() {
                    let content = format!("任务《{}》失败：{}", input.task_title, reason);
                    push_candidate(&mut candidates, MemoryCategory::Note, content, input, 0.8);
                }
            }
        }
        // Cancelled / Blocked / Waiting* / non-terminal produce no long-term
        // memory. (An interrupted run surfaces as a Blocked task.)
        TaskStatus::Cancelled
        | TaskStatus::WaitingApproval
        | TaskStatus::WaitingUser
        | TaskStatus::Draft
        | TaskStatus::Ready
        | TaskStatus::Running
        | TaskStatus::Blocked => {}
    }

    candidates.truncate(MAX_MEMORY_CANDIDATES_PER_EXECUTION);
    candidates
}

fn push_candidate(
    candidates: &mut Vec<MemoryCandidate>,
    category: MemoryCategory,
    content: String,
    input: &ReflectionInput,
    confidence: f32,
) {
    if candidates.len() >= MAX_MEMORY_CANDIDATES_PER_EXECUTION {
        return;
    }
    let content = truncate_chars(&content, MAX_CANDIDATE_CONTENT_CHARS);
    candidates.push(MemoryCandidate::new(
        category,
        content,
        Some(input.task_id.clone()),
        input.agent_name.clone(),
        confidence,
    ));
}

/// Sanitize a failure reason for persistence: char-safe bounded and
/// secret-free. Returns `None` when the content cannot be safely stored.
pub fn sanitize_failure_summary(raw: &str) -> Option<String> {
    let redacted = crate::safety::redact_error(raw);
    let bounded = truncate_chars(&redacted, 1000);
    if crate::safety::contains_sensitive_content(&bounded) {
        // Still sensitive after redaction — reject rather than risk storing it.
        None
    } else {
        Some(bounded)
    }
}
