// ============================================================
// Compatibility facade — Secret management moved to `crate::integrations::secret`
// ============================================================
//
// The OS-backed store, SecretRef persistence, legacy migration and runtime
// resolution now live under `integrations::secret`. Retained so existing
// `crate::secret::*` call sites keep compiling while they migrate; holds no
// implementation of its own.
//
// The secret-handling semantics are unchanged by the move: a value never
// belongs in AppConfig or MCP DB persistence, the DB stores only SecretRefs,
// and new writes never fall back to plaintext.

pub use crate::integrations::secret::*;
