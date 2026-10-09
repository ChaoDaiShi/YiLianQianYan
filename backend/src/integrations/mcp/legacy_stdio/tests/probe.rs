// ============================================================
// Legacy stdio client — handshake, pagination, error and config tests.
// ============================================================

use super::super::spawn::resolve_env;
use super::super::*;
use super::*;
use crate::db::McpServer;
use std::time::Duration;

// ── 1. initialize request is correct ──

#[tokio::test]
async fn initialize_request_is_correct() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let req = read_server_line(&mut reader).await;
        assert_eq!(req["jsonrpc"], "2.0");
        assert_eq!(req["method"], "initialize");
        assert_eq!(req["params"]["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert_eq!(req["params"]["clientInfo"]["name"], "yilianqianyan");
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
    });

    let mut session = McpSession::new(client);
    let info = session.initialize().await.unwrap();
    assert_eq!(info.protocol_version, MCP_PROTOCOL_VERSION);
    assert_eq!(info.server_name.as_deref(), Some("filesystem"));
    assert_eq!(info.server_version.as_deref(), Some("1.0.0"));
    mock.await.unwrap();
}

// ── 2. initialized notification is sent (no id) ──

#[tokio::test]
async fn initialized_notification_is_sent_without_id() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await; // initialize
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let notif = read_server_line(&mut reader).await;
        assert_eq!(notif["method"], "notifications/initialized");
        assert!(notif.get("id").is_none());
    });

    let mut session = McpSession::new(client);
    session.initialize().await.unwrap();
    session.send_initialized().await.unwrap();
    mock.await.unwrap();
}

// ── 3. tools/list is parsed ──

#[tokio::test]
async fn tools_list_parsed() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await; // initialize
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let notif = read_server_line(&mut reader).await;
        assert_eq!(notif["method"], "notifications/initialized");
        let list = read_server_line(&mut reader).await;
        assert_eq!(list["method"], "tools/list");
        write_server_line(
            &mut reader,
            &json!({
                "jsonrpc": "2.0",
                "id": 2,
                "result": {
                    "tools": [
                        { "name": "echo", "description": "Echo input", "inputSchema": { "type": "object" } }
                    ]
                }
            }),
        )
        .await;
    });

    let mut session = McpSession::new(client);
    let _info = session.initialize().await.unwrap();
    session.send_initialized().await.unwrap();
    let tools = session.list_tools().await.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");
    assert_eq!(tools[0].description.as_deref(), Some("Echo input"));
    assert_eq!(tools[0].input_schema["type"], "object");
    mock.await.unwrap();
}

// ── 4. notification before response does not misalign ──

#[tokio::test]
async fn notification_before_response_does_not_misalign() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await; // initialize
                                                     // Send a notification first, then the response.
        write_server_line(
            &mut reader,
            &json!({ "jsonrpc": "2.0", "method": "notifications/message", "params": {} }),
        )
        .await;
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
    });

    let mut session = McpSession::new(client);
    let info = session.initialize().await.unwrap();
    assert_eq!(info.protocol_version, MCP_PROTOCOL_VERSION);
    mock.await.unwrap();
}

// ── 5. RPC error is surfaced ──

#[tokio::test]
async fn rpc_error_is_reported() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await;
        write_server_line(
            &mut reader,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "error": { "code": -32601, "message": "Method not found" }
            }),
        )
        .await;
    });

    let mut session = McpSession::new(client);
    let err = session.initialize().await.unwrap_err();
    assert!(matches!(err, McpError::Rpc { code: -32601, .. }));
    mock.await.unwrap();
}

// ── 6. protocol version mismatch ──

#[tokio::test]
async fn protocol_version_mismatch_is_reported() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await;
        write_server_line(&mut reader, &initialize_response("2024-11-05")).await;
    });

    let mut session = McpSession::new(client);
    let err = session.initialize().await.unwrap_err();
    assert!(matches!(
        err,
        McpError::ProtocolVersionMismatch(v) if v == "2024-11-05"
    ));
    mock.await.unwrap();
}

// ── 7. pagination merges pages ──

