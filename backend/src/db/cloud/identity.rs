use crate::db::Database;
use crate::modules::cloud::domain::{IdentitySnapshot, IdentityStatus};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

impl Database {
    pub fn get_or_create_cloud_identity(&self) -> Result<IdentitySnapshot, rusqlite::Error> {
        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let row = transaction
            .query_row(
                "SELECT local_identity_id, device_id, account_id, email, status FROM cloud_identity WHERE singleton=1",
                [],
                map_identity,
            )
            .optional()?;
        if let Some(identity) = row {
            transaction.commit()?;
            return Ok(identity);
        }
        let local_identity_id = uuid::Uuid::new_v4().to_string();
        let device_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        transaction.execute(
            "INSERT INTO cloud_identity (singleton, local_identity_id, device_id, device_label, status, updated_at)
             VALUES (1, ?1, ?2, 'Windows device', 'guest', ?3)",
            params![local_identity_id, device_id, now],
        )?;
        transaction.commit()?;
        Ok(IdentitySnapshot::guest(local_identity_id, device_id))
    }

    pub fn cloud_identity(&self) -> Result<Option<IdentitySnapshot>, rusqlite::Error> {
        let conn = self.conn();
        conn.query_row(
            "SELECT local_identity_id, device_id, account_id, email, status FROM cloud_identity WHERE singleton=1",
            [],
            map_identity,
        )
        .optional()
    }

    pub fn save_authenticated_cloud_session(
        &self,
        account_id: &str,
        email: &str,
        session_id: &str,
        expires_at: i64,
        access_token_ref: &crate::integrations::secret::SecretRef,
        refresh_token_ref: &crate::integrations::secret::SecretRef,
    ) -> Result<(), rusqlite::Error> {
        self.get_or_create_cloud_identity()?;
        let now = chrono::Utc::now().timestamp_millis();
        let access_ref = serde_json::to_string(access_token_ref)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let refresh_ref = serde_json::to_string(refresh_token_ref)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let conn = self.conn();
        conn.execute(
            "UPDATE cloud_identity SET account_id=?1, email=?2, status='authenticated',
                    access_token_ref_json=?3, refresh_token_ref_json=?4,
                    current_session_id=?5, updated_at=?6 WHERE singleton=1",
            params![account_id, email, access_ref, refresh_ref, session_id, now],
        )?;
        let device_id: String = conn.query_row(
            "SELECT device_id FROM cloud_identity WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        conn.execute(
            "INSERT INTO cloud_session_metadata (session_id, account_id, device_id, expires_at, last_seen_at)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(session_id) DO UPDATE SET expires_at=excluded.expires_at, last_seen_at=excluded.last_seen_at",
            params![session_id, account_id, device_id, expires_at, now],
        )?;
        Ok(())
    }

    pub fn save_cloud_configuration(
        &self,
        cloud_base_url: &str,
        sync_existing_choice: &str,
    ) -> Result<(), rusqlite::Error> {
        self.get_or_create_cloud_identity()?;
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "UPDATE cloud_identity SET cloud_base_url=?1, sync_existing_choice=?2, updated_at=?3 WHERE singleton=1",
            params![cloud_base_url, sync_existing_choice, now],
        )?;
        Ok(())
    }

    pub fn cloud_connection_config(&self) -> Result<Option<(String, String)>, rusqlite::Error> {
        let conn = self.conn();
        conn.query_row(
            "SELECT cloud_base_url, sync_existing_choice FROM cloud_identity
             WHERE singleton=1 AND cloud_base_url IS NOT NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
    }

    pub fn cloud_current_session_id(&self) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.conn();
        conn.query_row(
            "SELECT current_session_id FROM cloud_identity WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map(|value| value.flatten())
    }

    pub fn set_sync_existing_choice(&self, choice: &str) -> Result<(), rusqlite::Error> {
        if !matches!(choice, "undecided" | "sync" | "local_only") {
            return Err(rusqlite::Error::InvalidParameterName(
                "invalid sync existing choice".into(),
            ));
        }
        self.get_or_create_cloud_identity()?;
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "UPDATE cloud_identity SET sync_existing_choice=?1,updated_at=?2 WHERE singleton=1",
            params![choice, now],
        )?;
        Ok(())
    }

    pub fn cloud_sync_existing_choice(&self) -> Result<String, rusqlite::Error> {
        self.get_or_create_cloud_identity()?;
        let conn = self.conn();
        conn.query_row(
            "SELECT sync_existing_choice FROM cloud_identity WHERE singleton=1",
            [],
            |row| row.get(0),
        )
    }

    pub fn clear_authenticated_cloud_session(&self) -> Result<(), rusqlite::Error> {
        self.get_or_create_cloud_identity()?;
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "UPDATE cloud_identity SET account_id=NULL, email=NULL, status='guest',
                    access_token_ref_json=NULL, refresh_token_ref_json=NULL,
                    current_session_id=NULL, updated_at=?1 WHERE singleton=1",
            [now],
        )?;
        Ok(())
    }

    pub fn mark_cloud_identity_closing(&self) -> Result<(), rusqlite::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "UPDATE cloud_identity SET status='closing',updated_at=?1 WHERE singleton=1",
            [now],
        )?;
        Ok(())
    }
}

fn map_identity(row: &rusqlite::Row<'_>) -> rusqlite::Result<IdentitySnapshot> {
    let status: String = row.get(4)?;
    let status = match status.as_str() {
        "guest" => IdentityStatus::Guest,
        "authenticated" => IdentityStatus::Authenticated,
        "offline_authenticated" => IdentityStatus::OfflineAuthenticated,
        "closing" => IdentityStatus::Closing,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(IdentitySnapshot {
        local_identity_id: row.get(0)?,
        device_id: row.get(1)?,
        account_id: row.get(2)?,
        email: row.get(3)?,
        status,
        local_core_available: true,
        cloud_enabled: status != IdentityStatus::Guest,
    })
}
