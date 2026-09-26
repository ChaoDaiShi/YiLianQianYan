// ============================================================
// Compatibility facade — legacy MCP stdio service moved to
// `crate::integrations::mcp::legacy_stdio`
// ============================================================
//
// Retained so the 4 existing `crate::mcp::*` call sites keep compiling while
// they migrate; holds no implementation of its own.

pub use crate::integrations::mcp::legacy_stdio::*;
