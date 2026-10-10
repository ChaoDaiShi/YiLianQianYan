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
pub enum RemoteRevocation {
    Confirmed,
    Pending,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogoutReport {
    pub local_signed_out: bool,
    pub remote_revocation: RemoteRevocation,
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
        // First retrieve active token for remote revocation before local deletion
        let access_token_ref = scoped_cloud_access_token_ref(&lease.scope_id);
        let active_token = self
            .secret_store
            .get(&access_token_ref)
            .await
            .ok()
            .flatten()
            .or_else(|| {
                // Check legacy key as fallback
                futures::executor::block_on(async {
                    self.secret_store
                        .get(&cloud_access_token_ref())
                        .await
                        .ok()
                        .flatten()
                })
            });

        // Clear local database session
        let _ = self.db.mark_cloud_profile_offline();

        let (Some(session_id), Some(origin)) = (session_id_opt, service_origin_opt) else {
            // Nothing to revoke remotely
            let _ = self.secret_store.delete(&access_token_ref).await;
            let _ = self
                .secret_store
                .delete(&scoped_cloud_refresh_token_ref(&lease.scope_id))
                .await;
            let _ = self.secret_store.delete(&cloud_access_token_ref()).await;
            let _ = self.secret_store.delete(&cloud_refresh_token_ref()).await;
            return LogoutReport {
                local_signed_out: true,
                remote_revocation: RemoteRevocation::Confirmed,
            };
        };

        let remote_revocation = if let Some(token) = active_token {
            let remote_res = send_remote_revoke(origin, session_id, token.expose_secret()).await;
            match remote_res {
                Ok(true) => {
                    // Confirmed: remove local secrets
                    let _ = self.secret_store.delete(&access_token_ref).await;
                    let _ = self
                        .secret_store
                        .delete(&scoped_cloud_refresh_token_ref(&lease.scope_id))
                        .await;
                    let _ = self.secret_store.delete(&cloud_access_token_ref()).await;
                    let _ = self.secret_store.delete(&cloud_refresh_token_ref()).await;
                    RemoteRevocation::Confirmed
                }
                Ok(false) | Err(_) => {
                    // Store revoke-only credential in secret store
                    let revoke_ref = revoke_session_token_ref(session_id);
                    let store_ok = self
                        .secret_store
                        .put(
                            &revoke_ref,
                            SecretString::from(token.expose_secret().to_string()),
                        )
                        .await
                        .is_ok();

                    // Remove normal access and refresh tokens so they can't be used for regular requests
                    let _ = self.secret_store.delete(&access_token_ref).await;
                    let _ = self
                        .secret_store
                        .delete(&scoped_cloud_refresh_token_ref(&lease.scope_id))
                        .await;
                    let _ = self.secret_store.delete(&cloud_access_token_ref()).await;
                    let _ = self.secret_store.delete(&cloud_refresh_token_ref()).await;

                    if store_ok {
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
                        if inserted.is_ok() {
                            RemoteRevocation::Pending
                        } else {
                            RemoteRevocation::Unknown
                        }
                    } else {
                        RemoteRevocation::Unknown
                    }
                }
            }
        } else {
            // No token was found in SecretStore
            RemoteRevocation::Unknown
        };

        LogoutReport {
            local_signed_out: true,
            remote_revocation,
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
             FROM cloud_pending_revocations WHERE next_retry_at <= ?1 ORDER BY next_retry_at ASC LIMIT 50",
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
            let token_opt = self.secret_store.get(&revoke_ref).await.ok().flatten();

            // Expire tasks after 5 attempts or if token missing
            if item.attempts >= 5 || token_opt.is_none() {
                let conn = self.db.conn();
                let _ = conn.execute(
                    "DELETE FROM cloud_pending_revocations WHERE session_id = ?1",
                    params![item.session_id],
                );
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
                    let conn = self.db.conn();
                    let _ = conn.execute(
                        "DELETE FROM cloud_pending_revocations WHERE session_id = ?1",
                        params![item.session_id],
                    );
                    let _ = self.secret_store.delete(&revoke_ref).await;
                    confirmed_count += 1;
                }
                _ => {
                    let backoff = (item.attempts as i64 + 1) * 60_000;
                    let conn = self.db.conn();
                    let _ = conn.execute(
                        "UPDATE cloud_pending_revocations SET attempts = attempts + 1, next_retry_at = ?1, updated_at = ?2 WHERE session_id = ?3",
                        params![now + backoff, now, item.session_id],
                    );
                }
            }
        }

        Ok(confirmed_count)
    }
}

async fn send_remote_revoke(
    origin: &str,
    session_id: &str,
    token: &str,
) -> Result<bool, reqwest::Error> {
    let url = format!(
        "{}/sessions/{}/revoke",
        origin.trim_end_matches('/'),
        session_id
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let response = client
        .post(&url)
        .header("Idempotency-Key", uuid::Uuid::new_v4().to_string())
        .bearer_auth(token)
        .send()
        .await?;

    Ok(response.status().is_success() || response.status() == StatusCode::NOT_FOUND)
}
