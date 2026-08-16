// ============================================================
// MCP Runtime primitives — protocol era, request metadata, header
// generation, and transport config. These are the pure, security-critical
// building blocks for the MCP Runtime Manager (no network I/O here).
//
// This module deliberately does NOT contain call_tool: real execution remains
// behind the Security Execution Gateway + the existing tool adapters.
// ============================================================

pub mod cache;
pub mod header_schema;
pub mod model;
pub mod protocol;
pub mod transport;

#[cfg(test)]
mod tests;

pub use cache::{CacheScope, McpCache};
pub use header_schema::scan_tool_header_bindings;
pub use model::{
    McpHeaderBinding, McpHeaderValueType, McpInputRequired, McpOperationOutcome, McpPromptArgument,
    McpPromptDescriptor, McpPromptMessage, McpPromptMessageContent, McpPromptResult,
    McpProtocolVersion, McpResourceContent, McpResourceDescriptor, McpResourceTemplate,
    McpRuntimeError, McpRuntimeStatus, McpServerCapabilities, McpTool, MAX_MCP_CACHE_TTL_MS,
    MAX_MCP_LIST_PAGES, MAX_MCP_REQUEST_BYTES, MAX_MCP_RESOURCES_PER_SERVER,
    MAX_MCP_RESPONSE_BYTES, MAX_MCP_TOOLS_PER_SERVER, MAX_RESOURCE_BLOB_BASE64_CHARS,
    MAX_RESOURCE_CONTENT_ITEMS, MAX_RESOURCE_TEXT_CHARS,
};
pub use protocol::{
    attach_request_metadata, build_request_metadata, encode_header_value, is_valid_header_token,
    param_header, reject_crlf, McpProtocolEra, LEGACY_MCP_VERSION, MODERN_MCP_VERSION,
};
pub use transport::{validate_mcp_url, McpTransportConfig};
