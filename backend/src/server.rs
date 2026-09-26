// ============================================================
// Compatibility facade — application composition moved to `crate::app`
// ============================================================
//
// `AppServer` state, its construction, and the serving lifecycle now live in
// `crate::app::state` and `crate::app::bootstrap`/`crate::app::lifecycle`.
// This module is retained so existing `crate::server::*` call sites keep
// compiling while they migrate; it holds no implementation of its own.

pub use crate::app::state::{
    AppServer, DiscoveredSubagent, LogBuffer, LogEntry, CHAT_MEMORY_MAX_CHARS, CHAT_MEMORY_TOP_K,
};

pub(crate) use crate::integrations::mcp::config::mcp_transport_config;
