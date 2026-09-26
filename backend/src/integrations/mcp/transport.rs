// ============================================================
// MCP transport behaviour — the trait every transport implements.
//
// The stdio and Streamable HTTP implementations live in `stdio.rs` and
// `http.rs`; the *configuration data* that selects between them lives in
// `config.rs`, because it is deserialized from the plugin store and must not
// drag a transport implementation into the caller's dependency graph.
// ============================================================

use std::collections::BTreeMap;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::jsonrpc::{JsonRpcMessage, JsonRpcRequest};
use super::model::{McpNegotiationResult, McpProtocolVersion, McpRuntimeError};

/// Optional per-request transport context (HTTP-only extra headers, e.g. the
/// `Mcp-Param-*` headers derived from `x-mcp-header`).
#[derive(Debug, Clone, Default)]
pub struct McpRequestOptions {
    pub extra_headers: BTreeMap<String, String>,
}

/// A concrete MCP transport (stdio or Streamable HTTP). Implementations own
/// their connection lifecycle and serialize requests as needed.
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Connect (spawn + negotiate for stdio) and return the negotiated protocol
    /// version + advertised capabilities. Idempotent.
    async fn connect(
        &self,
        cancel: &CancellationToken,
    ) -> Result<McpNegotiationResult, McpRuntimeError>;
    async fn send(
        &self,
        request: &JsonRpcRequest,
        cancel: &CancellationToken,
    ) -> Result<JsonRpcMessage, McpRuntimeError> {
        self.send_with_options(request, &McpRequestOptions::default(), cancel)
            .await
    }
    /// Send with transport-specific options (extra headers are ignored by stdio).
    async fn send_with_options(
        &self,
        request: &JsonRpcRequest,
        options: &McpRequestOptions,
        cancel: &CancellationToken,
    ) -> Result<JsonRpcMessage, McpRuntimeError>;
    async fn shutdown(&self);
    /// The negotiated protocol version (available after `connect`).
    fn protocol_version(&self) -> McpProtocolVersion;
}
