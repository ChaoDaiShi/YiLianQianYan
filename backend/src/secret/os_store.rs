// ============================================================
// OsSecretStore — the production OS-backed credential store.
//
//   Windows → Windows Credential Manager
//   macOS   → Keychain
//   Linux   → Secret Service
//
// keyring operations are blocking, so every put/get/delete is dispatched onto
// `tokio::task::spawn_blocking` — never executed inline on a Tokio worker.
//
// If the OS store is unavailable (e.g. headless Linux with no Secret Service),
// the store reports `Unavailable`; it never falls back to plaintext.
// ============================================================

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};

use super::model::{SecretRef, SecretStoreError, SecretStoreStatus, SECRET_SERVICE_NAME};
use super::store::SecretStore;

pub struct OsSecretStore;

impl Default for OsSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl OsSecretStore {
    pub fn new() -> Self {
        Self
    }
}

async fn blocking_put(account: String, value: String) -> Result<(), SecretStoreError> {
    tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new(SECRET_SERVICE_NAME, &account)
            .map_err(|_| SecretStoreError::Backend)?;
        entry
            .set_password(&value)
            .map_err(|_| SecretStoreError::Backend)
    })
    .await
    .map_err(|_| SecretStoreError::Backend)?
}

async fn blocking_get(account: String) -> Result<Option<String>, SecretStoreError> {
    tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new(SECRET_SERVICE_NAME, &account)
            .map_err(|_| SecretStoreError::Backend)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(SecretStoreError::Backend),
        }
    })
    .await
    .map_err(|_| SecretStoreError::Backend)?
}

async fn blocking_delete(account: String) -> Result<(), SecretStoreError> {
    tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new(SECRET_SERVICE_NAME, &account)
            .map_err(|_| SecretStoreError::Backend)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            // Deleting a missing credential is idempotent success.
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(SecretStoreError::Backend),
        }
    })
    .await
    .map_err(|_| SecretStoreError::Backend)?
}

#[async_trait]
impl SecretStore for OsSecretStore {
    async fn put(
        &self,
        secret_ref: &SecretRef,
        value: SecretString,
    ) -> Result<(), SecretStoreError> {
        secret_ref.validate()?;
        blocking_put(secret_ref.key.clone(), value.expose_secret().to_string()).await
    }

    async fn get(&self, secret_ref: &SecretRef) -> Result<Option<SecretString>, SecretStoreError> {
        secret_ref.validate()?;
        match blocking_get(secret_ref.key.clone()).await? {
            Some(value) => Ok(Some(SecretString::from(value))),
            None => Ok(None),
        }
    }

    async fn delete(&self, secret_ref: &SecretRef) -> Result<(), SecretStoreError> {
        secret_ref.validate()?;
        blocking_delete(secret_ref.key.clone()).await
    }

    async fn status(&self) -> SecretStoreStatus {
        // A successful probe (even NoEntry) proves the backend is reachable.
        match blocking_get("__yilian_status_probe__".to_string()).await {
            Ok(_) => SecretStoreStatus::Available,
            Err(SecretStoreError::Backend) => SecretStoreStatus::Unavailable,
            Err(_) => SecretStoreStatus::Unavailable,
        }
    }
}
