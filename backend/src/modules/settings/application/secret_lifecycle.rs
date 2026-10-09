//! Secret lifecycle — write-only API-key handling.
//!
//! rc.2 semantics preserved verbatim:
//!   missing key       -> KEEP
//!   empty key         -> KEEP
//!   non-empty new key -> REPLACE
//!   explicit clear    -> DELETE

use axum::Json;

use crate::app::state::AppServer;
use crate::config::types::AppConfig;
use crate::integrations::secret::{
    record_secret_event, SecretKind, SecretRef, SecretResolver, SecretSource,
};
use crate::safety::AuditEventType;

pub(crate) async fn delete_secret_or_fail(
    server: &AppServer,
    secret_ref: &SecretRef,
    kind: SecretKind,
) -> Result<(), Json<serde_json::Value>> {
    match server.secret_store.delete(secret_ref).await {
        Ok(()) => {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretDeleted,
                kind,
                secret_ref,
                "clear",
                true,
            );
            Ok(())
        }
        Err(_) => {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretDeleted,
                kind,
                secret_ref,
                "clear",
                false,
            );
            Err(Json(serde_json::json!({
                "error": "系统安全凭据库删除失败，密钥未清除"
            })))
        }
    }
}

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
