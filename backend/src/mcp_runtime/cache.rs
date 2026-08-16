// ============================================================
// MCP catalog cache — process-local, TTL-bounded.
//
// Respects a remote `ttlMs`, clamped to a local maximum. `ttlMs = 0` means
// "do not reuse". Manual refresh and configuration changes invalidate.
// ============================================================

use std::collections::HashMap;

use parking_lot::RwLock;
use serde::Serialize;

use super::model::MAX_MCP_CACHE_TTL_MS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheScope {
    Private,
    Public,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    expires_at: u64,
    scope: CacheScope,
}

/// A process-local cache keyed by (server_id, method, params identity).
pub struct McpCache {
    entries: RwLock<HashMap<String, CacheEntry>>,
}

impl McpCache {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn key(server_id: &str, method: &str, params_identity: &str) -> String {
        format!("{server_id}|{method}|{params_identity}")
    }

    /// Look up a cached value if it has not expired. `None` means miss.
    pub fn get(&self, key: &str, now_ms: u64) -> Option<CacheScope> {
        let entries = self.entries.read();
        entries.get(key).and_then(|entry| {
            if entry.expires_at > now_ms {
                Some(entry.scope)
            } else {
                None
            }
        })
    }

    /// Store a value with a remote ttl (clamped). `ttl_ms = 0` stores nothing.
    pub fn put(&self, key: &str, ttl_ms: u64, scope: CacheScope, now_ms: u64) {
        if ttl_ms == 0 {
            return;
        }
        let clamped = ttl_ms.min(MAX_MCP_CACHE_TTL_MS);
        self.entries.write().insert(
            key.to_string(),
            CacheEntry {
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
