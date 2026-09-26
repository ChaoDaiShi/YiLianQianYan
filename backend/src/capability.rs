// ============================================================
// Compatibility facade — Capability Registry moved to `crate::modules::capability`
// ============================================================
//
// Capability discovery, its model and the builtin / runtime providers now live
// under `modules::capability`. Retained so existing `crate::capability::*` call
// sites keep compiling while they migrate; holds no implementation of its own.

pub use crate::modules::capability::*;
