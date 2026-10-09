use secrecy::ExposeSecret;
use std::sync::Arc;

use crate::db::Database;
use crate::integrations::secret::{cloud_access_token_ref, SecretStore};

use super::contracts::{
    canonical_scope, PullPage, PushAck, ScopeError, ScopeLease, ScopedDb, SyncError,
    SYNC_EPOCH_HEADER, SYNC_PROTOCOL_HEADER,
};

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
    #[error("local sync persistence failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("sync error: {0}")]
    Sync(#[from] SyncError),
    #[error("scope error: {0}")]
    Scope(#[from] ScopeError),
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
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
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
        let identity = self.db.get_or_create_cloud_identity()?;
        let account_id = identity.account_id.as_deref().unwrap_or("default");
        let (_scope_key, scope_id) =
            canonical_scope(&self.base_url, account_id).map_err(CloudSyncError::Scope)?;
        let lease = ScopeLease {
            scope_id,
            generation: "default".to_string(),
        };
        let scoped_db = ScopedDb::new(self.db.clone(), lease.clone());
        self.sync_now_scoped(&scoped_db, &lease).await
    }

    pub async fn sync_now_scoped(
        &self,
        scoped_db: &ScopedDb,
        lease: &ScopeLease,
    ) -> Result<SyncReport, CloudSyncError> {
        scoped_db
            .validate_lease(lease)
            .map_err(CloudSyncError::Sync)?;

        let token = self
            .secret_store
            .get(&cloud_access_token_ref())
            .await
            .map_err(|_| CloudSyncError::AuthenticationRequired)?
            .ok_or(CloudSyncError::AuthenticationRequired)?;

        let identity = self.db.get_or_create_cloud_identity()?;
        let (checkpoint_cursor, sync_epoch) = scoped_db
            .read_checkpoint(lease)
            .map_err(CloudSyncError::Sync)?;

        // 1. Push phase
        let push_batch = scoped_db
            .load_or_create_push_batch(lease, 100)
            .map_err(CloudSyncError::Sync)?;
        let mut pushed = 0;
        let mut conflicts = 0;

        if !push_batch.is_empty() {
            let mut req = self
                .http
                .post(format!("{}/sync/push", self.base_url))
                .bearer_auth(token.expose_secret())
                .header(SYNC_PROTOCOL_HEADER, "2")
                .header("Idempotency-Key", uuid::Uuid::new_v4().to_string());
            if let Some(ref epoch) = sync_epoch {
                req = req.header(SYNC_EPOCH_HEADER, epoch);
            }
            let response = req
                .json(&serde_json::json!({
                    "device_id": identity.device_id,
                    "changes": push_batch
                }))
                .send()
                .await
                .map_err(|_| CloudSyncError::Transport)?;

            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                return Err(CloudSyncError::AuthenticationRequired);
            }
            if !response.status().is_success() {
                return Err(CloudSyncError::Transport);
            }

            scoped_db
                .validate_lease(lease)
                .map_err(CloudSyncError::Sync)?;

            let push_ack: PushAck = response
                .json()
                .await
                .map_err(|_| CloudSyncError::InvalidResponse)?;

            // Crucial S2 invariant: push_ack does NOT touch download_cursor!
            let ack_report = scoped_db
                .ack_push(lease, &push_ack)
                .map_err(CloudSyncError::Sync)?;
            pushed = ack_report.applied + ack_report.duplicate;
            conflicts += ack_report.conflicts;
        }

        // 2. Pull phase
        let mut pulled = 0;
        let mut cursor = checkpoint_cursor;

        loop {
            scoped_db
                .validate_lease(lease)
                .map_err(CloudSyncError::Sync)?;

            let mut req = self
                .http
                .get(format!("{}/sync/pull", self.base_url))
                .bearer_auth(token.expose_secret())
                .header(SYNC_PROTOCOL_HEADER, "2")
                .query(&[("cursor", cursor.as_str()), ("limit", "100")]);
            if let Some(ref epoch) = sync_epoch {
                req = req.header(SYNC_EPOCH_HEADER, epoch);
            }

            let response = req.send().await.map_err(|_| CloudSyncError::Transport)?;

            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                return Err(CloudSyncError::AuthenticationRequired);
            }
            if !response.status().is_success() {
                return Err(CloudSyncError::Transport);
            }

            scoped_db
                .validate_lease(lease)
                .map_err(CloudSyncError::Sync)?;

            let page: PullPage = response
                .json()
                .await
                .map_err(|_| CloudSyncError::InvalidResponse)?;

            // Atomically apply page: updates business tables, mirror, conflicts, and advances download_cursor
            let page_report = scoped_db
                .apply_pull_page(lease, &cursor, &page)
                .map_err(CloudSyncError::Sync)?;
            pulled += page_report.applied + page_report.duplicate;
            conflicts += page_report.conflicts;
            cursor = page_report.download_cursor;

            if !page.has_more {
                break;
            }
        }

        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.db.conn();
        conn.execute(
            "UPDATE cloud_sync_cursor SET last_success_at=?1, last_error_code=NULL, updated_at=?1 WHERE singleton=1",
            rusqlite::params![now],
        )?;

        Ok(SyncReport {
            pushed,
            pulled,
            conflicts,
            cursor,
        })
    }
}
