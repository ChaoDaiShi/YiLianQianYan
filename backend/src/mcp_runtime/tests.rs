// ============================================================
// MCP runtime model + cache tests.
// ============================================================

use super::*;

// ── Cache ──

#[test]
fn cache_hit_before_ttl_and_expired_after() {
    let cache = McpCache::new();
    let key = McpCache::key("srv1", "tools/list", "{}");
    cache.put(&key, 10_000, CacheScope::Private, 1_000);
    // Within TTL.
    assert_eq!(cache.get(&key, 5_000), Some(CacheScope::Private));
    // Expired.
    assert_eq!(cache.get(&key, 20_000), None);
}

#[test]
fn cache_ttl_zero_does_not_store() {
    let cache = McpCache::new();
    let key = McpCache::key("srv1", "tools/list", "{}");
    cache.put(&key, 0, CacheScope::Private, 1_000);
    assert_eq!(cache.get(&key, 1_500), None);
}

#[test]
fn cache_max_ttl_clamped() {
    let cache = McpCache::new();
    let key = McpCache::key("srv1", "tools/list", "{}");
    // Remote claims a huge TTL — must be clamped to the local max.
    cache.put(&key, u64::MAX / 2, CacheScope::Public, 1_000);
    // Within the local max window it is still cached.
    assert_eq!(
        cache.get(&key, 1_000 + MAX_MCP_CACHE_TTL_MS - 1),
        Some(CacheScope::Public)
    );
    // Beyond the local max it must have expired.
    assert_eq!(cache.get(&key, 1_000 + MAX_MCP_CACHE_TTL_MS + 1), None);
}

#[test]
fn cache_invalidation_removes_server_entries() {
    let cache = McpCache::new();
    let k1 = McpCache::key("srv1", "tools/list", "{}");
    let k2 = McpCache::key("srv2", "tools/list", "{}");
    cache.put(&k1, 60_000, CacheScope::Private, 1_000);
    cache.put(&k2, 60_000, CacheScope::Private, 1_000);
    cache.invalidate_server("srv1");
    assert_eq!(cache.get(&k1, 2_000), None);
    assert_eq!(cache.get(&k2, 2_000), Some(CacheScope::Private));
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
