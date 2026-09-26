// ============================================================
// Legacy MCP client protocol foundation — stdio JSON-RPC handshake +
// one-shot tool execution.
//
// Implements the minimal MCP client lifecycle over a stdio child process:
//
//   initialize
//   → notifications/initialized
//   → tools/list (with pagination)
//   → tools/call (single remote tool execution)
//
// This is the 2025-11-25 era client, retained for servers that never
// negotiated the modern request-scoped protocol in `transport`. MCP tools are
// NOT registered with the Agent, and `McpToolAdapter::execute()` on the
// modern path remains fail-closed.
//
// Split by role:
//
//   error    — protocol constants and the `McpError` surface
//   types    — wire result types and the bounded `ToolResult` bridge
//   session  — the stateful JSON-RPC session over any async read/write pair
//   spawn    — how a legacy server config becomes a spawned child
//   probe    — handshake + `tools/list` discovery
//   call     — one-shot `tools/call` execution
// ============================================================

pub mod call;
pub mod error;
pub mod probe;
pub mod session;
pub mod spawn;
pub mod types;

#[cfg(test)]
mod tests;

pub use call::call_stdio_tool;
pub use error::{
    McpError, MAX_MCP_TOOL_RESULT_CHARS, MAX_TOOL_LIST_PAGES, MCP_PROBE_TIMEOUT,
    MCP_PROTOCOL_VERSION, MCP_TOOL_CALL_TIMEOUT,
};
pub use probe::probe_stdio_server;
pub use session::McpSession;
pub use types::{McpCallResult, McpInitializeInfo, McpProbeResult, McpTool};
