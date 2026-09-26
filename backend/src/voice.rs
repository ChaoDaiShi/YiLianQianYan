// ============================================================
// Compatibility facade — Global voice moved to `crate::modules::voice`
// ============================================================
//
// The session runtime, the provider adapters and the dispatch contract now
// live under `modules::voice`. Retained so existing `crate::voice::*` call
// sites keep compiling while they migrate; holds no implementation of its own.

pub use crate::modules::voice::*;
