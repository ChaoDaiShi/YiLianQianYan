// ============================================================
// SecretStore + legacy migration tests.
// ============================================================

use std::sync::Arc;

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};

use super::*;
use crate::config::types::AppConfig;
use crate::db::{Database, McpServer};
use crate::safety::ControlSession;
use crate::server::AppServer;

fn temp_db(label: &str) -> (std::path::PathBuf, Database) {
    let path =
        std::env::temp_dir().join(format!("yilian-secret-{label}-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    (path, db)
}

fn test_store() -> Arc<dyn SecretStore> {
    Arc::new(InMemorySecretStore::new())
}

// ── Store that always fails (simulates an unavailable OS store) ──

struct FailingSecretStore;

#[async_trait]
impl SecretStore for FailingSecretStore {
    async fn put(
        &self,
        _secret_ref: &SecretRef,
        _value: SecretString,
    ) -> Result<(), SecretStoreError> {
        Err(SecretStoreError::Unavailable)
    }
    async fn get(&self, _secret_ref: &SecretRef) -> Result<Option<SecretString>, SecretStoreError> {
        Err(SecretStoreError::Unavailable)
    }
    async fn delete(&self, _secret_ref: &SecretRef) -> Result<(), SecretStoreError> {
        Err(SecretStoreError::Unavailable)
    }
    async fn status(&self) -> SecretStoreStatus {
        SecretStoreStatus::Unavailable
    }
}

// ── SecretRef + store basics ──

#[test]
fn secret_ref_validation() {
    assert!(SecretRef::new("model.chat.api_key").validate().is_ok());
    assert!(SecretRef::new("").validate().is_err());
    assert!(SecretRef::new("has\nnewline").validate().is_err());
    assert!(SecretRef::new("has\rreturn").validate().is_err());
    let too_long = "a".repeat(MAX_SECRET_KEY_LEN + 1);
    assert!(SecretRef::new(too_long).validate().is_err());
}

#[tokio::test]
async fn in_memory_store_roundtrip() {
    let store = test_store();
    let secret_ref = SecretRef::new("model.chat.api_key");
    store
        .put(&secret_ref, SecretString::from("abc123".to_string()))
        .await
        .unwrap();
    let got = store.get(&secret_ref).await.unwrap().unwrap();
    assert_eq!(got.expose_secret(), "abc123");
    store.delete(&secret_ref).await.unwrap();
    assert!(store.get(&secret_ref).await.unwrap().is_none());
}

#[test]
fn secret_debug_redacted() {
    let secret = SecretString::from("TOP_SECRET".to_string());
    let debug = format!("{:?}", secret);
    assert!(!debug.contains("TOP_SECRET"));
    // SecretStoreError never carries a value.
    let err = SecretStoreError::Unavailable;
    assert!(!format!("{:?}", err).contains("TOP_SECRET"));
}

// ── Resolver precedence + env fallback ──

#[tokio::test]
async fn secret_store_precedes_env() {
    let store = test_store();
    let resolver = SecretResolver::new(Arc::clone(&store));
    let secret_ref = SecretRef::new(CHAT_KEY_REF);
    store
        .put(&secret_ref, SecretString::from("A".to_string()))
        .await
        .unwrap();
    std::env::set_var("YILIAN_TEST_PRECEDENCE", "B");
    let mut config = AppConfig::default();
    config.model.api_key_ref = Some(secret_ref.clone());
    config.model.api_key_env = "YILIAN_TEST_PRECEDENCE".to_string();

    let resolved = resolver
        .resolve_api_key(&config.model)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resolved.expose_secret(), "A");
    std::env::remove_var("YILIAN_TEST_PRECEDENCE");
}

#[tokio::test]
async fn env_fallback_works() {
    let store = test_store();
    let resolver = SecretResolver::new(store);
    std::env::set_var("YILIAN_TEST_ENV_FALLBACK", "FROM_ENV");
    let mut config = AppConfig::default();
    config.model.api_key_ref = None;
    config.model.api_key_env = "YILIAN_TEST_ENV_FALLBACK".to_string();

    let resolved = resolver
        .resolve_api_key(&config.model)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resolved.expose_secret(), "FROM_ENV");
    std::env::remove_var("YILIAN_TEST_ENV_FALLBACK");
}

