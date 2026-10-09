// ============================================================
// Legacy MCP stdio client — constants and error surface.
//
// This is the pre-2026 client (`MCP_PROTOCOL_VERSION` = 2025-11-25), kept for
// servers that never negotiated the modern request-scoped protocol. The
// modern client is `transport::stdio`; the runtime manager picks between them.
// ============================================================

use std::time::Duration;

/// MCP protocol version supported by this client.
pub const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

/// Whole-probe timeout (spawn + handshake + tools/list).
pub const MCP_PROBE_TIMEOUT: Duration = Duration::from_secs(8);

/// Timeout for a one-shot stdio tools/call (initialize + initialized + call).
pub const MCP_TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum characters of the final `ToolResult.content` (UTF-8 safe truncation).
pub const MAX_MCP_TOOL_RESULT_CHARS: usize = 16_000;

/// Safety cap on pages fetched from a single tools/list pagination loop.
pub const MAX_TOOL_LIST_PAGES: usize = 20;

/// Safety cap on messages consumed while waiting for one response.
pub(super) const MAX_MESSAGES_PER_RESPONSE: usize = 100;

// ── Errors ──

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("invalid MCP config: {0}")]
    InvalidConfig(String),
    #[error("failed to start MCP server: {0}")]
    Spawn(String),
    #[error("MCP I/O error: {0}")]
    Io(String),
    #[error("invalid MCP JSON-RPC message: {0}")]
    InvalidMessage(String),
    #[error("MCP server returned error {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("unsupported MCP protocol version: {0}")]
    ProtocolVersionMismatch(String),
    #[error("MCP operation timed out")]
    Timeout,
    #[error("MCP tools/list exceeded pagination limit")]
    PaginationLimit,
    #[error("invalid MCP tool call: {0}")]
    InvalidToolCall(String),
}
