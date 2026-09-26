// ============================================================
// Legacy stdio client tests.
//
// The handshake and `tools/call` suites share one in-memory duplex fixture, so
// the helpers live here and the two suites are separate files: adding a probe
// test cannot accidentally depend on call-path state, and vice versa.
// ============================================================

mod call;
mod probe;

use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

fn duplex_pair() -> (DuplexStream, DuplexStream) {
    tokio::io::duplex(8192)
}

/// A persistent buffered reader so the mock server never loses
/// buffered lines between reads.
type MockReader = BufReader<DuplexStream>;

async fn read_server_line(reader: &mut MockReader) -> serde_json::Value {
    let mut buf = String::new();
    AsyncBufReadExt::read_line(reader, &mut buf).await.unwrap();
    serde_json::from_str(buf.trim()).unwrap()
}

async fn write_server_line(reader: &mut MockReader, msg: &serde_json::Value) {
    let mut line = serde_json::to_string(msg).unwrap();
    line.push('\n');
    let io = reader.get_mut();
    io.write_all(line.as_bytes()).await.unwrap();
    io.flush().await.unwrap();
}

fn initialize_response(protocol_version: &str) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "protocolVersion": protocol_version,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "filesystem", "version": "1.0.0" }
        }
    })
}
