// ============================================================
// MCP integration — protocol era, transports, runtime and catalogues.
//
// The public surface is flat (everything is re-exported below) so callers do
// not depend on how the internals are grouped. Internally the module is split
// by role:
//
//   config      — transport configuration + the legacy plugin-row conversion
//   protocol    — JSON-RPC framing, typed model, protocol era metadata
//   transport   — the `McpTransport` trait, stdio and Streamable HTTP impls
//   runtime     — server lifecycle manager and response cache
//   registry    — what a server advertises (`*/list` pages)
//   result      — what one operation returned (`tools/call`, `*/get`, `read`)
//   security    — header injection defense and `x-mcp-header` scanning
//
// These are the pure, security-critical building blocks for the MCP Runtime
// Manager (no network I/O outside `transport`/`runtime`). This module
// deliberately does NOT expose a `call_tool` entry point: real execution
// remains behind the Security Execution Gateway + the existing tool adapters.
// ============================================================

pub mod config;
pub mod legacy_stdio;
pub mod protocol;
pub mod registry;
pub mod result;
pub mod runtime;
pub mod security;
pub mod transport;

#[cfg(test)]
mod tests;

pub use config::{validate_mcp_url, McpTransportConfig};
pub use protocol::jsonrpc::{
    parse_message, JsonRpcError, JsonRpcErrorBody, JsonRpcMessage, JsonRpcNotification,
    JsonRpcRequest, JsonRpcSuccess,
};
pub use protocol::model::{
    McpHeaderBinding, McpHeaderValueType, McpInputRequired, McpOperationOutcome, McpPromptArgument,
    McpPromptDescriptor, McpPromptMessage, McpPromptMessageContent, McpPromptResult,
    McpProtocolVersion, McpResourceContent, McpResourceDescriptor, McpResourceTemplate,
    McpRuntimeError, McpRuntimeStatus, McpServerCapabilities, McpTool, MAX_MCP_CACHE_TTL_MS,
    MAX_MCP_LIST_PAGES, MAX_MCP_REQUEST_BYTES, MAX_MCP_RESOURCES_PER_SERVER,
    MAX_MCP_RESPONSE_BYTES, MAX_MCP_TOOLS_PER_SERVER, MAX_RESOURCE_BLOB_BASE64_CHARS,
    MAX_RESOURCE_CONTENT_ITEMS, MAX_RESOURCE_TEXT_CHARS,
};
pub use protocol::version::{
    attach_request_metadata, build_request_metadata, McpProtocolEra, LEGACY_MCP_VERSION,
    MODERN_MCP_VERSION,
};
pub use registry::prompts::parse_prompt_list;
pub use registry::resources::{parse_resource_list, parse_resource_template_list};
pub use registry::tools::parse_tool_list;
pub use result::prompt::parse_prompt_get;
pub use result::resource::parse_resource_contents;
pub use result::tool_call::{call_result_text, parse_call_result};
pub use runtime::cache::{CacheScope, McpCache, McpCacheValue};
pub use runtime::manager::{McpRuntimeManager, McpServerRuntime, McpToolCallResult};
pub use security::header::{encode_header_value, is_valid_header_token, param_header, reject_crlf};
pub use security::header_schema::{extract_header_values, scan_tool_header_bindings};
pub use transport::http::HttpTransport;
pub use transport::stdio::StdioTransport;
pub use transport::{McpRequestOptions, McpTransport};
