// ============================================================
// Compatibility facade — managed skill store moved to
// `crate::modules::memory_skill::store`
// ============================================================
//
// Retained so existing `crate::skill_management::*` call sites keep compiling
// while they migrate; holds no implementation of its own.

pub use crate::modules::memory_skill::store::*;
