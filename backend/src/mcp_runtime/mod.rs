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
pub mod http;
pub mod jsonrpc;
pub mod manager;
pub mod model;
pub mod prompts;
pub mod protocol;
pub mod resources;
pub mod stdio;
pub mod tools;
pub mod transport;

#[cfg(test)]
mod http_tests;
#[cfg(test)]
mod stdio_tests;
#[cfg(test)]
mod tests;

pub use cache::{CacheScope, McpCache, McpCacheValue};
pub use header_schema::{extract_header_values, scan_tool_header_bindings};
pub use http::HttpTransport;
pub use jsonrpc::{
    parse_message, JsonRpcError, JsonRpcErrorBody, JsonRpcMessage, JsonRpcNotification,
    JsonRpcRequest, JsonRpcSuccess,
};
pub use manager::{McpRuntimeManager, McpServerRuntime, McpToolCallResult};
pub use model::{
    McpHeaderBinding, McpHeaderValueType, McpInputRequired, McpOperationOutcome, McpPromptArgument,
    McpPromptDescriptor, McpPromptMessage, McpPromptMessageContent, McpPromptResult,
    McpProtocolVersion, McpResourceContent, McpResourceDescriptor, McpResourceTemplate,
    McpRuntimeError, McpRuntimeStatus, McpServerCapabilities, McpTool, MAX_MCP_CACHE_TTL_MS,
    MAX_MCP_LIST_PAGES, MAX_MCP_REQUEST_BYTES, MAX_MCP_RESOURCES_PER_SERVER,
    MAX_MCP_RESPONSE_BYTES, MAX_MCP_TOOLS_PER_SERVER, MAX_RESOURCE_BLOB_BASE64_CHARS,
    MAX_RESOURCE_CONTENT_ITEMS, MAX_RESOURCE_TEXT_CHARS,
};
pub use prompts::{parse_prompt_get, parse_prompt_list};
pub use protocol::{
    attach_request_metadata, build_request_metadata, encode_header_value, is_valid_header_token,
    param_header, reject_crlf, McpProtocolEra, LEGACY_MCP_VERSION, MODERN_MCP_VERSION,
};
pub use resources::{parse_resource_contents, parse_resource_list, parse_resource_template_list};
pub use stdio::StdioTransport;
pub use tools::{call_result_text, parse_call_result, parse_tool_list};
pub use transport::{validate_mcp_url, McpTransportConfig};
