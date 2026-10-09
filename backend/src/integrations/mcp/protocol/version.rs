// ============================================================
// MCP protocol era and request metadata.
//
// Modern MCP (2026-07-28) is request-scoped: every HTTP POST carries its own
// protocol metadata and method/name headers. There is no Mcp-Session-Id, no
// persistent GET SSE session, and no Last-Event-ID resume.
//
// Header-value encoding and injection defense live in `security::header` —
// they are a security boundary, not a protocol version concern.
// ============================================================

pub const MODERN_MCP_VERSION: &str = "2026-07-28";
pub const LEGACY_MCP_VERSION: &str = "2025-11-25";

const CLIENT_NAME: &str = "YiLianQianYan";

/// Which protocol era a server speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpProtocolEra {
    Modern2026,
    Legacy2025,
}

/// The `_meta` object attached to every modern request's params.
pub fn build_request_metadata(version: &str) -> serde_json::Value {
    serde_json::json!({
        "io.modelcontextprotocol/protocolVersion": MODERN_MCP_VERSION,
        "io.modelcontextprotocol/clientInfo": {
            "name": CLIENT_NAME,
            "version": version,
        },
        "io.modelcontextprotocol/clientCapabilities": {},
    })
}

/// Build the `params._meta` object for a modern JSON-RPC request.
pub fn attach_request_metadata(params: &mut serde_json::Value, app_version: &str) {
    if let serde_json::Value::Object(map) = params {
        map.insert("_meta".to_string(), build_request_metadata(app_version));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modern_request_contains_required_meta() {
        let meta = build_request_metadata("0.9.0");
        assert_eq!(
            meta["io.modelcontextprotocol/protocolVersion"],
            MODERN_MCP_VERSION
        );
        assert_eq!(
            meta["io.modelcontextprotocol/clientInfo"]["name"],
            CLIENT_NAME
        );
    }
}
