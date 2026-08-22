// ============================================================
// Settings API handlers — write-only Secret UX + safe config.
//
// API keys never round-trip as plaintext: GET returns "" for the literal and a
// computed `configured`/`source` pair; PUT writes new keys into the SecretStore
// behind a stable SecretRef (or clears them) and persists only the ref.
// ============================================================

use axum::{extract::State, Json};
use secrecy::SecretString;
use std::sync::Arc;

use crate::config::types::AppConfig;
use crate::safety::AuditEventType;
use crate::secret::{
    record_secret_event, SecretKind, SecretRef, SecretResolver, SecretSource, CHAT_KEY_REF,
    EMBEDDING_KEY_REF,
};
use crate::server::AppServer;

pub(crate) async fn chat_source(
    resolver: &SecretResolver,
    config: &AppConfig,
) -> (SecretSource, bool) {
    if let Some(secret_ref) = &config.model.api_key_ref {
        let ok = matches!(resolver.resolve_ref(secret_ref).await, Ok(Some(_)));
        return (SecretSource::SecretStore, ok);
    }
    if !config.model.api_key_env.is_empty() {
        let ok = std::env::var(&config.model.api_key_env).is_ok();
        return (SecretSource::Environment, ok);
    }
    if !config.model.api_key.is_empty() {
        return (SecretSource::LegacyPending, true);
    }
    (SecretSource::None, false)
}

pub(crate) async fn embedding_source(
    resolver: &SecretResolver,
    config: &AppConfig,
) -> (SecretSource, bool) {
    if let Some(secret_ref) = &config.model.embedding_api_key_ref {
        let ok = matches!(resolver.resolve_ref(secret_ref).await, Ok(Some(_)));
        return (SecretSource::SecretStore, ok);
    }
    if !config.model.embedding_api_key_env.is_empty() {
        let ok = std::env::var(&config.model.embedding_api_key_env).is_ok();
        return (SecretSource::Environment, ok);
    }
    if !config.model.embedding_api_key.is_empty() {
        return (SecretSource::LegacyPending, true);
    }
    (SecretSource::None, false)
}

async fn build_redacted_config(server: &AppServer) -> serde_json::Value {
    let config = server.config.read().clone();
    let mut value = serde_json::to_value(&config).unwrap_or_else(|_| serde_json::json!({}));

    let (chat_src, chat_configured) = chat_source(&server.secret_resolver, &config).await;
    let (embed_src, embed_configured) = embedding_source(&server.secret_resolver, &config).await;

    if let Some(model) = value
        .get_mut("model")
        .and_then(serde_json::Value::as_object_mut)
    {
        model.insert(
            "api_key".to_string(),
            serde_json::Value::String(String::new()),
        );
        model.insert(
            "embedding_api_key".to_string(),
            serde_json::Value::String(String::new()),
        );
        model.insert(
            "api_key_configured".to_string(),
            serde_json::Value::Bool(chat_configured),
        );
        model.insert(
            "embedding_api_key_configured".to_string(),
            serde_json::Value::Bool(embed_configured),
        );
        model.insert(
            "api_key_source".to_string(),
            serde_json::to_value(chat_src).unwrap(),
        );
        model.insert(
            "embedding_api_key_source".to_string(),
            serde_json::to_value(embed_src).unwrap(),
        );
    }

    let migration_pending = {
        let mut pending = 0usize;
        if !config.model.api_key.is_empty() {
            pending += 1;
        }
        if !config.model.embedding_api_key.is_empty() {
            pending += 1;
        }
        pending += server
            .db
            .list_mcp_servers()
            .unwrap_or_default()
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
        pending
    };
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "secret_store_status".to_string(),
            serde_json::to_value(server.secret_resolver.status().await).unwrap(),
        );
        obj.insert(
            "migration_pending".to_string(),
            serde_json::Value::from(migration_pending as u64),
        );
    }

    value
}

