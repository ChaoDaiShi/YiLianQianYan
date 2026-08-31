use rusqlite::{params, Connection, OptionalExtension};

const SHARED_VERSION_MAX: i64 = 999;

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const SHARED_MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "0001_resource_core",
    sql: "CREATE TABLE resources (
            id TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            name TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            size INTEGER NOT NULL CHECK (size >= 0),
            hash TEXT NOT NULL,
            storage_path TEXT NOT NULL,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
          );
          CREATE INDEX idx_resources_created_at ON resources(created_at);
          CREATE INDEX idx_resources_hash ON resources(hash);",
}];

pub(super) fn table_exists(conn: &Connection, table: &str) -> rusqlite::Result<bool> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1",
            [table],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(exists)
}

fn validate_shared_version(version: i64) -> rusqlite::Result<()> {
    if (0..=SHARED_VERSION_MAX).contains(&version) {
        return Ok(());
    }
    Err(rusqlite::Error::InvalidParameterName(format!(
        "migration version {version} is outside the Shared/Core namespace 0000-0999"
    )))
}

pub(super) fn run_versioned_migrations(
    conn: &mut Connection,
    adopted_v09: bool,
) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            owner TEXT NOT NULL,
            applied_at INTEGER NOT NULL
         );",
    )?;

    let now = chrono::Utc::now().timestamp_millis();
    let baseline_name = if adopted_v09 {
        "0000_v09_adopted"
    } else {
        "0000_v09_bootstrap"
    };
    conn.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, name, owner, applied_at)
         VALUES (0, ?1, 'shared', ?2)",
        params![baseline_name, now],
    )?;

    for migration in SHARED_MIGRATIONS {
        validate_shared_version(migration.version)?;
        let already_applied = conn
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE version=?1",
                [migration.version],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if already_applied {
            continue;
        }

        let transaction = conn.transaction()?;
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, name, owner, applied_at)
             VALUES (?1, ?2, 'shared', ?3)",
            params![migration.version, migration.name, now],
        )?;
        transaction.commit()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn versions(conn: &Connection) -> Vec<i64> {
        let mut statement = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .expect("schema_migrations must exist");
        statement
            .query_map([], |row| row.get::<_, i64>(0))
            .expect("migration versions must be queryable")
            .collect::<Result<Vec<_>, _>>()
            .expect("migration versions must be valid")
    }

    #[test]
    fn fresh_database_records_shared_baseline_and_resource_migration() {
        let mut conn = Connection::open_in_memory().unwrap();

        run_versioned_migrations(&mut conn, false).unwrap();

        assert_eq!(versions(&conn), vec![0, 1]);
        assert!(table_exists(&conn, "resources").unwrap());
    }

    #[test]
    fn existing_v09_database_is_adopted_without_losing_user_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
             );
             INSERT INTO conversations (id, title, created_at, updated_at)
             VALUES ('existing', '保留的对话', 1, 1);",
        )
        .unwrap();

        run_versioned_migrations(&mut conn, true).unwrap();

        let title: String = conn
            .query_row(
                "SELECT title FROM conversations WHERE id='existing'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(title, "保留的对话");
        assert_eq!(versions(&conn), vec![0, 1]);
    }

    #[test]
    fn repeat_startup_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();

        run_versioned_migrations(&mut conn, false).unwrap();
        run_versioned_migrations(&mut conn, false).unwrap();

        assert_eq!(versions(&conn), vec![0, 1]);
        let resource_table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='resources'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(resource_table_count, 1);
    }

    #[test]
    fn shared_migration_namespace_rejects_versions_owned_by_product_lines() {
        assert!(validate_shared_version(999).is_ok());
        assert!(validate_shared_version(1000).is_err());
        assert!(validate_shared_version(2000).is_err());
    }
}
