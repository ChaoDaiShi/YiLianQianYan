//! Snapshot an existing installation before applying the November capability migrations.
use rusqlite::{Connection, OptionalExtension, Result};
use std::path::Path;
fn error(message: impl ToString) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.to_string())
}
fn check(conn: &Connection) -> Result<()> {
    let status: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if status != "ok" {
        return Err(error(format!("SQLite integrity check: {status}")));
    }
    Ok(())
}

pub(super) fn snapshot_if_needed(path: &Path, conn: &Connection) -> Result<()> {
    if path == Path::new(":memory:")
        || !super::migrations::table_exists(conn, "conversations")?
        || super::migrations::table_exists(conn, "local_onboarding")?
    {
        return Ok(());
    }
    check(conn)?;
    let parent = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| error("missing database name"))?
        .to_string_lossy();
    let marker = "upgrade.backup.capabilities.20261124";
    let has_settings = super::migrations::table_exists(conn, "settings")?;
    if has_settings {
        let prior: Option<String> = conn
            .query_row("SELECT value FROM settings WHERE key=?1", [marker], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(id) = prior.and_then(|v| uuid::Uuid::parse_str(&v).ok()) {
            let prior_file = parent.join(format!("{name}.before-capability-upgrade-{id}.backup"));
            if prior_file.is_file() {
                let prior_conn = Connection::open_with_flags(
                    prior_file,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )?;
                check(&prior_conn)?;
                return Ok(());
            }
        }
    }
    let id = uuid::Uuid::new_v4();
    let dest = parent.join(format!("{name}.before-capability-upgrade-{id}.backup"));
    let temp = dest.with_extension("partial");
    let result = (|| {
        let mut out = Connection::open(&temp)?;
        rusqlite::backup::Backup::new(conn, &mut out)?.run_to_completion(
            128,
            std::time::Duration::from_millis(5),
            None,
        )?;
        check(&out)?;
        drop(out);
        std::fs::rename(&temp, &dest).map_err(error)?;
        if has_settings {
            conn.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",rusqlite::params![marker,id.to_string()])?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backup_restores_legacy_content_and_migration_is_repeatable() {
        let dir = std::env::temp_dir().join(format!("yilian-upgrade-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("legacy.sqlite");
        {
            let db = super::super::Database::new(&path).unwrap();
            db.conn().execute_batch("INSERT INTO conversations(id,title,created_at,updated_at) VALUES('kept','历史对话',1,1);
                DROP TABLE local_onboarding; DELETE FROM schema_migrations WHERE version=1110;").unwrap();
        }
        for _ in 0..2 {
            let db = super::super::Database::new(&path).unwrap();
            let title: String = db
                .conn()
                .query_row("SELECT title FROM conversations WHERE id='kept'", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(title, "历史对话");
        }
        let backups = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|s| s == "backup"))
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        let restored = dir.join("restored.sqlite");
        std::fs::copy(&backups[0], &restored).unwrap();
        let conn = Connection::open(&restored).unwrap();
        check(&conn).unwrap();
        assert!(!super::super::migrations::table_exists(&conn, "local_onboarding").unwrap());
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM conversations WHERE id='kept'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        drop(conn);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
