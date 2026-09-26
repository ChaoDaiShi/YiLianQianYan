// ============================================================
// SecretStore trait — the minimal capability surface for secret storage.
//
// There is deliberately NO list-all / export API: a SecretStore is backend
// infrastructure, not a password-manager UI.
// ============================================================

use async_trait::async_trait;
use secrecy::SecretString;

use super::model::{SecretRef, SecretStoreError, SecretStoreStatus};

#[async_trait]
pub trait SecretStore: Send + Sync {
    async fn put(
        &self,
        secret_ref: &SecretRef,
        value: SecretString,
    ) -> Result<(), SecretStoreError>;

    async fn get(&self, secret_ref: &SecretRef) -> Result<Option<SecretString>, SecretStoreError>;

    async fn delete(&self, secret_ref: &SecretRef) -> Result<(), SecretStoreError>;

    async fn status(&self) -> SecretStoreStatus;
}
