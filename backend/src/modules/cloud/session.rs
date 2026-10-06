use secrecy::SecretString;
use std::sync::Arc;

use crate::db::Database;
use crate::integrations::secret::{
    cloud_access_token_ref, cloud_refresh_token_ref, SecretStore, SecretStoreError,
};

use super::domain::AuthenticatedSessionInput;

#[derive(Debug, thiserror::Error)]
pub enum CloudSessionError {
    #[error("secret store operation failed")]
    Secret(#[from] SecretStoreError),
    #[error("cloud session persistence failed")]
    Database(#[from] rusqlite::Error),
}

pub struct CloudSessionService {
    db: Database,
    secret_store: Arc<dyn SecretStore>,
}

impl CloudSessionService {
    pub fn new(db: Database, secret_store: Arc<dyn SecretStore>) -> Self {
        Self { db, secret_store }
    }

    pub async fn persist_authenticated_session(
        &self,
        session: AuthenticatedSessionInput,
    ) -> Result<(), CloudSessionError> {
        let access_ref = cloud_access_token_ref();
        let refresh_ref = cloud_refresh_token_ref();
        self.secret_store
            .put(&access_ref, SecretString::from(session.access_token))
            .await?;
        if let Err(error) = self
            .secret_store
            .put(&refresh_ref, SecretString::from(session.refresh_token))
            .await
        {
            let _ = self.secret_store.delete(&access_ref).await;
            return Err(error.into());
        }
        if let Err(error) = self.db.save_authenticated_cloud_session(
            &session.account_id,
            &session.email,
            &session.session_id,
            session.expires_at,
            &access_ref,
            &refresh_ref,
        ) {
            let _ = self.secret_store.delete(&access_ref).await;
            let _ = self.secret_store.delete(&refresh_ref).await;
            return Err(error.into());
        }
        Ok(())
    }

    pub async fn logout(&self) -> Result<(), CloudSessionError> {
        self.secret_store.delete(&cloud_access_token_ref()).await?;
        self.secret_store.delete(&cloud_refresh_token_ref()).await?;
        self.db.clear_authenticated_cloud_session()?;
        Ok(())
    }
}
