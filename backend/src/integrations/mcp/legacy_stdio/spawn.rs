// ============================================================
// Legacy MCP stdio client — child process configuration.
//
// One place decides how a legacy server's command, args, secret-free env map
// and secret refs become a spawned child, so the probe path and the tool-call
// path cannot drift apart on timeout, stderr, or cleanup handling.
// ============================================================

use crate::db::McpServer;
use crate::utils::process::hide_tokio_command_window;

use super::error::McpError;

/// The joined stdin/stdout stream of a spawned legacy MCP child.
pub(super) type StdioChildIo =
    tokio::io::Join<tokio::process::ChildStdout, tokio::process::ChildStdin>;

/// Spawn a legacy stdio MCP child and return it with its joined stdio stream.
///
/// The caller owns the [`tokio::process::Child`] so it can kill + reap
/// explicitly; `kill_on_drop(true)` remains as a final safety net.
pub(super) async fn spawn_stdio_child(
    server: &McpServer,
) -> Result<(tokio::process::Child, StdioChildIo), McpError> {
    let command = server
        .command
        .as_deref()
        .map(str::trim)
        .filter(|cmd| !cmd.is_empty())
        .ok_or_else(|| McpError::InvalidConfig("command is required".to_string()))?;
    let args = server.args.clone().unwrap_or_default();
    let envs = resolve_env(&server.env)?;

    let mut cmd = tokio::process::Command::new(command);
    hide_tokio_command_window(&mut cmd);
    cmd.args(&args);
    for (key, value) in &envs {
        cmd.env(key, value);
    }
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| McpError::Spawn(e.to_string()))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| McpError::Io("child stdout unavailable".to_string()))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| McpError::Io("child stdin unavailable".to_string()))?;
    Ok((child, tokio::io::join(stdout, stdin)))
}

// ── Config resolution ──

/// Validate that `env` is a JSON object of string values.
/// Never returns or logs the env values themselves.
pub(super) fn resolve_env(
    env: &Option<serde_json::Value>,
) -> Result<Vec<(String, String)>, McpError> {
    let Some(value) = env else {
        return Ok(Vec::new());
    };
    let obj = value.as_object().ok_or_else(|| {
        McpError::InvalidConfig("env must be a JSON object of string values".to_string())
    })?;
    let mut out = Vec::with_capacity(obj.len());
    for (key, val) in obj {
        let s = val.as_str().ok_or_else(|| {
            McpError::InvalidConfig(format!("env value for key '{key}' must be a string"))
        })?;
        out.push((key.clone(), s.to_string()));
    }
    Ok(out)
}
