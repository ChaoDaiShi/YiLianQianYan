// ============================================================
// Compatibility facade — Memory-to-Skill moved to `crate::modules::memory_skill`
// ============================================================
//
// Candidate review, evidence screening and the version lifecycle now live
// under `modules::memory_skill`. Retained so existing `crate::memory_skill::*`
// call sites keep compiling while they migrate; holds no implementation of its
// own.

pub use crate::modules::memory_skill::*;
