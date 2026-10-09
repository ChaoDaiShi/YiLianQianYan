// ============================================================
// Legacy MCP stdio client — probe (initialize + tools/list).
// ============================================================

use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::db::McpServer;

use super::error::{McpError, MCP_PROBE_TIMEOUT};
use super::session::McpSession;
use super::spawn::spawn_stdio_child;
use super::types::McpProbeResult;

/// Spawn a stdio MCP server, complete the initialize + tools/list
/// handshake, and return discovered metadata.
///
/// The child process is always cleaned up (killed + reaped) whether the
/// probe succeeds or fails.
pub async fn probe_stdio_server(server: &McpServer) -> Result<McpProbeResult, McpError> {
    probe_stdio_server_with_timeout(server, MCP_PROBE_TIMEOUT).await
}

async fn probe_stdio_server_with_timeout(
    server: &McpServer,
    timeout: Duration,
) -> Result<McpProbeResult, McpError> {
    tokio::time::timeout(timeout, probe_stdio_server_inner(server))
        .await
        .map_err(|_| McpError::Timeout)?
}

async fn probe_stdio_server_inner(server: &McpServer) -> Result<McpProbeResult, McpError> {
    let (mut child, io) = spawn_stdio_child(server).await?;

    let mut session = McpSession::new(io);
    let result = run_probe(&mut session).await;

    // Best-effort cleanup: drop session (closes stdin), kill + reap child.
    drop(session);
    let _ = child.kill().await;
    let _ = child.wait().await;

    result
}

async fn run_probe<IO>(session: &mut McpSession<IO>) -> Result<McpProbeResult, McpError>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    let init = session.initialize().await?;
    session.send_initialized().await?;
    let tools = session.list_tools().await?;
    Ok(McpProbeResult {
        protocol_version: init.protocol_version,
        server_name: init.server_name,
        server_version: init.server_version,
        tools,
    })
}
