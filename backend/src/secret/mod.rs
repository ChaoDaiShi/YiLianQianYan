// ============================================================
// Secret management — OS-backed SecretStore + SecretRef persistence +
// legacy plaintext migration + runtime resolution.
//
// A Secret value NEVER belongs in AppConfig or MCP DB persistence. The DB
// stores only SecretRefs; the runtime resolves on demand. New writes never
// fall back to plaintext.
// ============================================================

pub mod memory_store;
pub mod migration;
pub mod model;
pub mod os_store;
pub mod resolver;
pub mod store;

pub use memory_store::InMemorySecretStore;
pub use migration::migrate_legacy_secrets;
pub use model::{
    llm_model_key_ref, mcp_env_ref, SecretKind, SecretMigrationReport, SecretRef, SecretSource,
    SecretStoreError, SecretStoreStatus, CHAT_KEY_REF, EMBEDDING_KEY_REF, MAX_SECRET_KEY_LEN,
    MAX_SECRET_VALUE_BYTES, SECRET_SERVICE_NAME,
};
pub use os_store::OsSecretStore;
pub use resolver::SecretResolver;
pub use store::SecretStore;

#[cfg(test)]
mod tests;

/// Record a secret mutation audit event. Details carry only the secret kind,
/// the SecretRef key (a digest, not the value), the operation, and success —
/// never the secret value itself.
pub fn record_secret_event(
    recorder: &crate::safety::AuditRecorder,
    event_type: crate::safety::AuditEventType,
    kind: SecretKind,
    secret_ref: &SecretRef,
    operation: &str,
    success: bool,
) {
    let _ = recorder.record(crate::safety::AuditEventInput {
        event_type,
        correlation_id: secret_ref.key.clone(),
        request_id: secret_ref.key.clone(),
        subject_id: "local-user".to_string(),
        role_key: "owner".to_string(),
        details: serde_json::json!({
            "secret_kind": kind,
            "secret_ref": secret_ref.key,
            "operation": operation,
            "success": success,
        }),
        ..Default::default()
    });
}
