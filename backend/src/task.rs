// ============================================================
// Compatibility facade — Task World moved to `crate::modules::task`
// ============================================================
//
// The domain, application and HTTP layers now live under `modules::task`.
// Retained so existing `crate::task::*` call sites keep compiling while they
// migrate; holds no implementation of its own.

pub use crate::modules::task::*;
