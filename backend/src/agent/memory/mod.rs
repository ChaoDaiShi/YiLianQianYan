// ============================================================
// Agent Memory Runtime — retrieval (Phase 1) + learning loop (Phase 2).
//
//   context.rs     — memory retrieval + context injection (Phase 1)
//   candidate.rs   — MemoryCandidate + MemoryCategory
//   policy.rs      — conservative write validation
//   reflection.rs  — turn a completed task into candidates
//   writer.rs      — validate + persist + embed
//
// Reflection decides WHAT to remember; the writer (via policy + the existing
// Memory store) decides whether it is safe and valuable enough to keep. No
// raw embedding vectors are ever surfaced.
// ============================================================

pub mod candidate;
pub mod context;
pub mod policy;
pub mod reflection;
pub mod writer;

#[cfg(test)]
mod tests;

pub use candidate::{MemoryCandidate, MemoryCategory};
pub use context::{
    build_context_text, build_memory_query, MemoryContext, MemoryContextBuilder,
    MemoryRetrievalMode, MAX_MEMORY_CONTEXT_CHARS, MAX_MEMORY_QUERY_CHARS,
};
pub use policy::{
    validate_candidate, MemoryWritePolicy, ValidationError, DEFAULT_MAX_CONTENT_CHARS,
};
pub use reflection::{
    sanitize_failure_summary, DeterministicMemoryReflector, MemoryReflector, ReflectionInput,
    ReflectorError, MAX_MEMORY_CANDIDATES_PER_EXECUTION,
};
pub use writer::{MemoryLearningReport, MemoryWriter, WriteOutcome, WriteReport};
