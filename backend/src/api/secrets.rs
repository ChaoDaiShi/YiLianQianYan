// ============================================================
// Secrets status API — value-free, control-session protected.
//
// There is deliberately NO endpoint to read a secret value. This only reports
// store availability, migration state, and per-secret source/configured flags.
// ============================================================

use axum::{extract::State, Json};
use std::sync::Arc;

use super::settings::{chat_source, embedding_source};
use crate::secret::SecretStoreStatus;
use crate::server::AppServer;

pub async fn status_handler(State(server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    let config = server.config.read().clone();
    let store_status = server.secret_resolver.status().await;
    let available = store_status == SecretStoreStatus::Available;

    let (chat_src, chat_configured) = chat_source(&server.secret_resolver, &config).await;
    let (embed_src, embed_configured) = embedding_source(&server.secret_resolver, &config).await;

    let mut migration_pending = 0usize;
    if !config.model.api_key.is_empty() {
        migration_pending += 1;
    }
    if !config.model.embedding_api_key.is_empty() {
        migration_pending += 1;
    }
    let servers = server.db.list_mcp_servers().unwrap_or_default();
    migration_pending += servers
        .iter()
        .filter(|s| s.transport == "stdio")
        .filter(|s| {
            s.env
                .as_ref()
                .and_then(|e| e.as_object())
                .map(|o| !o.is_empty())
                .unwrap_or(false)
        })
        .count();
    let mcp_env_secret_count: usize = servers.iter().map(|s| s.env_secret_refs.len()).sum();

    Json(serde_json::json!({
        "backend": "os_keyring",
        "available": available,
        "migration": {
            "pending": migration_pending,
            "failed": 0,
        },
        "chat_api_key": {
            "configured": chat_configured,
            "source": chat_src,
        },
        "embedding_api_key": {
            "configured": embed_configured,
            "source": embed_src,
        },
        "mcp_env_secret_count": mcp_env_secret_count,
    }))
}
