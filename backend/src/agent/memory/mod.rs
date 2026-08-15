// ============================================================
// Agent Memory Runtime — integrate the existing Memory System into
// the Agent Runtime.
//
// A [`MemoryContextBuilder`] turns a task/step into a retrieval query, runs
// hybrid retrieval (lexical + vector, with lexical fallback), ranks the results,
// and produces a bounded, UI-safe context string that the Agent injects into its
// LLM context. It never fetches or exposes the raw embedding vectors.
// ============================================================

pub mod context;

#[cfg(test)]
mod tests;

pub use context::{
    build_context_text, build_memory_query, MemoryContext, MemoryContextBuilder,
    MemoryRetrievalMode, MAX_MEMORY_CONTEXT_CHARS, MAX_MEMORY_QUERY_CHARS,
};
