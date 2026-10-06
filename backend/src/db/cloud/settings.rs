use crate::db::Database;
use rusqlite::{params, OptionalExtension};

impl Database {
    pub fn save_cloud_lifecycle_job(
        &self,
        job: &crate::modules::cloud::domain::LifecycleJob,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO cloud_lifecycle_jobs (id,kind,status,progress,result_ref,error_code,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(id) DO UPDATE SET status=excluded.status,progress=excluded.progress,
                result_ref=excluded.result_ref,error_code=excluded.error_code,updated_at=excluded.updated_at",
            params![job.id,job.kind,job.status,job.progress,job.result_ref,job.error_code,job.created_at,job.updated_at],
        )?;
        Ok(())
    }

    pub fn cloud_search_config(
        &self,
    ) -> Result<
        (
            String,
            String,
            Option<crate::integrations::secret::SecretRef>,
        ),
        rusqlite::Error,
    > {
        let conn = self.conn();
        let row: Option<(String, String, Option<String>)> = conn
            .query_row(
                "SELECT provider, base_url, api_key_ref_json FROM cloud_search_config WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((provider, base_url, secret_json)) = row else {
            return Ok(("anysearch".into(), "https://api.anysearch.com".into(), None));
        };
        let secret_ref = secret_json
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
        Ok((provider, base_url, secret_ref))
    }

    pub fn save_cloud_search_config(
        &self,
        base_url: &str,
        secret_ref: Option<&crate::integrations::secret::SecretRef>,
    ) -> Result<(), rusqlite::Error> {
        let secret_json = secret_ref
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "INSERT INTO cloud_search_config (singleton, provider, base_url, api_key_ref_json, updated_at)
             VALUES (1, 'anysearch', ?1, ?2, ?3)
             ON CONFLICT(singleton) DO UPDATE SET base_url=excluded.base_url,
                api_key_ref_json=excluded.api_key_ref_json, updated_at=excluded.updated_at",
            params![base_url, secret_json, now],
        )?;
        Ok(())
    }
}
