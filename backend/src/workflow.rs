// ============================================================
// Compatibility facade — Workflow moved to `crate::modules::workflow`
// ============================================================
//
// Retained so existing `crate::workflow::*` call sites keep compiling while
// they migrate; holds no implementation of its own.

pub use crate::modules::workflow::*;
