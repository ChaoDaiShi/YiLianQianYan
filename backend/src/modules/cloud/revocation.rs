use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use rusqlite::params;
use secrecy::ExposeSecret;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::integrations::secret::{
    cloud_access_token_ref, cloud_refresh_token_ref, revoke_session_token_ref,
    scoped_cloud_access_token_ref, scoped_cloud_refresh_token_ref, SecretStore,
};
use crate::modules::cloud::contracts::ScopeLease;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalCleanup {
    Cleared,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteRevocation {
    Confirmed,
    Pending,
    Unknown,
    NotRequired,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogoutReport {
    pub local_signed_out: bool,
    pub local_cleanup: LocalCleanup,
    pub remote_revocation: RemoteRevocation,
    pub error_codes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingRevocationRecord {
    pub session_id: String,
    pub scope_id: String,
    pub service_origin: String,
    pub account_id: String,
    pub attempts: u32,
    pub next_retry_at: i64,
    pub last_error_code: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisteredRevocationSummary {
    pub profile_id: String,
    pub session_id: String,
    pub remote_revocation: String,
    pub last_error_code: Option<String>,
}

pub struct RevocationService {
    db: Database,
    secret_store: Arc<dyn SecretStore>,
}

impl RevocationService {
    pub fn new(db: Database, secret_store: Arc<dyn SecretStore>) -> Self {
        Self { db, secret_store }
    }

    /// Execute logout:
    /// 1. Disable local session credentials
    /// 2. Attempt remote revocation
    /// 3. If remote unreachable, store non-sensitive pending task and revoke-only token
    pub async fn logout(
        &self,
        lease: &ScopeLease,
        session_id_opt: Option<&str>,
        service_origin_opt: Option<&str>,
        account_id_opt: Option<&str>,
    ) -> LogoutReport {
        // Retrieve active token for remote revocation before local deletion
        let access_token_ref = scoped_cloud_access_token_ref(&lease.scope_id);
        let active_token = match self.secret_store.get(&access_token_ref).await {
            Ok(Some(t)) => Some(t),
            Ok(None) => match self.secret_store.get(&cloud_access_token_ref()).await {
                Ok(t) => t,
                Err(_) => None,
            },
            Err(_) => None,
        };

        // Clear local database session and delete secrets individually
        let mut error_codes = Vec::<String>::new();
        if self.db.mark_cloud_profile_offline().is_err() {
            error_codes.push("LOCAL_IDENTITY_WRITE_FAILED".into());
        }
        for secret_ref in [
            scoped_cloud_access_token_ref(&lease.scope_id),
            scoped_cloud_refresh_token_ref(&lease.scope_id),
            cloud_access_token_ref(),
            cloud_refresh_token_ref(),
        ] {
            if self.secret_store.delete(&secret_ref).await.is_err() {
                error_codes.push("LOCAL_SECRET_DELETE_FAILED".into());
            }
        }
        error_codes.sort();
        error_codes.dedup();
        let local_cleanup = if error_codes.is_empty() {
            LocalCleanup::Cleared
        } else {
            LocalCleanup::Failed
        };

        let (Some(session_id), Some(origin)) = (session_id_opt, service_origin_opt) else {
            return LogoutReport {
                local_signed_out: local_cleanup == LocalCleanup::Cleared,
                local_cleanup,
                remote_revocation: RemoteRevocation::NotRequired,
                error_codes,
            };
        };

        let remote_revocation = if let Some(token) = active_token {
            let remote_res = send_remote_revoke(origin, session_id, token.expose_secret()).await;
            match remote_res {
                Ok(true) => RemoteRevocation::Confirmed,
                Ok(false) | Err(_) => {
                    let revoke_ref = revoke_session_token_ref(session_id);
                    let store_ok = self
                        .secret_store
                        .put(
                            &revoke_ref,
                            SecretString::from(token.expose_secret().to_string()),
                        )
                        .await
                        .is_ok();

                    let account_id = account_id_opt.unwrap_or("unknown");
                    let now = chrono::Utc::now().timestamp_millis();
                    let inserted = self.insert_pending_revocation(
                        session_id,
                        &lease.scope_id,
                        origin,
                        account_id,
                        now + 60_000,
                        Some("REMOTE_UNREACHABLE"),
                        now,
                    );
                    if store_ok && inserted.is_ok() {
                        RemoteRevocation::Pending
                    } else {
                        RemoteRevocation::Unknown
                    }
                }
            }
        } else {
            let account_id = account_id_opt.unwrap_or("unknown");
            let now = chrono::Utc::now().timestamp_millis();
            let _ = self.insert_pending_revocation(
                session_id,
                &lease.scope_id,
                origin,
                account_id,
                now + 60_000,
                Some("REVOKE_CREDENTIAL_MISSING"),
                now,
            );
            RemoteRevocation::Unknown
        };

        LogoutReport {
            local_signed_out: local_cleanup == LocalCleanup::Cleared,
            local_cleanup,
            remote_revocation,
            error_codes,
        }
    }

    /// Insert or update pending revocation task
    pub fn insert_pending_revocation(
        &self,
        session_id: &str,
        scope_id: &str,
        service_origin: &str,
        account_id: &str,
        next_retry_at: i64,
        error_code: Option<&str>,
        now: i64,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO cloud_pending_revocations (
                session_id, scope_id, service_origin, account_id,
                attempts, next_retry_at, last_error_code, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?7)
            ON CONFLICT(session_id) DO UPDATE SET
                attempts = attempts + 1,
                next_retry_at = excluded.next_retry_at,
                last_error_code = excluded.last_error_code,
                updated_at = excluded.updated_at",
            params![
                session_id,
                scope_id,
                service_origin,
                account_id,
                next_retry_at,
                error_code,
                now,
            ],
        )?;
        Ok(())
    }

    /// List pending revocations ready for retry
    pub fn list_pending_revocations(
        &self,
        now: i64,
    ) -> Result<Vec<PendingRevocationRecord>, rusqlite::Error> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT session_id, scope_id, service_origin, account_id, attempts, next_retry_at, last_error_code, created_at, updated_at
             FROM cloud_pending_revocations
             WHERE attempts < 5
               AND next_retry_at <= ?1
               AND COALESCE(last_error_code, '') NOT IN
                   ('RETRY_EXHAUSTED', 'REVOKE_CREDENTIAL_MISSING')
             ORDER BY next_retry_at ASC LIMIT 50",
        )?;
        let rows = stmt.query_map(params![now], |row| {
            Ok(PendingRevocationRecord {
                session_id: row.get(0)?,
                scope_id: row.get(1)?,
                service_origin: row.get(2)?,
                account_id: row.get(3)?,
                attempts: row.get(4)?,
                next_retry_at: row.get(5)?,
                last_error_code: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    /// Retry pending revocations against remote servers.
    /// Never reactivates signed-out accounts.
    pub async fn retry_pending_revocations(&self, now: i64) -> Result<usize, rusqlite::Error> {
        let pending = self.list_pending_revocations(now)?;
        let mut confirmed_count = 0;

        for item in pending {
            let revoke_ref = revoke_session_token_ref(&item.session_id);
            let token_res = self.secret_store.get(&revoke_ref).await;
            let token_opt = match token_res {
                Ok(opt) => opt,
                Err(_) => {
                    let backoff = (item.attempts as i64 + 1) * 60_000;
                    let conn = self.db.conn();
                    let _ = conn.execute(
                        "UPDATE cloud_pending_revocations SET attempts = attempts + 1, next_retry_at = ?1, last_error_code = 'SECRET_READ_FAILED', updated_at = ?2 WHERE session_id = ?3",
                        params![now + backoff, now, item.session_id],
                    );
                    continue;
                }
            };

            if token_opt.is_none() {
                let conn = self.db.conn();
                let _ = conn.execute(
                    "UPDATE cloud_pending_revocations SET last_error_code = 'REVOKE_CREDENTIAL_MISSING', updated_at = ?1 WHERE session_id = ?2",
                    params![now, item.session_id],
                );
                continue;
            }

            if item.attempts >= 5 {
                {
                    let conn = self.db.conn();
                    let _ = conn.execute(
                        "UPDATE cloud_pending_revocations SET last_error_code = 'RETRY_EXHAUSTED', updated_at = ?1 WHERE session_id = ?2",
                        params![now, item.session_id],
                    );
                }
                let _ = self.secret_store.delete(&revoke_ref).await;
                continue;
            }

            let token = token_opt.unwrap();
            let res = send_remote_revoke(
                &item.service_origin,
                &item.session_id,
                token.expose_secret(),
            )
            .await;
            match res {
                Ok(true) => {
                    {
                        let conn = self.db.conn();
                        let _ = conn.execute(
                            "DELETE FROM cloud_pending_revocations WHERE session_id = ?1",
                            params![item.session_id],
                        );
                    }
                    let _ = self.secret_store.delete(&revoke_ref).await;
                    confirmed_count += 1;
                }
                _ => {
                    let next_attempts = item.attempts + 1;
                    {
                        let conn = self.db.conn();
                        if next_attempts >= 5 {
                            let _ = conn.execute(
                                "UPDATE cloud_pending_revocations SET attempts = ?1, last_error_code = 'RETRY_EXHAUSTED', updated_at = ?2 WHERE session_id = ?3",
                                params![next_attempts, now, item.session_id],
                            );
                        } else {
                            let backoff = (next_attempts as i64) * 60_000;
                            let _ = conn.execute(
                                "UPDATE cloud_pending_revocations SET attempts = ?1, next_retry_at = ?2, last_error_code = 'REMOTE_UNREACHABLE', updated_at = ?3 WHERE session_id = ?4",
                                params![next_attempts, now + backoff, now, item.session_id],
                            );
                        }
                    }
                    if next_attempts >= 5 {
                        let _ = self.secret_store.delete(&revoke_ref).await;
                    }
                }
            }
        }

        Ok(confirmed_count)
    }
}

pub async fn retry_registered_profile_revocations(
    app_data_dir: &std::path::Path,
    secret_store: std::sync::Arc<dyn crate::integrations::secret::SecretStore>,
) -> Result<usize, super::session::CloudSessionError> {
    let registry =
        match crate::modules::cloud::profile::ProfileManager::load_or_initialize(app_data_dir) {
            Ok(r) => r,
            Err(_) => return Ok(0),
        };

    let now = chrono::Utc::now().timestamp_millis();
    let mut total_confirmed = 0;

    for entry in registry.profiles.values() {
        let db_path = app_data_dir
            .join(crate::modules::cloud::profile::PROFILES_DIRNAME)
            .join(&entry.directory_name)
            .join("app.db");
        if !db_path.exists() {
            tracing::warn!(profile_id = %entry.profile_id, "profile database missing, skipping revocation retry");
            continue;
        }
        let db = match Database::new(&db_path) {
            Ok(db) => db,
            Err(e) => {
                tracing::warn!(profile_id = %entry.profile_id, %e, "failed to open profile database for revocation retry");
                continue;
            }
        };
        let service = RevocationService::new(db, Arc::clone(&secret_store));
        match service.retry_pending_revocations(now).await {
            Ok(count) => total_confirmed += count,
            Err(e) => {
                tracing::warn!(profile_id = %entry.profile_id, %e, "error retrying pending revocations");
            }
        }
    }

    Ok(total_confirmed)
}

pub fn list_registered_profile_revocations(
    app_data_dir: &std::path::Path,
) -> Result<Vec<RegisteredRevocationSummary>, super::session::CloudSessionError> {
    let registry = crate::modules::cloud::profile::ProfileManager::load_or_initialize(app_data_dir)
        .map_err(|_| super::session::CloudSessionError::Unavailable)?;
    let mut summaries = Vec::new();
    for entry in registry.profiles.values() {
        let db_path = app_data_dir
            .join(crate::modules::cloud::profile::PROFILES_DIRNAME)
            .join(&entry.directory_name)
            .join("app.db");
        if !db_path.exists() {
            continue;
        }
        let db = Database::new(&db_path)?;
        let conn = db.conn();
        let mut statement = conn.prepare(
            "SELECT session_id, last_error_code FROM cloud_pending_revocations ORDER BY created_at",
        )?;
        let rows = statement.query_map([], |row| {
            let last_error_code: Option<String> = row.get(1)?;
            let remote_revocation = match last_error_code.as_deref() {
                Some("RETRY_EXHAUSTED") | Some("REVOKE_CREDENTIAL_MISSING") => "unknown",
                _ => "pending",
            };
            Ok(RegisteredRevocationSummary {
                profile_id: entry.profile_id.clone(),
                session_id: row.get(0)?,
                remote_revocation: remote_revocation.to_string(),
                last_error_code,
            })
        })?;
        summaries.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    Ok(summaries)
}

pub async fn run_registered_profile_revocation_retry_loop(
    app_data_dir: std::path::PathBuf,
    secret_store: Arc<dyn SecretStore>,
    interval: Duration,
    cancellation: tokio_util::sync::CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => break,
            _ = tokio::time::sleep(interval) => {
                retry_registered_profile_local_cleanup(
                    &app_data_dir,
                    Arc::clone(&secret_store),
                ).await;
                if let Err(error) = retry_registered_profile_revocations(
                    &app_data_dir,
                    Arc::clone(&secret_store),
                ).await {
                    tracing::warn!(%error, "pending session revocation retry deferred");
                }
            }
        }
    }
}

async fn retry_registered_profile_local_cleanup(
    app_data_dir: &std::path::Path,
    secret_store: Arc<dyn SecretStore>,
) {
    let registry =
        match crate::modules::cloud::profile::ProfileManager::load_or_initialize(app_data_dir) {
            Ok(registry) => registry,
            Err(error) => {
                tracing::warn!(%error, "profile registry unavailable for local logout cleanup");
                return;
            }
        };
    let profiles = registry
        .profiles
        .values()
        .filter(|entry| {
            entry.kind == crate::modules::cloud::profile::ProfileKind::Account
                && entry.reauthentication_required
        })
        .collect::<Vec<_>>();
    if profiles.is_empty() {
        return;
    }
    for entry in profiles {
        for secret_ref in [
            scoped_cloud_access_token_ref(&entry.scope_id),
            scoped_cloud_refresh_token_ref(&entry.scope_id),
        ] {
            if let Err(error) = secret_store.delete(&secret_ref).await {
                tracing::warn!(
                    profile_id = %entry.profile_id,
                    %error,
                    "original profile credential cleanup deferred"
                );
            }
        }
    }
    for secret_ref in [cloud_access_token_ref(), cloud_refresh_token_ref()] {
        if let Err(error) = secret_store.delete(&secret_ref).await {
            tracing::warn!(%error, "legacy cloud credential cleanup deferred");
        }
    }
}

async fn send_remote_revoke(
    origin: &str,
    session_id: &str,
    token: &str,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let (client, base) = super::transport::build_cloud_http(origin, Duration::from_secs(10))?;
    let url = format!("{base}/sessions/{session_id}/revoke");
    let response = client
        .post(&url)
        .header("Idempotency-Key", uuid::Uuid::new_v4().to_string())
        .bearer_auth(token)
        .send()
        .await?;

    if response.status().is_success() {
        return Ok(true);
    }
    if response.status() == StatusCode::NOT_FOUND {
        if let Ok(body) = response.text().await {
            if body.contains("NOT_FOUND") || body.contains("Session not found") {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
