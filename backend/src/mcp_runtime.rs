// ============================================================
// Compatibility facade — MCP runtime moved to `crate::integrations::mcp`
// ============================================================
//
// Retained so the 9 existing `crate::mcp_runtime::*` call sites keep compiling
// while they migrate; holds no implementation of its own.

pub use crate::integrations::mcp::*;