#[tokio::test]
async fn rotation_uses_new_value() {
    let store = test_store();
    let secret_ref = SecretRef::new("model.chat.api_key");
    store
        .put(&secret_ref, SecretString::from("old".to_string()))
        .await
        .unwrap();
    store
        .put(&secret_ref, SecretString::from("new".to_string()))
        .await
        .unwrap();
    let got = store.get(&secret_ref).await.unwrap().unwrap();
    assert_eq!(got.expose_secret(), "new");
}

#[tokio::test]
async fn delete_removes_secret() {
    let store = test_store();
    let secret_ref = SecretRef::new("model.chat.api_key");
    store
        .put(&secret_ref, SecretString::from("value".to_string()))
        .await
        .unwrap();
    store.delete(&secret_ref).await.unwrap();
    assert!(store.get(&secret_ref).await.unwrap().is_none());
}

// ── Legacy migration (pure function) ──

#[tokio::test]
async fn legacy_model_secret_migration() {
    let store = test_store();
    let mut config = AppConfig::default();
    config.model.api_key = "SUPER_SECRET_CHAT".to_string();
    config.model.embedding_api_key = "SUPER_SECRET_EMBED".to_string();

    let report = migrate_legacy_secrets(store.as_ref(), &mut config, &mut []).await;
    assert!(report.migrated_chat_key);
    assert!(report.migrated_embedding_key);
    assert!(config.model.api_key.is_empty());
    assert!(config.model.embedding_api_key.is_empty());
    assert!(config.model.api_key_ref.is_some());
    assert!(config.model.embedding_api_key_ref.is_some());

    let chat = store
        .get(&SecretRef::new(CHAT_KEY_REF))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(chat.expose_secret(), "SUPER_SECRET_CHAT");
    let embed = store
        .get(&SecretRef::new(EMBEDDING_KEY_REF))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(embed.expose_secret(), "SUPER_SECRET_EMBED");
}

#[tokio::test]
async fn legacy_stdio_mcp_env_migration() {
    let store = test_store();
    let mut server = McpServer {
        id: "mcp-1".to_string(),
        name: "fs".to_string(),
        transport: "stdio".to_string(),
        command: Some("x".to_string()),
        args: None,
        url: None,
        env: Some(serde_json::json!({"API_KEY": "SUPER_SECRET_MCP", "MODE": "production"})),
        env_secret_refs: Default::default(),
        enabled: true,
        created_at: 0,
        updated_at: 0,
    };

    let report = migrate_legacy_secrets(
        store.as_ref(),
        &mut AppConfig::default(),
        std::slice::from_mut(&mut server),
    )
    .await;
    assert_eq!(report.migrated_mcp_values, 2);
    assert_eq!(server.env_secret_refs.len(), 2);
    assert!(server.env.is_none());
    assert!(!serde_json::to_string(&server)
        .unwrap()
        .contains("SUPER_SECRET_MCP"));
}

#[tokio::test]
async fn migration_failure_preserves_plaintext() {
    let failing = FailingSecretStore;
    let mut config = AppConfig::default();
    config.model.api_key = "SUPER_SECRET_CHAT".to_string();

    let report = migrate_legacy_secrets(&failing, &mut config, &mut []).await;
    assert_eq!(report.failed, 1);
    assert_eq!(report.pending, 1);
    // Plaintext preserved — data never lost.
    assert_eq!(config.model.api_key, "SUPER_SECRET_CHAT");
    assert!(config.model.api_key_ref.is_none());
}

#[tokio::test]
async fn migration_idempotent() {
    let store = test_store();
    let mut config = AppConfig::default();
    config.model.api_key = "SUPER_SECRET_CHAT".to_string();

    let r1 = migrate_legacy_secrets(store.as_ref(), &mut config, &mut []).await;
    assert!(r1.migrated_chat_key);

    // Second run: no legacy literal remains → nothing new migrated.
    let r2 = migrate_legacy_secrets(store.as_ref(), &mut config, &mut []).await;
    assert!(!r2.migrated_chat_key);
    assert_eq!(r2.migrated_mcp_values, 0);

    // Value still resolves correctly.
    let got = store
        .get(&SecretRef::new(CHAT_KEY_REF))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.expose_secret(), "SUPER_SECRET_CHAT");
}

