use secrecy::ExposeSecret;
use secrecy::SecretString;
use std::sync::Arc;

use crate::db::Database;
use crate::integrations::secret::{
    cloud_access_token_ref, cloud_refresh_token_ref, scoped_cloud_access_token_ref,
    scoped_cloud_refresh_token_ref, SecretStore, SecretStoreError,
};

use super::client::CloudApiClient;
use super::domain::AuthenticatedSessionInput;
use super::domain::CloudSessionView;

#[derive(Debug, thiserror::Error)]
pub enum CloudSessionError {
    #[error("secret store operation failed")]
    Secret(#[from] SecretStoreError),
    #[error("cloud session persistence failed")]
    Database(#[from] rusqlite::Error),
    #[error("invalid cloud service URL")]
    InvalidBaseUrl,
    #[error("cloud session service is unavailable")]
    Unavailable,
    #[error("cloud session response is invalid")]
    InvalidResponse,
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

    pub async fn persist_authenticated_session_scoped(
        &self,
        session: AuthenticatedSessionInput,
        scope_id: &str,
    ) -> Result<(), CloudSessionError> {
        let access_ref = scoped_cloud_access_token_ref(scope_id);
        let refresh_ref = scoped_cloud_refresh_token_ref(scope_id);
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

    pub async fn logout_with_report(
        &self,
        lease: &crate::modules::cloud::contracts::ScopeLease,
        session_id_opt: Option<&str>,
        service_origin_opt: Option<&str>,
        account_id_opt: Option<&str>,
    ) -> crate::modules::cloud::revocation::LogoutReport {
        let revocation_service = crate::modules::cloud::revocation::RevocationService::new(
            self.db.clone(),
            Arc::clone(&self.secret_store),
        );
        revocation_service
            .logout(lease, session_id_opt, service_origin_opt, account_id_opt)
            .await
    }

    pub async fn list_remote_sessions(
        &self,
        base_url: &str,
    ) -> Result<Vec<CloudSessionView>, CloudSessionError> {
        let (client, base) = remote_client(base_url)?;
        let access_ref = self
            .db
            .cloud_secret_refs()?
            .map(|refs| refs.0)
            .unwrap_or_else(cloud_access_token_ref);
        let token = self
            .secret_store
            .get(&access_ref)
            .await?
            .ok_or(CloudSessionError::Unavailable)?;
        let response = client
            .get(format!("{base}/sessions"))
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .map_err(|_| CloudSessionError::Unavailable)?;
        if !response.status().is_success() {
            return Err(CloudSessionError::Unavailable);
        }
        #[derive(serde::Deserialize)]
        struct Envelope {
            sessions: Vec<CloudSessionView>,
        }
        Ok(response
            .json::<Envelope>()
            .await
            .map_err(|_| CloudSessionError::InvalidResponse)?
            .sessions)
    }

    pub async fn refresh(&self, base_url: &str) -> Result<(), CloudSessionError> {
        let refresh_ref = self
            .db
            .cloud_secret_refs()?
            .map(|refs| refs.1)
            .unwrap_or_else(cloud_refresh_token_ref);
        let refresh = self
            .secret_store
            .get(&refresh_ref)
            .await?
            .ok_or(CloudSessionError::Unavailable)?;
        let client =
            CloudApiClient::new(base_url).map_err(|_| CloudSessionError::InvalidBaseUrl)?;
        let session = client
            .refresh(refresh.expose_secret())
            .await
            .map_err(|_| CloudSessionError::Unavailable)?;
        let identity = self.db.get_or_create_cloud_identity()?;
        if let Some(account_id) = identity.account_id {
            let (_, scope_id) =
                crate::modules::cloud::contracts::canonical_scope(base_url, &account_id)
                    .map_err(|_| CloudSessionError::InvalidBaseUrl)?;
            self.persist_authenticated_session_scoped(session, &scope_id)
                .await
        } else {
            self.persist_authenticated_session(session).await
        }
    }

    pub async fn revoke_remote_session(
        &self,
        base_url: &str,
        session_id: &str,
    ) -> Result<(), CloudSessionError> {
        let (client, base) = remote_client(base_url)?;
        let access_ref = self
            .db
            .cloud_secret_refs()?
            .map(|refs| refs.0)
            .unwrap_or_else(cloud_access_token_ref);
        let token = self
            .secret_store
            .get(&access_ref)
            .await?
            .ok_or(CloudSessionError::Unavailable)?;
        let response = client
            .post(format!("{base}/sessions/{session_id}/revoke"))
            .header("Idempotency-Key", uuid::Uuid::new_v4().to_string())
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .map_err(|_| CloudSessionError::Unavailable)?;
        if !response.status().is_success() {
            return Err(CloudSessionError::Unavailable);
        }
        Ok(())
    }
}

fn remote_client(base_url: &str) -> Result<(reqwest::Client, String), CloudSessionError> {
    let parsed = url::Url::parse(base_url.trim()).map_err(|_| CloudSessionError::InvalidBaseUrl)?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(CloudSessionError::InvalidBaseUrl);
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|_| CloudSessionError::Unavailable)?;
    Ok((client, base_url.trim().trim_end_matches('/').to_string()))
}
