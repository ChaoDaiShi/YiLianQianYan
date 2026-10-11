use secrecy::ExposeSecret;
use std::sync::Arc;

use crate::integrations::secret::{scoped_cloud_access_token_ref, SecretStore};
use crate::modules::cloud::contracts::{
    RemoteRecord, ResolveConflictRequest, ResolveConflictResult, ScopeLease, ScopedDb, SyncError,
    SYNC_EPOCH_HEADER, SYNC_PROTOCOL_HEADER,
};
use crate::modules::cloud::domain::SyncOperation;

pub struct ConflictService {
    scoped_db: ScopedDb,
    secret_store: Arc<dyn SecretStore>,
    base_url: String,
    http: reqwest::Client,
}

impl ConflictService {
    pub fn new(
        scoped_db: ScopedDb,
        secret_store: Arc<dyn SecretStore>,
        base_url: impl Into<String>,
    ) -> Result<Self, SyncError> {
        let raw = base_url.into();
        let (http, base_url) =
            super::transport::build_cloud_http(&raw, std::time::Duration::from_secs(20)).map_err(
                |e| match e {
                    super::transport::CloudTransportError::InvalidUrl => {
                        SyncError::Transport("invalid base url".into())
                    }
                    super::transport::CloudTransportError::HttpsRequired => {
                        SyncError::Transport("remote cloud service requires HTTPS".into())
                    }
                    super::transport::CloudTransportError::ClientUnavailable => {
                        SyncError::Transport("cloud client unavailable".into())
                    }
                },
            )?;

        Ok(Self {
            scoped_db,
            secret_store,
            base_url,
            http,
        })
    }

    pub fn scoped_db(&self) -> &ScopedDb {
        &self.scoped_db
    }

    /// Fetches authoritative remote snapshot for an open conflict if payload is missing.
    /// Fails closed if 404 or network fails without modifying the open conflict record.
    pub async fn ensure_remote_snapshot(
        &self,
        lease: &ScopeLease,
        conflict_id: &str,
    ) -> Result<RemoteRecord, SyncError> {
        self.scoped_db.validate_lease(lease)?;

        let conflict = self.scoped_db.with_read_tx(lease, |tx| {
            crate::db::cloud::conflicts::get_conflict_tx(tx, conflict_id)
        })?;

        // If remote payload is already present or remote operation is delete, we don't need network fetch
        if conflict.remote_payload.is_some() || conflict.remote_operation == SyncOperation::Delete {
            return Ok(RemoteRecord {
                change_id: conflict.remote_change_id.unwrap_or_default(),
                entity_type: conflict.entity_type,
                entity_id: conflict.entity_id,
                revision: conflict.remote_revision,
                operation: conflict.remote_operation,
                payload: conflict.remote_payload,
                sequence: conflict.remote_sequence.unwrap_or_else(|| "0".into()),
                device_id: String::new(),
                changed_at: conflict.created_at,
            });
        }

        // Fetch authoritative snapshot via S1 GET /api/v1/sync/entities/:entity_type/:entity_id
        let token = self
            .secret_store
            .get(&scoped_cloud_access_token_ref(&lease.scope_id))
            .await
            .map_err(|_| SyncError::AuthenticationRequired)?
            .ok_or(SyncError::AuthenticationRequired)?;

        let url = if self.base_url.ends_with("/api/v1") {
            format!(
                "{}/sync/entities/{}/{}",
                self.base_url, conflict.entity_type, conflict.entity_id
            )
        } else {
            format!(
                "{}/api/v1/sync/entities/{}/{}",
                self.base_url, conflict.entity_type, conflict.entity_id
            )
        };

        let (_, epoch) = self.scoped_db.read_checkpoint(lease)?;
        let epoch = epoch.ok_or(SyncError::CursorResetRequired)?;
        let res = self
            .http
            .get(&url)
            .bearer_auth(token.expose_secret())
            .header(SYNC_PROTOCOL_HEADER, "2")
            .header(SYNC_EPOCH_HEADER, epoch)
            .send()
            .await
            .map_err(|e| SyncError::Transport(e.to_string()))?;

        if res.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SyncError::AuthenticationRequired);
        }

        if res.status() == reqwest::StatusCode::NOT_FOUND {
            // 404 does NOT mean delete; preserve open and return error
            return Err(SyncError::RemotePayloadMissing(
                "remote entity not found (404)".to_string(),
            ));
        }

        if !res.status().is_success() {
            return Err(SyncError::Transport(format!(
                "failed to get remote snapshot: status {}",
                res.status()
            )));
        }

        let remote_rec: RemoteRecord = res
            .json()
            .await
            .map_err(|e| SyncError::InvalidPage(format!("invalid remote record: {e}")))?;

        // Update stored conflict with complete remote snapshot
        let payload_str = remote_rec
            .payload
            .as_ref()
            .map(|p| serde_json::to_string(p).unwrap_or_default());

        self.scoped_db.with_write_tx(lease, |tx| {
            tx.execute(
                "UPDATE cloud_sync_conflicts SET
                    remote_payload_json=?2, remote_revision=?3, remote_operation=?4,
                    remote_change_id=?5, remote_sequence=?6
                 WHERE id=?1 AND status='open'",
                rusqlite::params![
                    conflict_id,
                    payload_str,
                    remote_rec.revision,
                    remote_rec.operation.as_str(),
                    &remote_rec.change_id,
                    &remote_rec.sequence,
                ],
            )?;
            Ok(())
        })?;

        Ok(remote_rec)
    }

    /// Resolves an open conflict atomically within an immediate database transaction.
    /// Validates conflict version, remote revision, local generation, and resolution choice.
    pub fn resolve_conflict(
        &self,
        lease: &ScopeLease,
        request: &ResolveConflictRequest,
        device_id: &str,
    ) -> Result<ResolveConflictResult, SyncError> {
        resolve_conflict(&self.scoped_db, lease, request, device_id)
    }
}

/// Standalone resolve_conflict helper as specified in the S4 plan
pub fn resolve_conflict(
    scoped_db: &ScopedDb,
    lease: &ScopeLease,
    request: &ResolveConflictRequest,
    device_id: &str,
) -> Result<ResolveConflictResult, SyncError> {
    scoped_db.with_write_tx(lease, |tx| {
        crate::db::cloud::conflicts::resolve_conflict_tx(tx, request, device_id)
    })
}