// ── Raw SQLite HARD gate: no plaintext after successful migration ──

#[tokio::test]
async fn raw_sqlite_legacy_secrets_migrated_and_plaintext_removed() {
    let (path, db) = temp_db("raw-migrate");

    // Simulate an OLD database: insert plaintext directly, bypassing the guard.
    {
        let conn = db.conn();
        let legacy_config = serde_json::json!({
            "model": {
                "api_key": "SUPER_SECRET_CHAT_123",
                "embedding_api_key": "SUPER_SECRET_EMBED_456"
            }
        });
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('app_config', ?1)",
            rusqlite::params![legacy_config.to_string()],
        )
        .unwrap();
        let env_json = serde_json::json!({"MCP_KEY": "SUPER_SECRET_MCP_789"});
        conn.execute(
            "INSERT INTO mcp_servers (id, name, transport, command, args, url, env, env_secret_refs, enabled, created_at, updated_at) VALUES (?1, ?2, 'stdio', 'x', NULL, NULL, ?3, NULL, 1, 0, 0)",
            rusqlite::params!["mcp-1", "fs", env_json.to_string()],
        )
        .unwrap();
    }
    drop(db);

    let store = test_store();
    let server = AppServer::new_with_control_session_and_store(
        &path,
        ".",
        ControlSession::generate(),
        store.clone(),
    )
    .unwrap();
    let report = server.migrate_secrets().await;
    assert!(report.migrated_chat_key);
    assert!(report.migrated_embedding_key);
    assert_eq!(report.migrated_mcp_values, 1);

    // Raw SQLite scan — plaintext MUST be gone.
    let conn = server.db.conn();
    let settings_value: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'app_config'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!settings_value.contains("SUPER_SECRET_CHAT_123"));
    assert!(!settings_value.contains("SUPER_SECRET_EMBED_456"));

    let mcp_env: Option<String> = conn
        .query_row("SELECT env FROM mcp_servers WHERE id = 'mcp-1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(!mcp_env.unwrap_or_default().contains("SUPER_SECRET_MCP_789"));

    let refs: Option<String> = conn
        .query_row(
            "SELECT env_secret_refs FROM mcp_servers WHERE id = 'mcp-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(refs.is_some());
    drop(conn);

    // Values are recoverable from the store (not the DB).
    let chat = store
        .get(&SecretRef::new(CHAT_KEY_REF))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(chat.expose_secret(), "SUPER_SECRET_CHAT_123");
    let embed = store
        .get(&SecretRef::new(EMBEDDING_KEY_REF))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(embed.expose_secret(), "SUPER_SECRET_EMBED_456");

    drop(server);
    let _ = std::fs::remove_file(&path);
}

// ── New writes never persist plaintext (persistence guards) ──

#[test]
fn save_settings_refuses_plaintext_api_keys() {
    let (path, db) = temp_db("settings-guard");
    let mut config = AppConfig::default();
    config.model.api_key = "NEW_CHAT_SECRET".to_string();
    assert!(db.save_settings(&config).is_err());
    drop(db);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn create_mcp_refuses_stdio_plaintext_env() {
    let (path, db) = temp_db("mcp-guard");
    let server = McpServer {
        id: "mcp-1".to_string(),
        name: "fs".to_string(),
        transport: "stdio".to_string(),
        command: Some("x".to_string()),
        args: None,
        url: None,
        env: Some(serde_json::json!({"NEW_MCP_SECRET": "value"})),
        env_secret_refs: Default::default(),
        enabled: true,
        created_at: 0,
        updated_at: 0,
    };
    assert!(db.create_mcp_server(&server).is_err());
    drop(db);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn new_settings_secret_never_persisted() {
    // A config with only SecretRefs serializes without any literal key.
    let mut config = AppConfig::default();
    config.model.api_key = "NEW_CHAT_SECRET".to_string(); // legacy literal (never persisted)
    config.model.api_key_ref = Some(SecretRef::new(CHAT_KEY_REF));
    let json = serde_json::to_string(&config).unwrap();
    assert!(!json.contains("NEW_CHAT_SECRET"));
    assert!(json.contains("api_key_ref"));
}
