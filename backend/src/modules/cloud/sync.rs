use secrecy::ExposeSecret;
use serde::Deserialize;
use std::sync::Arc;

use crate::db::Database;
use crate::integrations::secret::{cloud_access_token_ref, SecretStore};

use super::domain::SyncOperation;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SyncReport {
    pub pushed: usize,
    pub pulled: usize,
    pub conflicts: usize,
    pub cursor: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CloudSyncError {
    #[error("invalid cloud service URL")]
    InvalidBaseUrl,
    #[error("cloud authentication is required")]
    AuthenticationRequired,
    #[error("cloud sync transport failed")]
    Transport,
    #[error("cloud sync response was invalid")]
    InvalidResponse,
    #[error("local sync persistence failed")]
    Database(#[from] rusqlite::Error),
}

pub struct CloudSyncService {
    db: Database,
    secret_store: Arc<dyn SecretStore>,
    base_url: String,
    http: reqwest::Client,
}

impl CloudSyncService {
    pub fn new(
        db: Database,
        secret_store: Arc<dyn SecretStore>,
        base_url: impl Into<String>,
    ) -> Result<Self, CloudSyncError> {
        let raw = base_url.into();
        let url = url::Url::parse(raw.trim()).map_err(|_| CloudSyncError::InvalidBaseUrl)?;
        if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() || url.password().is_some() {
            return Err(CloudSyncError::InvalidBaseUrl);
        }
        Ok(Self {
            db,
            secret_store,
            base_url: raw.trim().trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|_| CloudSyncError::Transport)?,
        })
    }

    pub async fn sync_now(&self) -> Result<SyncReport, CloudSyncError> {
        let token = self
            .secret_store
            .get(&cloud_access_token_ref())
            .await
            .map_err(|_| CloudSyncError::AuthenticationRequired)?
            .ok_or(CloudSyncError::AuthenticationRequired)?;
        let identity = self.db.get_or_create_cloud_identity()?;
        let pending = self.db.pending_sync_changes(100)?;
        let mut pushed = 0;
        let mut conflicts = 0;
        let mut cursor = self.db.cloud_sync_status()?.cursor.unwrap_or_else(|| "0".to_string());

        if !pending.is_empty() {
            let response = self
                .http
                .post(format!("{}/sync/push", self.base_url))
                .bearer_auth(token.expose_secret())
                .header("Idempotency-Key", uuid::Uuid::new_v4().to_string())
                .json(&serde_json::json!({"device_id":identity.device_id,"changes":pending}))
                .send()
                .await
                .map_err(|_| CloudSyncError::Transport)?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                return Err(CloudSyncError::AuthenticationRequired);
            }
            if !response.status().is_success() {
                return Err(CloudSyncError::Transport);
            }
            let push: PushResponse = response.json().await.map_err(|_| CloudSyncError::InvalidResponse)?;
            cursor = push.cursor;
            for result in push.results {
                let Some(item) = pending.iter().find(|item| item.change_id == result.change_id) else {
                    continue;
                };
                match result.status.as_str() {
                    "applied" | "duplicate" => {
                        self.db.remove_sync_change(&result.change_id)?;
                        pushed += 1;
                    }
                    "conflict" => {
                        self.db.record_sync_conflict(item, result.authoritative_revision.unwrap_or(item.revision))?;
                        conflicts += 1;
                    }
                    _ => {
                        self.db.mark_sync_change_failed(
                            &result.change_id,
                            result.error_code.as_deref().unwrap_or("SYNC_REJECTED"),
                            chrono::Utc::now().timestamp_millis() + 30_000,
                        )?;
                    }
                }
            }
        }

        let mut pulled = 0;
        loop {
            let response = self
                .http
                .get(format!("{}/sync/pull", self.base_url))
                .bearer_auth(token.expose_secret())
                .query(&[("cursor", cursor.as_str()), ("limit", "100")])
                .send()
                .await
                .map_err(|_| CloudSyncError::Transport)?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                return Err(CloudSyncError::AuthenticationRequired);
            }
            if !response.status().is_success() {
                return Err(CloudSyncError::Transport);
            }
            let page: PullResponse = response.json().await.map_err(|_| CloudSyncError::InvalidResponse)?;
            for change in &page.changes {
                let operation = SyncOperation::parse(&change.operation).ok_or(CloudSyncError::InvalidResponse)?;
                self.db.apply_remote_sync_change(
                    &change.entity_type,
                    &change.entity_id,
                    change.revision,
                    operation,
                    change.payload.as_ref(),
                    change.sequence,
                )?;
                pulled += 1;
            }
            cursor = page.next_cursor;
            if !page.has_more {
                break;
            }
        }
        self.db.save_sync_success(&cursor)?;
        Ok(SyncReport { pushed, pulled, conflicts, cursor })
    }
}

#[derive(Deserialize)]
struct PushResponse {
    results: Vec<PushResult>,
    cursor: String,
}

#[derive(Deserialize)]
struct PushResult {
    change_id: String,
    status: String,
    authoritative_revision: Option<i64>,
    error_code: Option<String>,
}

#[derive(Deserialize)]
struct PullResponse {
    changes: Vec<RemoteChange>,
    next_cursor: String,
    has_more: bool,
}

#[derive(Deserialize)]
struct RemoteChange {
    entity_type: String,
    entity_id: String,
    revision: i64,
    operation: String,
    payload: Option<serde_json::Value>,
    sequence: i64,
}
