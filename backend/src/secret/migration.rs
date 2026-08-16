// ============================================================
// Legacy secret migration — move plaintext secrets out of SQLite and into the
// OS-backed SecretStore.
//
// Invariants (HARD):
//   - SecretStore write + read-back verification succeeds BEFORE the plaintext
//     is cleared from the DB model.
//   - On any store failure the legacy plaintext is left untouched and the
//     report marks `pending`/`failed`. Data is never lost.
//   - Idempotent: re-running never duplicates refs or corrupts values.
// ============================================================

use secrecy::SecretString;

use super::model::{
    mcp_env_ref, SecretMigrationReport, SecretRef, SecretStoreError, CHAT_KEY_REF,
    EMBEDDING_KEY_REF,
};
use super::store::SecretStore;
use crate::config::types::AppConfig;
use crate::db::McpServer;

/// Migrate legacy plaintext secrets. Mutates `config` + `servers` in place:
/// only entries whose store write was verified have their plaintext cleared.
pub async fn migrate_legacy_secrets(
    store: &dyn SecretStore,
    config: &mut AppConfig,
    servers: &mut [McpServer],
) -> SecretMigrationReport {
    let mut report = SecretMigrationReport::default();

    // ── Chat API key ──
    if config.model.api_key_ref.is_none() && !config.model.api_key.is_empty() {
        let secret_ref = SecretRef::new(CHAT_KEY_REF);
        match migrate_one(store, &secret_ref, &config.model.api_key).await {
            Ok(()) => {
                config.model.api_key_ref = Some(secret_ref);
                config.model.api_key.clear();
                report.migrated_chat_key = true;
            }
            Err(_) => {
                report.pending += 1;
                report.failed += 1;
            }
        }
    }

    // ── Embedding API key ──
    if config.model.embedding_api_key_ref.is_none() && !config.model.embedding_api_key.is_empty() {
        let secret_ref = SecretRef::new(EMBEDDING_KEY_REF);
        match migrate_one(store, &secret_ref, &config.model.embedding_api_key).await {
            Ok(()) => {
                config.model.embedding_api_key_ref = Some(secret_ref);
                config.model.embedding_api_key.clear();
                report.migrated_embedding_key = true;
            }
            Err(_) => {
                report.pending += 1;
                report.failed += 1;
            }
        }
    }

    // ── stdio MCP env values (ALL values, no secret/non-secret guessing) ──
    for server in servers.iter_mut() {
        if server.transport != "stdio" {
            continue;
        }
        let Some(env) = server.env.clone() else {
            continue;
        };
        let Some(obj) = env.as_object() else {
            continue;
        };
        // Track which names were migrated this pass (or were already migrated).
        for (name, value) in obj {
            if server.env_secret_refs.contains_key(name) {
                continue; // already migrated (idempotency)
            }
            let Some(value_str) = value.as_str() else {
                continue;
            };
            let secret_ref = mcp_env_ref(&server.id, name);
            match migrate_one(store, &secret_ref, value_str).await {
                Ok(()) => {
                    server.env_secret_refs.insert(name.clone(), secret_ref);
                    report.migrated_mcp_values += 1;
                }
                Err(_) => {
                    report.pending += 1;
                    report.failed += 1;
                }
            }
        }
        // Clear plaintext env for every migrated name, keep any unmigrated ones.
        let remaining: serde_json::Map<String, serde_json::Value> = server
            .env
            .as_ref()
            .and_then(|e| e.as_object())
            .map(|obj| {
                obj.iter()
                    .filter(|(name, _)| !server.env_secret_refs.contains_key(*name))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default();
        server.env = if remaining.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(remaining))
        };
    }

    report
}

/// Write a secret and verify it reads back before reporting success.
async fn migrate_one(
    store: &dyn SecretStore,
    secret_ref: &SecretRef,
    value: &str,
) -> Result<(), SecretStoreError> {
    let secret = SecretString::from(value.to_string());
    store.put(secret_ref, secret).await?;
    let read_back = store.get(secret_ref).await?;
    match read_back {
        Some(_) => Ok(()),
        None => Err(SecretStoreError::Backend),
    }
}
