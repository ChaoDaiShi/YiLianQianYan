// ============================================================
// MCP Runtime primitives — protocol era, request metadata, header
// generation, and transport config. These are the pure, security-critical
// building blocks for the MCP Runtime Manager (no network I/O here).
//
// This module deliberately does NOT contain call_tool: real execution remains
// behind the Security Execution Gateway + the existing tool adapters.
// ============================================================

pub mod protocol;
pub mod transport;

pub use protocol::{
    build_request_metadata, encode_header_value, is_valid_header_token, reject_crlf,
    McpProtocolEra, MODERN_MCP_VERSION,
};
pub use transport::{validate_mcp_url, McpTransportConfig};
