// ============================================================
// Legacy MCP stdio client — one-shot `tools/call`.
//
// A fresh child is spawned per call: the legacy protocol has no session
// resumption, and a short-lived process keeps a crashed server from being
// mistaken for a live one.
// ============================================================

use tokio::io::{AsyncRead, AsyncWrite};

use crate::db::McpServer;
use crate::tools::trait_def::ToolResult;

use super::error::{McpError, MCP_TOOL_CALL_TIMEOUT};
use super::session::McpSession;
use super::spawn::spawn_stdio_child;

/// Spawn a stdio MCP server, handshake, execute one remote tool, and
/// return the bounded [`ToolResult`].
///
/// The child process is owned OUTSIDE the timeout so that on timeout (or any
/// error) we can explicitly kill + reap it. `kill_on_drop(true)` remains as a
/// final safety net.
pub async fn call_stdio_tool(
    server: &McpServer,
    remote_tool_name: &str,
    arguments: serde_json::Value,
) -> Result<ToolResult, McpError> {
    if remote_tool_name.trim().is_empty() {
        return Err(McpError::InvalidToolCall(
            "remote tool name is empty".to_string(),
        ));
    }
    if !arguments.is_object() {
        return Err(McpError::InvalidToolCall(
            "arguments must be a JSON object".to_string(),
        ));
    }

    let (mut child, io) = spawn_stdio_child(server).await?;
    let mut session = McpSession::new(io);

    let protocol_result = tokio::time::timeout(
        MCP_TOOL_CALL_TIMEOUT,
        run_tool_call(&mut session, remote_tool_name, arguments),
    )
    .await;

    // Child ownership is outside the timeout: always clean up explicitly.
    drop(session);
    let _ = child.kill().await;
    let _ = child.wait().await;

    match protocol_result {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(McpError::Timeout),
    }
}

async fn run_tool_call<IO>(
    session: &mut McpSession<IO>,
    remote_tool_name: &str,
    arguments: serde_json::Value,
) -> Result<ToolResult, McpError>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    session.initialize().await?;
    session.send_initialized().await?;
    let result = session.call_tool(remote_tool_name, arguments).await?;
    Ok(result.into_tool_result())
}
