// ============================================================
// SecretResolver — the single resolution boundary for runtime secrets.
//
// Resolution precedence (chat / embedding):
//   1. SecretRef → SecretStore
//   2. env-var name → process environment
//   3. legacy literal (LEGACY MIGRATION ONLY — never a normal write path)
//
// Only explicit runtime consumers (LLM, Embedding, specific MCP servers) may
// resolve. There is no arbitrary secret lookup API for tools.
// ============================================================

use secrecy::SecretString;
use std::sync::Arc;

use super::model::{SecretRef, SecretStoreError, SecretStoreStatus};
use super::store::SecretStore;
use crate::config::types::ModelConfig;

pub struct SecretResolver {
    store: Arc<dyn SecretStore>,
}

impl SecretResolver {
    pub fn new(store: Arc<dyn SecretStore>) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &Arc<dyn SecretStore> {
        &self.store
    }

    /// Resolve a SecretRef against the store (None when absent).
    pub async fn resolve_ref(
        &self,
        secret_ref: &SecretRef,
    ) -> Result<Option<SecretString>, SecretStoreError> {
        self.store.get(secret_ref).await
    }

    /// Resolve a stdio MCP env secret ref (value or None).
    pub async fn resolve_env_ref(
        &self,
        secret_ref: &SecretRef,
    ) -> Result<Option<SecretString>, SecretStoreError> {
        self.store.get(secret_ref).await
    }

    pub async fn resolve_api_key(
        &self,
        config: &ModelConfig,
    ) -> Result<Option<SecretString>, SecretStoreError> {
        if let Some(secret_ref) = &config.api_key_ref {
            return self.store.get(secret_ref).await;
        }
        if !config.api_key_env.is_empty() {
            if let Ok(value) = std::env::var(&config.api_key_env) {
                return Ok(Some(SecretString::from(value)));
            }
        }
        // LEGACY MIGRATION ONLY — a literal key survives only until migration.
        if !config.api_key.is_empty() {
            return Ok(Some(SecretString::from(config.api_key.clone())));
        }
        Ok(None)
    }

    pub async fn resolve_embedding_api_key(
        &self,
        config: &ModelConfig,
    ) -> Result<Option<SecretString>, SecretStoreError> {
        if let Some(secret_ref) = &config.embedding_api_key_ref {
            return self.store.get(secret_ref).await;
        }
        if !config.embedding_api_key_env.is_empty() {
            if let Ok(value) = std::env::var(&config.embedding_api_key_env) {
                return Ok(Some(SecretString::from(value)));
            }
        }
        // LEGACY MIGRATION ONLY.
        if !config.embedding_api_key.is_empty() {
            return Ok(Some(SecretString::from(config.embedding_api_key.clone())));
        }
        Ok(None)
    }

    pub async fn status(&self) -> SecretStoreStatus {
        self.store.status().await
    }
}
