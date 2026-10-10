use std::path::{Path, PathBuf};
use std::time::Duration;

use super::contracts::ScopeError;
use super::profile::{ProfileManager, LEGACY_QUARANTINE_DIRNAME};

impl ProfileManager {
    pub fn quarantine_legacy_db_if_needed(
        app_data_dir: &Path,
    ) -> Result<Option<PathBuf>, ScopeError> {
        let legacy_db_path = app_data_dir.join("yilianqianyan-v1.db");
        if !legacy_db_path.exists() {
            return Ok(None);
        }

        let migration_id = format!("quarantine-{}", chrono::Utc::now().format("%Y%m%d%H%M%S"));
        let quarantine_dir = app_data_dir
            .join(LEGACY_QUARANTINE_DIRNAME)
            .join(&migration_id);
        std::fs::create_dir_all(&quarantine_dir)
            .map_err(|error| ScopeError::Io(format!("create quarantine dir: {error}")))?;

        let dest_db_path = quarantine_dir.join("yilianqianyan-v1.db");
        backup_sqlite_db(&legacy_db_path, &dest_db_path).map_err(|error| {
            ScopeError::MigrationFailed(format!("sqlite backup API failed: {error}"))
        })?;

        let manifest = serde_json::json!({
            "migration_id": migration_id,
            "source_path": legacy_db_path.to_string_lossy(),
            "destination_path": dest_db_path.to_string_lossy(),
            "quarantined_at": chrono::Utc::now().timestamp_millis(),
            "reason": "V1.1 profile isolation quarantine"
        });
        std::fs::write(quarantine_dir.join("manifest.json"), manifest.to_string())
            .map_err(|error| ScopeError::Io(format!("write quarantine manifest: {error}")))?;

        tracing::info!(
            "Legacy database backed up successfully to {}",
            dest_db_path.display()
        );
        Ok(Some(dest_db_path))
    }
}

pub(super) fn backup_sqlite_db(src_path: &Path, dst_path: &Path) -> Result<(), rusqlite::Error> {
    let src_conn = rusqlite::Connection::open(src_path)?;
    let mut dst_conn = rusqlite::Connection::open(dst_path)?;
    let backup = rusqlite::backup::Backup::new(&src_conn, &mut dst_conn)?;
    backup.run_to_completion(5, Duration::from_millis(20), None)?;
    Ok(())
}
