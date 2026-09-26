// ============================================================
// MCP runtime models — typed, bounded, serialization-safe.
//
// Sensitive runtime state (child handles, secret config) is never represented
// here; this module holds only safe, bounded, display-ready models.
// ============================================================

use serde::{Deserialize, Serialize};

// ── Limits ──

pub const MAX_MCP_TOOLS_PER_SERVER: usize = 1000;
pub const MAX_MCP_RESOURCES_PER_SERVER: usize = 2000;
pub const MAX_MCP_LIST_PAGES: usize = 100;
pub const MAX_MCP_SCHEMA_BYTES: usize = 256 * 1024;
pub const MAX_MCP_SCHEMA_DEPTH: usize = 64;
pub const MAX_MCP_STDIO_LINE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_MCP_REQUEST_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_MCP_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RESOURCE_CONTENT_ITEMS: usize = 32;
pub const MAX_RESOURCE_TEXT_CHARS: usize = 2_000_000;
pub const MAX_RESOURCE_BLOB_BASE64_CHARS: usize = 8_000_000;
pub const MAX_MCP_CACHE_TTL_MS: u64 = 60 * 60 * 1000;
pub const MAX_MCP_SSE_EVENT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_MCP_SSE_EVENTS_PER_REQUEST: usize = 10_000;

// ── Protocol / status ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpProtocolVersion {
    V2026_07_28,
    V2025_11_25,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpRuntimeStatus {
    Disconnected,
    Connecting,
    Ready,
    Degraded,
    Unavailable,
    Disabled,
    Misconfigured,
}

impl McpProtocolVersion {
    /// Canonical wire version string (matches the MCP spec, not the Rust
    /// variant name).
    pub const fn as_str(self) -> &'static str {
        match self {
            McpProtocolVersion::V2026_07_28 => "2026-07-28",
            McpProtocolVersion::V2025_11_25 => "2025-11-25",
        }
    }
}

impl McpRuntimeStatus {
    /// Canonical snake_case status string.
    pub const fn as_str(self) -> &'static str {
        match self {
            McpRuntimeStatus::Disconnected => "disconnected",
            McpRuntimeStatus::Connecting => "connecting",
            McpRuntimeStatus::Ready => "ready",
            McpRuntimeStatus::Degraded => "degraded",
            McpRuntimeStatus::Unavailable => "unavailable",
            McpRuntimeStatus::Disabled => "disabled",
            McpRuntimeStatus::Misconfigured => "misconfigured",
        }
    }
}

/// The result of a transport `connect()`: the negotiated protocol version and
/// the server's advertised capabilities.
#[derive(Debug, Clone, PartialEq)]
pub struct McpNegotiationResult {
    pub protocol_version: McpProtocolVersion,
    pub capabilities: McpServerCapabilities,
}

/// Parse the `capabilities` object from a discover/initialize result. A
/// capability is "supported" when its key is present (the value is an object or
/// `true`).
pub fn parse_capabilities(value: &serde_json::Value) -> McpServerCapabilities {
    let caps = value.get("capabilities");
    let has = |name: &str| caps.and_then(|c| c.get(name)).is_some_and(|v| !v.is_null());
    McpServerCapabilities {
        tools: has("tools"),
        resources: has("resources"),
        prompts: has("prompts"),
        list_changed: caps
            .and_then(|c| c.get("tools"))
            .and_then(|t| t.get("listChanged"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        extensions: serde_json::Value::Null,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerCapabilities {
    #[serde(default)]
    pub tools: bool,
    #[serde(default)]
    pub resources: bool,
    #[serde(default)]
    pub prompts: bool,
    #[serde(default)]
    pub list_changed: bool,
    #[serde(default)]
    pub extensions: serde_json::Value,
}

// ── Tool model ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<serde_json::Value>,
    /// Computed `x-mcp-header` bindings (not part of the wire schema).
    #[serde(skip, default)]
    pub header_bindings: Vec<McpHeaderBinding>,
}

/// The primitive types allowed for an `x-mcp-header` annotated argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpHeaderValueType {
    String,
    Integer,
    Boolean,
}

/// A parsed `x-mcp-header` binding: the argument path + HTTP header name it
/// maps to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpHeaderBinding {
    pub argument_path: Vec<String>,
    pub header_name: String,
    pub value_type: McpHeaderValueType,
}

// ── Resource model ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceDescriptor {
    pub uri: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceTemplate {
    pub uri_template: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpResourceContent {
    Text {
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        text: String,
    },
    Blob {
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        blob_base64: String,
    },
}

// ── Prompt model ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpPromptArgument {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpPromptDescriptor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub arguments: Vec<McpPromptArgument>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpPromptMessageContent {
    Text { text: String },
    Image { data: String, mime_type: String },
    Audio { data: String, mime_type: String },
    ResourceLink { uri: String, name: String },
    EmbeddedResource { resource: McpResourceContent },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum McpPromptMessage {
    User { content: McpPromptMessageContent },
    Assistant { content: McpPromptMessageContent },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpPromptResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub messages: Vec<McpPromptMessage>,
}

// ── MRTR outcome ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInputRequired {
    pub prompt: String,
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}

/// A bounded MRTR-aware operation outcome. `InputRequired` is a distinct,
/// non-success state (never auto-resumed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum McpOperationOutcome<T> {
    Complete(T),
    InputRequired(McpInputRequired),
}

// ── Error model ──

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum McpRuntimeError {
    #[error("MCP server not found")]
    ServerNotFound,
    #[error("MCP server is disabled")]
    Disabled,
    #[error("MCP tool not found: {0}")]
    ToolNotFound(String),
    #[error("MCP server is misconfigured")]
    Misconfigured,
    #[error("MCP server failed to spawn: {0}")]
    SpawnFailed(String),
    #[error("MCP transport error: {0}")]
    Transport(String),
    #[error("MCP request timed out")]
    Timeout,
    #[error("MCP protocol error: {0}")]
    Protocol(String),
    #[error("MCP server does not support the requested protocol version")]
    UnsupportedProtocol,
    #[error("MCP method is not supported: {0}")]
    MethodUnsupported(String),
    #[error("MCP capability is not supported: {0}")]
    CapabilityUnsupported(String),
    #[error("MCP returned an invalid response")]
    InvalidResponse,
    #[error("MCP response exceeded the size limit")]
    ResponseTooLarge,
    #[error("MCP request exceeded the size limit")]
    RequestTooLarge,
    #[error("MCP server rejected the request as unauthorized")]
    Unauthorized,
    #[error("MCP server requires interactive input (MRTR)")]
    InputRequired,
    #[error("MCP request was cancelled")]
    Cancelled,
    #[error("MCP server returned an error: {0}")]
    ServerError(String),
}
