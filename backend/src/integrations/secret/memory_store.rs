// ============================================================
// InMemorySecretStore — test / CI store (never used in production).
//
// Holds SecretString values in memory only. No plaintext file fallback.
// ============================================================

use async_trait::async_trait;
use parking_lot::Mutex;
use secrecy::SecretString;
use std::collections::HashMap;

use super::model::{SecretRef, SecretStoreError, SecretStoreStatus};
use super::store::SecretStore;

pub struct InMemorySecretStore {
    inner: Mutex<HashMap<String, SecretString>>,
}

impl Default for InMemorySecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemorySecretStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl SecretStore for InMemorySecretStore {
    async fn put(
        &self,
        secret_ref: &SecretRef,
        value: SecretString,
    ) -> Result<(), SecretStoreError> {
        secret_ref.validate()?;
        self.inner.lock().insert(secret_ref.key.clone(), value);
        Ok(())
    }

    async fn get(&self, secret_ref: &SecretRef) -> Result<Option<SecretString>, SecretStoreError> {
        secret_ref.validate()?;
        Ok(self.inner.lock().get(&secret_ref.key).cloned())
    }

    async fn delete(&self, secret_ref: &SecretRef) -> Result<(), SecretStoreError> {
        secret_ref.validate()?;
        self.inner.lock().remove(&secret_ref.key);
        Ok(())
    }

    async fn status(&self) -> SecretStoreStatus {
        SecretStoreStatus::Available
    }
}
