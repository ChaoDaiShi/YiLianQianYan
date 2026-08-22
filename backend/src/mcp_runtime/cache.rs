// ============================================================
// MCP catalog cache — process-local, TTL-bounded, stores real results.
//
// Respects a remote `ttlMs` (clamped to a local maximum); `ttlMs = 0` means
// "do not cache". Keys are `server_id | method | sha256(canonical params)`
// so secret-bearing params never appear in the key or logs.
// ============================================================

use std::collections::HashMap;

use parking_lot::RwLock;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::model::{
    McpPromptDescriptor, McpResourceContent, McpResourceDescriptor, McpResourceTemplate, McpTool,
    MAX_MCP_CACHE_TTL_MS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheScope {
    Private,
    Public,
}

/// A typed cached result.
#[derive(Debug, Clone)]
pub enum McpCacheValue {
    Tools(Vec<McpTool>),
    Resources(Vec<McpResourceDescriptor>),
    ResourceTemplates(Vec<McpResourceTemplate>),
    ResourceRead(Vec<McpResourceContent>),
    Prompts(Vec<McpPromptDescriptor>),
}

#[derive(Debug, Clone)]
struct CacheEntry {
    value: McpCacheValue,
    expires_at: u64,
    // Scope is parsed for protocol correctness; Phase 4 treats all scopes as
    // process-local, so it is stored but not consulted for isolation.
    #[allow(dead_code)]
    scope: CacheScope,
}

/// A process-local cache keyed by `server_id | method | sha256(params)`.
pub struct McpCache {
    entries: RwLock<HashMap<String, CacheEntry>>,
}

impl McpCache {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Build a secret-safe cache key: `server_id|method|sha256(canonical_params)`.
    pub fn key(server_id: &str, method: &str, params: &serde_json::Value) -> String {
        let canonical = serde_json::to_string(params).unwrap_or_default();
        let digest = Sha256::digest(canonical.as_bytes());
        let digest_hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        format!("{server_id}|{method}|{digest_hex}")
    }

    /// Look up a cached value if it has not expired. `None` means miss.
    pub fn get(&self, key: &str, now_ms: u64) -> Option<McpCacheValue> {
        let entries = self.entries.read();
        entries.get(key).and_then(|entry| {
            if entry.expires_at > now_ms {
                Some(entry.value.clone())
            } else {
                None
            }
        })
    }

    /// Store a value with a remote ttl (clamped). `ttl_ms = 0` stores nothing.
    pub fn put(
        &self,
        key: &str,
        value: McpCacheValue,
        ttl_ms: u64,
        scope: CacheScope,
        now_ms: u64,
    ) {
        if ttl_ms == 0 {
            return;
        }
        let clamped = ttl_ms.min(MAX_MCP_CACHE_TTL_MS);
        self.entries.write().insert(
            key.to_string(),
            CacheEntry {
                value,
                expires_at: now_ms + clamped,
                scope,
            },
        );
    }

    pub fn invalidate_server(&self, server_id: &str) {
        let prefix = format!("{server_id}|");
        self.entries
            .write()
            .retain(|key, _| !key.starts_with(&prefix));
    }

    pub fn clear(&self) {
        self.entries.write().clear();
    }
}

impl Default for McpCache {
    fn default() -> Self {
        Self::new()
    }
}