#[tokio::test]
async fn tools_list_pagination_merges_pages() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await; // initialize
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let _ = read_server_line(&mut reader).await; // initialized
                                                     // Page 1
        let l1 = read_server_line(&mut reader).await;
        let id1 = l1["id"].as_i64().unwrap();
        write_server_line(
            &mut reader,
            &json!({
                "jsonrpc": "2.0",
                "id": id1,
                "result": { "tools": [{ "name": "A", "inputSchema": {} }], "nextCursor": "page2" }
            }),
        )
        .await;
        // Page 2
        let l2 = read_server_line(&mut reader).await;
        assert_eq!(l2["params"]["cursor"], "page2");
        let id2 = l2["id"].as_i64().unwrap();
        write_server_line(
            &mut reader,
            &json!({
                "jsonrpc": "2.0",
                "id": id2,
                "result": { "tools": [{ "name": "B", "inputSchema": {} }] }
            }),
        )
        .await;
    });

    let mut session = McpSession::new(client);
    session.initialize().await.unwrap();
    session.send_initialized().await.unwrap();
    let tools = session.list_tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["A", "B"]);
    mock.await.unwrap();
}

// ── 8. pagination limit ──

#[tokio::test]
async fn tools_list_pagination_limit() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await; // initialize
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let _ = read_server_line(&mut reader).await; // initialized
                                                     // Keep replying with nextCursor until the client drops.
        loop {
            let line = match read_server_line_opt(&mut reader).await {
                Some(line) => line,
                None => break, // client closed
            };
            let id = line["id"].as_i64().unwrap();
            write_server_line(
                &mut reader,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "tools": [], "nextCursor": "again" }
                }),
            )
            .await;
        }
    });

    let mut session = McpSession::new(client);
    session.initialize().await.unwrap();
    session.send_initialized().await.unwrap();
    let err = session.list_tools().await.unwrap_err();
    assert!(matches!(err, McpError::PaginationLimit));
    // Drop the client session to close the duplex, letting the mock
    // observe EOF and finish its read-until-close loop.
    drop(session);
    mock.await.unwrap();
}

/// Read a server line, returning `None` on EOF.
async fn read_server_line_opt(reader: &mut MockReader) -> Option<serde_json::Value> {
    let mut buf = String::new();
    let n = AsyncBufReadExt::read_line(reader, &mut buf).await.ok()?;
    if n == 0 {
        return None;
    }
    serde_json::from_str(buf.trim()).ok()
}

// ── 9. invalid env is rejected ──

#[test]
fn invalid_env_is_rejected() {
    let env = Some(json!({ "TOKEN": 123 }));
    assert!(matches!(resolve_env(&env), Err(McpError::InvalidConfig(_))));
}

#[test]
fn valid_env_is_accepted() {
    let env = Some(json!({ "TOKEN": "abc", "EMPTY": "" }));
    let resolved = resolve_env(&env).unwrap();
    assert!(resolved.contains(&("TOKEN".to_string(), "abc".to_string())));
}

// ── 10. missing command is rejected ──

#[test]
fn missing_command_is_rejected() {
    let server = McpServer {
        id: "id".to_string(),
        name: "n".to_string(),
        transport: "stdio".to_string(),
        command: None,
        args: None,
        url: None,
        env: None,
        env_secret_refs: Default::default(),
        enabled: true,
        created_at: 0,
        updated_at: 0,
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let err = rt.block_on(probe_stdio_server(&server)).unwrap_err();
    assert!(matches!(err, McpError::InvalidConfig(_)));
}

// ── 11. timeout does not hang ──

#[tokio::test]
async fn initialize_times_out_when_server_is_silent() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await; // initialize
                                                     // Never respond.
        tokio::time::sleep(Duration::from_secs(30)).await;
    });

    let mut session = McpSession::new(client);
    let start = std::time::Instant::now();
    let result = tokio::time::timeout(Duration::from_millis(200), session.initialize()).await;
    assert!(result.is_err()); // timed out
    assert!(start.elapsed() < Duration::from_secs(5));
    mock.abort();
}
