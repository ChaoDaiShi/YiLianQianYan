// ============================================================
// Compatibility facade — LLM client moved to `crate::integrations::llm`
// ============================================================
//
// The OpenAI-compatible client, its request/response types and usage
// accounting now live under `integrations::llm`. Retained so existing
// `crate::llm::*` call sites keep compiling while they migrate; holds no
// implementation of its own.

pub use crate::integrations::llm::*;
