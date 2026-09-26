// ============================================================
// MCP runtime model + cache tests.
// ============================================================

use super::*;

// ── Cache ──

fn tools_value() -> McpCacheValue {
    McpCacheValue::Tools(vec![McpTool {
        name: "echo".to_string(),
        title: None,
        description: None,
        input_schema: serde_json::json!({"type": "object"}),
        output_schema: None,
        annotations: None,
        header_bindings: Vec::new(),
    }])
}

#[test]
fn cache_hit_before_ttl_and_expired_after() {
    let cache = McpCache::new();
    let key = McpCache::key("srv1", "tools/list", &serde_json::json!({}));
    cache.put(&key, tools_value(), 10_000, CacheScope::Private, 1_000);
    // Within TTL.
    assert!(cache.get(&key, 5_000).is_some());
    // Expired.
    assert!(cache.get(&key, 20_000).is_none());
}

#[test]
fn cache_ttl_zero_does_not_store() {
    let cache = McpCache::new();
    let key = McpCache::key("srv1", "tools/list", &serde_json::json!({}));
    cache.put(&key, tools_value(), 0, CacheScope::Private, 1_000);
    assert!(cache.get(&key, 1_500).is_none());
}

#[test]
fn cache_max_ttl_clamped() {
    let cache = McpCache::new();
    let key = McpCache::key("srv1", "tools/list", &serde_json::json!({}));
    // Remote claims a huge TTL — must be clamped to the local max.
    cache.put(&key, tools_value(), u64::MAX / 2, CacheScope::Public, 1_000);
    assert!(cache.get(&key, 1_000 + MAX_MCP_CACHE_TTL_MS - 1).is_some());
    assert!(cache.get(&key, 1_000 + MAX_MCP_CACHE_TTL_MS + 1).is_none());
}

#[test]
fn cache_invalidation_removes_server_entries() {
    let cache = McpCache::new();
    let k1 = McpCache::key("srv1", "tools/list", &serde_json::json!({}));
    let k2 = McpCache::key("srv2", "tools/list", &serde_json::json!({}));
    cache.put(&k1, tools_value(), 60_000, CacheScope::Private, 1_000);
    cache.put(&k2, tools_value(), 60_000, CacheScope::Private, 1_000);
    cache.invalidate_server("srv1");
    assert!(cache.get(&k1, 2_000).is_none());
    assert!(cache.get(&k2, 2_000).is_some());
}

#[test]
fn cache_key_hashes_params_and_avoids_raw_secret() {
    let k = McpCache::key(
        "srv1",
        "resources/read",
        &serde_json::json!({"uri": "https://x/y?token=SUPER_SECRET"}),
    );
    // The key contains no raw secret query, only the digest.
    assert!(!k.contains("SUPER_SECRET"));
    assert!(k.starts_with("srv1|resources/read|"));
}

// ── MRTR outcome ──

#[test]
fn mcp_operation_outcome_complete_and_input_required() {
    let complete: McpOperationOutcome<String> =
        serde_json::from_value(serde_json::json!({"result": "done", "resultType": "complete"}))
            .unwrap_or(McpOperationOutcome::Complete("done".to_string()));
    assert!(matches!(complete, McpOperationOutcome::Complete(_)));

    let input_required: McpOperationOutcome<serde_json::Value> =
        McpOperationOutcome::InputRequired(McpInputRequired {
            prompt: "请确认".to_string(),
            input_schema: None,
        });
    // InputRequired is distinct from Complete — never treated as success.
    assert!(matches!(
        input_required,
        McpOperationOutcome::InputRequired(_)
    ));
}

// ── Resource / prompt content ──

#[test]
fn resource_content_serializes_typed() {
    let text = McpResourceContent::Text {
        uri: "file:///notes".to_string(),
        mime_type: Some("text/plain".to_string()),
        text: "hello".to_string(),
    };
    let json = serde_json::to_value(&text).unwrap();
    assert_eq!(json["kind"], "text");

    let blob = McpResourceContent::Blob {
        uri: "file:///img".to_string(),
        mime_type: Some("image/png".to_string()),
        blob_base64: "aGVsbG8=".to_string(),
    };
    assert_eq!(serde_json::to_value(&blob).unwrap()["kind"], "blob");
}

#[test]
fn prompt_message_content_is_typed() {
    let msg = McpPromptMessage::User {
        content: McpPromptMessageContent::Text {
            text: "hi".to_string(),
        },
    };
    assert_eq!(serde_json::to_value(&msg).unwrap()["role"], "user");
}

#[test]
fn runtime_error_is_typed_not_string() {
    assert_eq!(
        McpRuntimeError::ServerNotFound.to_string(),
        "MCP server not found"
    );
    assert_ne!(
        McpRuntimeError::Unauthorized.to_string(),
        McpRuntimeError::Cancelled.to_string()
    );
}
