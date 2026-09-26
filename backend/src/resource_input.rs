// ============================================================
// Compatibility facade — Resource moved to `crate::modules::resource`
// ============================================================
//
// Bounded extraction, the upload policy, preview projection and node binding
// now live under `modules::resource`. Retained so existing
// `crate::resource_input::*` call sites keep compiling while they migrate;
// holds no implementation of its own.

pub use crate::modules::resource::*;
