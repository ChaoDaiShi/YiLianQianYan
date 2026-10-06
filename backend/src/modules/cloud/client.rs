use reqwest::header::HeaderValue;
use serde::Deserialize;

use super::domain::{AuthenticatedSessionInput, CloudAuthRequest};

#[derive(Debug, thiserror::Error)]
pub enum CloudClientError {
    #[error("invalid cloud service URL")]
    InvalidBaseUrl,
    #[error("cloud service is unavailable")]
    Unavailable,
    #[error("cloud authentication failed")]
    Authentication,
    #[error("cloud request was rejected")]
    Rejected,
    #[error("cloud service returned an invalid response")]
    InvalidResponse,
}

#[derive(Clone)]
pub struct CloudApiClient {
    base_url: String,
    http: reqwest::Client,
}

impl CloudApiClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self, CloudClientError> {
        let raw = base_url.into();
        let parsed = url::Url::parse(raw.trim()).map_err(|_| CloudClientError::InvalidBaseUrl)?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.username() != ""
            || parsed.password().is_some()
        {
            return Err(CloudClientError::InvalidBaseUrl);
        }
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|_| CloudClientError::Unavailable)?;
        Ok(Self {
            base_url: raw.trim().trim_end_matches('/').to_string(),
            http,
        })
    }

    pub async fn login(
        &self,
        request: CloudAuthRequest,
    ) -> Result<AuthenticatedSessionInput, CloudClientError> {
        self.authenticate("identity/login", request, None).await
    }

    pub async fn register(
        &self,
        request: CloudAuthRequest,
        idempotency_key: &str,
    ) -> Result<AuthenticatedSessionInput, CloudClientError> {
        let header =
            HeaderValue::from_str(idempotency_key).map_err(|_| CloudClientError::Rejected)?;
        self.authenticate("identity/register", request, Some(header))
            .await
    }

    pub async fn refresh(
        &self,
        refresh_token: &str,
    ) -> Result<AuthenticatedSessionInput, CloudClientError> {
        if refresh_token.len() < 20 {
            return Err(CloudClientError::Authentication);
        }
        let response = self
            .http
            .post(format!("{}/identity/refresh", self.base_url))
            .json(&serde_json::json!({"refresh_token": refresh_token}))
            .send()
            .await
            .map_err(|_| CloudClientError::Unavailable)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CloudClientError::Authentication);
        }
        if !response.status().is_success() {
            return Err(CloudClientError::Rejected);
        }
        let auth: AuthEnvelope = response
            .json()
            .await
            .map_err(|_| CloudClientError::InvalidResponse)?;
        auth.into_session()
    }

    async fn authenticate(
        &self,
        path: &str,
        request: CloudAuthRequest,
        idempotency_key: Option<HeaderValue>,
    ) -> Result<AuthenticatedSessionInput, CloudClientError> {
        let body = serde_json::json!({
            "email": request.email,
            "password": request.password,
            "display_name": request.display_name,
            "device": {
                "id": request.device_id,
                "label": request.device_label,
                "client_version": request.client_version,
            }
        });
        let mut builder = self
            .http
            .post(format!("{}/{}", self.base_url, path))
            .json(&body);
        if let Some(value) = idempotency_key {
            builder = builder.header("Idempotency-Key", value);
        }
        let response = builder
            .send()
            .await
            .map_err(|_| CloudClientError::Unavailable)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(CloudClientError::Authentication);
        }
        if !response.status().is_success() {
            return Err(CloudClientError::Rejected);
        }
        let auth: AuthEnvelope = response
            .json()
            .await
            .map_err(|_| CloudClientError::InvalidResponse)?;
        auth.into_session()
    }
}

#[derive(Deserialize)]
struct AuthEnvelope {
    account: AccountEnvelope,
    session: SessionEnvelope,
    access_token: String,
    refresh_token: String,
    expires_in: i64,
}

#[derive(Deserialize)]
struct AccountEnvelope {
    id: String,
    email: String,
}

#[derive(Deserialize)]
struct SessionEnvelope {
    id: String,
}

impl AuthEnvelope {
    fn into_session(self) -> Result<AuthenticatedSessionInput, CloudClientError> {
        if self.access_token.is_empty() || self.refresh_token.is_empty() {
            return Err(CloudClientError::InvalidResponse);
        }
        Ok(AuthenticatedSessionInput {
            account_id: self.account.id,
            email: self.account.email,
            session_id: self.session.id,
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires_at: chrono::Utc::now().timestamp_millis()
                + self.expires_in.saturating_mul(1_000),
        })
    }
}
