use secrecy::{ExposeSecret, SecretString};
use std::sync::Arc;

use crate::{
    db::Database,
    integrations::secret::{cloud_access_token_ref, deletion_status_credential_ref, SecretStore},
};

use super::domain::{CloudDataSummary, LifecycleJob};

#[derive(Debug, thiserror::Error)]
pub enum CloudLifecycleError {
    #[error("invalid cloud service URL")]
    InvalidBaseUrl,
    #[error("remote cloud service requires HTTPS")]
    HttpsRequired,
    #[error("cloud authentication is required")]
    AuthenticationRequired,
    #[error("cloud lifecycle service is unavailable")]
    Unavailable,
    #[error("cloud lifecycle response is invalid")]
    InvalidResponse,
    #[error("export not found")]
    NotFound,
    #[error("export has expired")]
    Expired,
    #[error("local lifecycle persistence failed")]
    Database(#[from] rusqlite::Error),
}

impl From<super::transport::CloudTransportError> for CloudLifecycleError {
    fn from(err: super::transport::CloudTransportError) -> Self {
        match err {
            super::transport::CloudTransportError::InvalidUrl => {
                CloudLifecycleError::InvalidBaseUrl
            }
            super::transport::CloudTransportError::HttpsRequired => {
                CloudLifecycleError::HttpsRequired
            }
            super::transport::CloudTransportError::ClientUnavailable => {
                CloudLifecycleError::Unavailable
            }
        }
    }
}

pub struct CloudLifecycleService {
    db: Database,
    secret_store: Arc<dyn SecretStore>,
    base_url: String,
    http: reqwest::Client,
}

impl CloudLifecycleService {
    pub fn new(
        db: Database,
        secret_store: Arc<dyn SecretStore>,
        base_url: impl Into<String>,
    ) -> Result<Self, CloudLifecycleError> {
        let raw = base_url.into();
        let (http, base_url) =
            super::transport::build_cloud_http(&raw, std::time::Duration::from_secs(30))?;
        Ok(Self {
            db,
            secret_store,
            base_url,
            http,
        })
    }

    pub async fn data_summary(&self) -> Result<CloudDataSummary, CloudLifecycleError> {
        self.get_json("user-data/summary").await
    }

    pub async fn request_export(&self) -> Result<LifecycleJob, CloudLifecycleError> {
        let job = self
            .post_json("exports", Some(serde_json::json!({})))
            .await?;
        self.db.save_cloud_lifecycle_job(&job)?;
        Ok(job)
    }

    pub async fn request_account_deletion(&self) -> Result<LifecycleJob, CloudLifecycleError> {
        let job: LifecycleJob = self
            .post_json(
                "account/deletion",
                Some(serde_json::json!({"confirmation":"DELETE"})),
            )
            .await?;

        // If server issued a dedicated deletion status credential, persist to SecretStore
        if let Some(ref raw_credential) = job.status_credential {
            let cred_ref = deletion_status_credential_ref(&job.id);
            let _ = self
                .secret_store
                .put(&cred_ref, SecretString::from(raw_credential.clone()))
                .await;
        }

        self.db.save_cloud_lifecycle_job(&job)?;
        Ok(job)
    }

    pub async fn download_export(
        &self,
        id: &str,
    ) -> Result<(String, Vec<u8>), CloudLifecycleError> {
        let token = self.access_token().await?;
        let response = self
            .http
            .get(format!("{}/exports/{id}/download", self.base_url))
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .map_err(|_| CloudLifecycleError::Unavailable)?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CloudLifecycleError::AuthenticationRequired);
        }
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(CloudLifecycleError::NotFound);
        }
        if response.status() == reqwest::StatusCode::GONE {
            return Err(CloudLifecycleError::Expired);
        }
        if !response.status().is_success() {
            return Err(CloudLifecycleError::Unavailable);
        }

        let filename = response
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .and_then(|h| h.to_str().ok())
            .and_then(|val| {
                val.split("filename=")
                    .nth(1)
                    .map(|s| s.trim_matches('"').trim().to_string())
            })
            .unwrap_or_else(|| format!("yilian-export-{id}.json"));

        let bytes = response
            .bytes()
            .await
            .map_err(|_| CloudLifecycleError::InvalidResponse)?
            .to_vec();

        Ok((filename, bytes))
    }

    pub async fn job(&self, kind: &str, id: &str) -> Result<LifecycleJob, CloudLifecycleError> {
        let job = match kind {
            "export" => self.get_json(&format!("exports/{id}")).await?,
            "deletion" => {
                // If we have a stored deletion status credential in SecretStore, use it
                let cred_ref = deletion_status_credential_ref(id);
                if let Ok(Some(secret)) = self.secret_store.get(&cred_ref).await {
                    match self
                        .get_json_with_custom_auth(
                            &format!("account/deletion/{id}"),
                            &format!("DeletionCredential {}", secret.expose_secret()),
                        )
                        .await
                    {
                        Ok(job) => job,
                        Err(_) => self.get_json(&format!("account/deletion/{id}")).await?,
                    }
                } else {
                    self.get_json(&format!("account/deletion/{id}")).await?
                }
            }
            _ => return Err(CloudLifecycleError::InvalidResponse),
        };

        self.db.save_cloud_lifecycle_job(&job)?;
        Ok(job)
    }

    async fn access_token(&self) -> Result<secrecy::SecretString, CloudLifecycleError> {
        let access_ref = self
            .db
            .cloud_secret_refs()?
            .map(|refs| refs.0)
            .unwrap_or_else(cloud_access_token_ref);
        self.secret_store
            .get(&access_ref)
            .await
            .map_err(|_| CloudLifecycleError::AuthenticationRequired)?
            .ok_or(CloudLifecycleError::AuthenticationRequired)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<T, CloudLifecycleError> {
        let token = self.access_token().await?;
        let response = self
            .http
            .get(format!("{}/{}", self.base_url, path))
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .map_err(|_| CloudLifecycleError::Unavailable)?;
        parse_response(response).await
    }

    async fn get_json_with_custom_auth<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        auth_header: &str,
    ) -> Result<T, CloudLifecycleError> {
        let response = self
            .http
            .get(format!("{}/{}", self.base_url, path))
            .header(reqwest::header::AUTHORIZATION, auth_header)
            .send()
            .await
            .map_err(|_| CloudLifecycleError::Unavailable)?;
        parse_response(response).await
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T, CloudLifecycleError> {
        let token = self.access_token().await?;
        let mut request = self
            .http
            .post(format!("{}/{}", self.base_url, path))
            .bearer_auth(token.expose_secret())
            .header("Idempotency-Key", uuid::Uuid::new_v4().to_string());
        if let Some(body) = body {
            request = request.json(&body)
        }
        let response = request
            .send()
            .await
            .map_err(|_| CloudLifecycleError::Unavailable)?;
        parse_response(response).await
    }
}

async fn parse_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, CloudLifecycleError> {
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(CloudLifecycleError::AuthenticationRequired);
    }
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CloudLifecycleError::NotFound);
    }
    if response.status() == reqwest::StatusCode::GONE {
        return Err(CloudLifecycleError::Expired);
    }
    if !response.status().is_success() {
        return Err(CloudLifecycleError::Unavailable);
    }
    response
        .json()
        .await
        .map_err(|_| CloudLifecycleError::InvalidResponse)
}
