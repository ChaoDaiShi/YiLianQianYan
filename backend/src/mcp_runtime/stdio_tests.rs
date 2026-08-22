// ============================================================
// Real I/O stdio integration test — a self-spawned mock MCP server
// process communicates over stdin/stdout with the persistent transport.
// ============================================================

use std::io::{BufRead, Write};
use std::sync::Arc;

use secrecy::SecretString;
use serde_json::{json, Value};

use super::*;
use crate::secret::{InMemorySecretStore, SecretRef, SecretResolver, SecretStore};

/// Acts as a mock MCP server when `YILIAN_MOCK_MCP=1`. Writes directly to the
/// raw stdout fd (bypassing libtest capture) so the parent can read frames.
#[test]
fn mock_stdio_server() {
    if std::env::var("YILIAN_MOCK_MCP").ok().as_deref() != Some("1") {
        return;
    }
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = v.get("id").cloned();
        let method = v.get("method").and_then(|m| m.as_str()).unwrap_or_default();
        // Notifications have no id → no response.
        if id.is_none() {
            continue;
        }
        let response = match method {
            // server/discover fails → client must fall back to legacy initialize.
            "server/discover" => {
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "method not found"}})
            }
            "initialize" => {
                json!({"jsonrpc": "2.0", "id": id, "result": {"protocolVersion": "2025-11-25", "capabilities": {"tools": {}}, "serverInfo": {"name": "mock", "version": "1.0.0"}}})
            }
            "tools/list" => {
                json!({"jsonrpc": "2.0", "id": id, "result": {"tools": [{"name": "echo", "inputSchema": {"type": "object"}}]}})
            }
            "tools/call" => {
                // Echo a test env var so the parent can assert secret resolution.
                let secret = std::env::var("TEST_SECRET").unwrap_or_default();
                json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": format!("stdio ok secret={secret}")}], "isError": false}})
            }
            _ => {
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "method not found"}})
            }
        };
        let mut out = serde_json::to_string(&response).unwrap();
        out.push('\n');
        if stdout.write_all(out.as_bytes()).is_err() {
            break;
        }
        let _ = stdout.flush();
    }
}

fn stdio_config(exe: &std::path::Path) -> McpTransportConfig {
    McpTransportConfig::Stdio {
        command: exe.to_string_lossy().to_string(),
        args: vec![
            "--exact".to_string(),
            "mcp_runtime::stdio_tests::mock_stdio_server".to_string(),
            "--nocapture".to_string(),
        ],
        env: [("YILIAN_MOCK_MCP".to_string(), "1".to_string())]
            .into_iter()
            .collect(),
        env_secret_refs: Default::default(),
    }
}

#[tokio::test]
async fn stdio_legacy_fallback_tools_list_and_call() {
    let exe = std::env::current_exe().expect("test binary path");
    let manager = McpRuntimeManager::new();
    manager.register_server(
        "stdio1".to_string(),
        "Mock Stdio".to_string(),
        stdio_config(&exe),
    );

    manager
        .refresh_server("stdio1")
        .await
        .expect("connect + tools/list");
    // Version propagation: legacy server (server/discover → method not found)
    // must report 2025-11-25, not 2026-07-28.
    let runtime = manager.get_server("stdio1").unwrap();
    assert_eq!(runtime.protocol_version, McpProtocolVersion::V2025_11_25);

    let tools = manager.list_tools("stdio1").await.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");

    let cancel = tokio_util::sync::CancellationToken::new();
    let result = manager
        .call_tool("stdio1", "echo", json!({}), &cancel)
        .await
        .unwrap();
    assert!(result.text.contains("stdio ok"));

    manager.shutdown_all().await;
}

#[tokio::test]
async fn stdio_resolves_secret_ref_env() {
    let exe = std::env::current_exe().expect("test binary path");
    let store = Arc::new(InMemorySecretStore::new());
    let secret_ref = SecretRef::new("mcp.test.env.TEST_SECRET");
    store
        .put(&secret_ref, SecretString::from("abc123".to_string()))
        .await
        .unwrap();
    let resolver = Arc::new(SecretResolver::new(store));
    let manager = McpRuntimeManager::with_resolver(resolver);

    let mut config = stdio_config(&exe);
    if let McpTransportConfig::Stdio {
        env_secret_refs, ..
    } = &mut config
    {
        env_secret_refs.insert("TEST_SECRET".to_string(), secret_ref.clone());
    }
    manager.register_server(
        "stdio_secret".to_string(),
        "Secret Stdio".to_string(),
        config,
    );
    manager.refresh_server("stdio_secret").await.unwrap();

    let cancel = tokio_util::sync::CancellationToken::new();
    let result = manager
        .call_tool("stdio_secret", "echo", json!({}), &cancel)
        .await
        .unwrap();
    // The child received the resolved secret via its environment.
    assert!(result.text.contains("abc123"));

    manager.shutdown_all().await;
}