pub async fn get_handler(State(server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    Json(build_redacted_config(&server).await)
}

pub async fn update_handler(
    State(server): State<Arc<AppServer>>,
    Json(mut incoming): Json<AppConfig>,
) -> Json<serde_json::Value> {
    let existing = server.config.read().clone();

    // ── Chat API key: clear → delete; non-empty → rotate; empty → preserve. ──
    if incoming.model.clear_api_key {
        if let Some(secret_ref) = existing.model.api_key_ref.clone() {
            let ok = server.secret_store.delete(&secret_ref).await.is_ok();
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretDeleted,
                SecretKind::ChatApiKey,
                &secret_ref,
                "clear",
                ok,
            );
        }
        incoming.model.api_key_ref = None;
        incoming.model.api_key = String::new();
    } else if !incoming.model.api_key.is_empty() {
        let secret_ref = existing
            .model
            .api_key_ref
            .clone()
            .unwrap_or_else(|| SecretRef::new(CHAT_KEY_REF));
        if let Err(e) = server
            .secret_store
            .put(
                &secret_ref,
                SecretString::from(incoming.model.api_key.clone()),
            )
            .await
        {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretStoreUnavailable,
                SecretKind::ChatApiKey,
                &secret_ref,
                "rotate",
                false,
            );
            return Json(
                serde_json::json!({ "error": format!("系统安全凭据库不可用，API Key 未保存: {e}") }),
            );
        }
        record_secret_event(
            &server.audit_recorder,
            AuditEventType::SecretRotated,
            SecretKind::ChatApiKey,
            &secret_ref,
            "rotate",
            true,
        );
        incoming.model.api_key_ref = Some(secret_ref);
        incoming.model.api_key = String::new();
    } else {
        incoming.model.api_key_ref = existing.model.api_key_ref.clone();
        incoming.model.api_key = String::new();
    }
    incoming.model.clear_api_key = false;

    // ── Embedding API key ──
    if incoming.model.clear_embedding_api_key {
        if let Some(secret_ref) = existing.model.embedding_api_key_ref.clone() {
            let ok = server.secret_store.delete(&secret_ref).await.is_ok();
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretDeleted,
                SecretKind::EmbeddingApiKey,
                &secret_ref,
                "clear",
                ok,
            );
        }
        incoming.model.embedding_api_key_ref = None;
        incoming.model.embedding_api_key = String::new();
    } else if !incoming.model.embedding_api_key.is_empty() {
        let secret_ref = existing
            .model
            .embedding_api_key_ref
            .clone()
            .unwrap_or_else(|| SecretRef::new(EMBEDDING_KEY_REF));
        if let Err(e) = server
            .secret_store
            .put(
                &secret_ref,
                SecretString::from(incoming.model.embedding_api_key.clone()),
            )
            .await
        {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretStoreUnavailable,
                SecretKind::EmbeddingApiKey,
                &secret_ref,
                "rotate",
                false,
            );
            return Json(
                serde_json::json!({ "error": format!("系统安全凭据库不可用，Embedding API Key 未保存: {e}") }),
            );
        }
        record_secret_event(
            &server.audit_recorder,
            AuditEventType::SecretRotated,
            SecretKind::EmbeddingApiKey,
            &secret_ref,
            "rotate",
            true,
        );
        incoming.model.embedding_api_key_ref = Some(secret_ref);
        incoming.model.embedding_api_key = String::new();
    } else {
        incoming.model.embedding_api_key_ref = existing.model.embedding_api_key_ref.clone();
        incoming.model.embedding_api_key = String::new();
    }
    incoming.model.clear_embedding_api_key = false;

    match server.db.save_settings(&incoming) {
        Ok(_) => {
            *server.config.write() = incoming;
            Json(serde_json::json!({"status": "saved"}))
        }
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::ControlSession;
    use crate::secret::InMemorySecretStore;

    #[tokio::test]
    async fn settings_get_never_returns_secret_value() {
        let path = std::env::temp_dir().join(format!(
            "yilian-settings-redact-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store: Arc<dyn crate::secret::SecretStore> = Arc::new(InMemorySecretStore::new());
        let server = AppServer::new_with_control_session_and_store(
            &path,
            ".",
            ControlSession::generate(),
            Arc::clone(&store),
        )
        .unwrap();
        store
            .put(
                &SecretRef::new(CHAT_KEY_REF),
                SecretString::from("SUPER_SECRET".to_string()),
            )
            .await
            .unwrap();
        {
            let mut cfg = server.config.write();
            cfg.model.api_key_ref = Some(SecretRef::new(CHAT_KEY_REF));
        }

        let json = build_redacted_config(&server).await;
        let text = json.to_string();
        assert!(!text.contains("SUPER_SECRET"));
        assert_eq!(json["model"]["api_key"], "");
        assert_eq!(json["model"]["api_key_configured"], true);
        assert_eq!(json["model"]["api_key_source"], "secret_store");

        drop(server);
        let _ = std::fs::remove_file(&path);
    }
}
