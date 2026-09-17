use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::ops::RangeInclusive;

const SHARED_VERSION_MIN: i64 = 0;
const SHARED_VERSION_MAX: i64 = 999;
const V1_TASK_WORLD_VERSION_MIN: i64 = 1000;
const V1_TASK_WORLD_VERSION_MAX: i64 = 1999;

const SCHEMA_MIGRATIONS_DDL: &str = "CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    owner TEXT NOT NULL,
    digest TEXT,
    applied_at INTEGER NOT NULL
);";

/// Product and Shared migration namespaces are represented explicitly so a
/// product module cannot accidentally register a migration in another owner's
/// version range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MigrationOwner {
    Shared,
    V1TaskWorld,
}

impl MigrationOwner {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::V1TaskWorld => "v1_task_world",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Shared => "Shared/Core",
            Self::V1TaskWorld => "v1 Task World",
        }
    }

    fn version_range(self) -> RangeInclusive<i64> {
        match self {
            Self::Shared => SHARED_VERSION_MIN..=SHARED_VERSION_MAX,
            Self::V1TaskWorld => V1_TASK_WORLD_VERSION_MIN..=V1_TASK_WORLD_VERSION_MAX,
        }
    }
}

/// A static migration definition consumed by Shared and product modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MigrationSpec {
    pub(crate) version: i64,
    pub(crate) name: &'static str,
    pub(crate) owner: MigrationOwner,
    pub(crate) sql: &'static str,
}

/// Descriptive alias for callers registering product-owned migrations.
pub(crate) type ProductMigrationSpec = MigrationSpec;

impl MigrationSpec {
    pub(crate) const fn new(
        version: i64,
        name: &'static str,
        owner: MigrationOwner,
        sql: &'static str,
    ) -> Self {
        Self {
            version,
            name,
            owner,
            sql,
        }
    }

    pub(crate) fn digest(&self) -> String {
        let bytes = Sha256::digest(self.sql.as_bytes());
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

const SHARED_MIGRATIONS: &[MigrationSpec] = &[MigrationSpec::new(
    1,
    "0001_resource_core",
    MigrationOwner::Shared,
    "CREATE TABLE resources (
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
)];

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

fn migration_error(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.into())
}

fn ensure_digest_column(conn: &Connection) -> rusqlite::Result<()> {
    let mut statement = conn.prepare("PRAGMA table_info(schema_migrations)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    if !columns.iter().any(|column| column == "digest") {
        conn.execute_batch("ALTER TABLE schema_migrations ADD COLUMN digest TEXT;")?;
    }
    Ok(())
}

fn validate_version(owner: MigrationOwner, version: i64) -> rusqlite::Result<()> {
    if owner.version_range().contains(&version) {
        return Ok(());
    }

    let range = owner.version_range();
    Err(migration_error(format!(
        "migration version {version} is outside the {} namespace {}-{}",
        owner.label(),
        range.start(),
        range.end()
    )))
}

fn validate_spec(spec: &MigrationSpec) -> rusqlite::Result<()> {
    if spec.name.is_empty() {
        return Err(migration_error("migration name must not be empty"));
    }
    validate_version(spec.owner, spec.version)
}

fn validate_specs(specs: &[MigrationSpec]) -> rusqlite::Result<()> {
    for (index, spec) in specs.iter().enumerate() {
        validate_spec(spec)?;
        for previous in &specs[..index] {
            if previous.version == spec.version {
                return Err(migration_error(format!(
                    "duplicate migration version {} in one registration batch",
                    spec.version
                )));
            }
            if previous.name == spec.name {
                return Err(migration_error(format!(
                    "duplicate migration name {:?} in one registration batch",
                    spec.name
                )));
            }
        }
    }
    Ok(())
}

fn validate_product_specs(specs: &[MigrationSpec]) -> rusqlite::Result<()> {
    validate_specs(specs)?;
    for spec in specs {
        if spec.owner == MigrationOwner::Shared {
            return Err(migration_error(
                "product migrations must use a product-owned migration namespace",
            ));
        }
    }
    Ok(())
}

fn validate_shared_version(version: i64) -> rusqlite::Result<()> {
    validate_version(MigrationOwner::Shared, version)
}

fn apply_migrations(conn: &mut Connection, specs: &[MigrationSpec]) -> rusqlite::Result<()> {
    validate_specs(specs)?;
    if specs.is_empty() {
        return Ok(());
    }

    let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(SCHEMA_MIGRATIONS_DDL)?;
    ensure_digest_column(&transaction)?;

    // Complete metadata preflight before executing any migration body. A
    // failure here rolls back the transaction and leaves product SQL untouched.
    let mut pending = Vec::with_capacity(specs.len());
    for spec in specs {
        let name_collision: Option<i64> = transaction
            .query_row(
                "SELECT version FROM schema_migrations
                 WHERE name=?1 AND version<>?2 LIMIT 1",
                params![spec.name, spec.version],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing_version) = name_collision {
            return Err(migration_error(format!(
                "migration name {:?} already belongs to version {existing_version}",
                spec.name
            )));
        }

        let existing: Option<(String, String, Option<String>)> = transaction
            .query_row(
                "SELECT name, owner, digest FROM schema_migrations WHERE version=?1",
                [spec.version],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        if let Some((existing_name, existing_owner, existing_digest)) = existing {
            if existing_name != spec.name || existing_owner != spec.owner.as_str() {
                return Err(migration_error(format!(
                    "migration version {} metadata collision: existing name={existing_name:?}, owner={existing_owner:?}; requested name={:?}, owner={:?}",
                    spec.version,
                    spec.name,
                    spec.owner.as_str()
                )));
            }
            let requested_digest = spec.digest();
            match existing_digest {
                Some(existing_digest) if existing_digest != requested_digest => {
                    return Err(migration_error(format!(
                        "migration version {} digest mismatch: registered SQL differs from the applied migration",
                        spec.version
                    )));
                }
                Some(_) => {}
                None => {
                    transaction.execute(
                        "UPDATE schema_migrations SET digest=?1 WHERE version=?2 AND digest IS NULL",
                        params![requested_digest, spec.version],
                    )?;
                }
            }
            continue;
        }

        pending.push(spec);
    }

    pending.sort_unstable_by_key(|spec| spec.version);

    let now = chrono::Utc::now().timestamp_millis();
    for spec in pending {
        transaction.execute_batch(spec.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, name, owner, digest, applied_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                spec.version,
                spec.name,
                spec.owner.as_str(),
                spec.digest(),
                now
            ],
        )?;
    }
    transaction.commit()
}

pub(crate) fn apply_product_migrations(
    conn: &mut Connection,
    specs: &[ProductMigrationSpec],
) -> rusqlite::Result<()> {
    validate_product_specs(specs)?;
    apply_migrations(conn, specs)
}

pub(crate) fn apply_product_migration(
    conn: &mut Connection,
    spec: &ProductMigrationSpec,
) -> rusqlite::Result<()> {
    apply_product_migrations(conn, std::slice::from_ref(spec))
}

pub(super) fn run_versioned_migrations(
    conn: &mut Connection,
    adopted_v09: bool,
) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA_MIGRATIONS_DDL)?;

    let now = chrono::Utc::now().timestamp_millis();
    let baseline_name = if adopted_v09 {
        "0000_v09_adopted"
    } else {
        "0000_v09_bootstrap"
    };

    let existing_baseline: Option<(String, String)> = conn
        .query_row(
            "SELECT name, owner FROM schema_migrations WHERE version=0",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match existing_baseline {
        // The adoption probe changes after the first bootstrap creates the
        // conversations table, so both historical Shared baseline names are
        // valid on subsequent starts.
        Some((name, owner))
            if owner == MigrationOwner::Shared.as_str()
                && (name == "0000_v09_bootstrap" || name == "0000_v09_adopted") => {}
        Some((name, owner)) => {
            return Err(migration_error(format!(
                "migration version 0 metadata collision: existing name={name:?}, owner={owner:?}; requested name={baseline_name:?}, owner={:?}",
                MigrationOwner::Shared.as_str()
            )))
        }
        None => {
            conn.execute(
                "INSERT INTO schema_migrations (version, name, owner, applied_at)
                 VALUES (0, ?1, ?2, ?3)",
                params![baseline_name, MigrationOwner::Shared.as_str(), now],
            )?;
        }
    }

    for migration in SHARED_MIGRATIONS {
        apply_migrations(conn, std::slice::from_ref(migration))?;
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

    fn metadata(conn: &Connection, version: i64) -> (String, String) {
        conn.query_row(
            "SELECT name, owner FROM schema_migrations WHERE version=?1",
            [version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("migration metadata must be queryable")
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
    fn product_migrations_register_under_the_v1_namespace() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_versioned_migrations(&mut conn, false).unwrap();

        let task_world = MigrationSpec {
            version: 1000,
            name: "1000_task_world_seed",
            owner: MigrationOwner::V1TaskWorld,
            sql: "CREATE TABLE task_world_seed (id INTEGER PRIMARY KEY);",
        };
        apply_product_migration(&mut conn, &task_world).unwrap();

        assert_eq!(
            metadata(&conn, 1000),
            ("1000_task_world_seed".into(), "v1_task_world".into())
        );
        assert!(table_exists(&conn, "task_world_seed").unwrap());
        let task_digest: String = conn
            .query_row(
                "SELECT digest FROM schema_migrations WHERE version=1000",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(task_digest, task_world.digest());
    }

    #[test]
    fn product_migrations_execute_pending_specs_in_version_order() {
        let mut conn = Connection::open_in_memory().unwrap();
        let child = MigrationSpec {
            version: 1001,
            name: "1001_task_world_child",
            owner: MigrationOwner::V1TaskWorld,
            sql: "CREATE TABLE task_world_child AS SELECT id FROM task_world_parent;",
        };
        let parent = MigrationSpec {
            version: 1000,
            name: "1000_task_world_parent",
            owner: MigrationOwner::V1TaskWorld,
            sql: "CREATE TABLE task_world_parent (id INTEGER PRIMARY KEY);",
        };

        apply_product_migrations(&mut conn, &[child, parent]).unwrap();

        assert_eq!(versions(&conn), vec![1000, 1001]);
        assert!(table_exists(&conn, "task_world_parent").unwrap());
        assert!(table_exists(&conn, "task_world_child").unwrap());
    }

    #[test]
    fn product_migrations_reject_owner_range_mismatch_before_sql() {
        let mut conn = Connection::open_in_memory().unwrap();
        let task_world_outside_owner_range = MigrationSpec {
            version: 999,
            name: "invalid_task_world_version",
            owner: MigrationOwner::V1TaskWorld,
            sql: "CREATE TABLE must_not_run_task_world (id INTEGER);",
        };
        assert!(apply_product_migration(&mut conn, &task_world_outside_owner_range).is_err());
        assert!(!table_exists(&conn, "schema_migrations").unwrap());
        assert!(!table_exists(&conn, "must_not_run_task_world").unwrap());
    }

    #[test]
    fn product_migrations_are_idempotent_and_reject_metadata_collisions() {
        let mut conn = Connection::open_in_memory().unwrap();
        let spec = MigrationSpec {
            version: 1001,
            name: "1001_task_world_state",
            owner: MigrationOwner::V1TaskWorld,
            sql: "CREATE TABLE task_world_state (id INTEGER PRIMARY KEY);",
        };

        apply_product_migration(&mut conn, &spec).unwrap();
        apply_product_migration(&mut conn, &spec).unwrap();
        assert_eq!(
            metadata(&conn, 1001),
            ("1001_task_world_state".into(), "v1_task_world".into())
        );

        let name_collision = MigrationSpec {
            version: 1001,
            name: "1001_task_world_renamed",
            owner: MigrationOwner::V1TaskWorld,
            sql: "CREATE TABLE must_not_run_name_collision (id INTEGER);",
        };
        assert!(apply_product_migration(&mut conn, &name_collision).is_err());

        conn.execute(
            "INSERT INTO schema_migrations (version, name, owner, applied_at)
             VALUES (1002, '1002_task_world_state', 'unexpected_owner', 0)",
            [],
        )
        .unwrap();
        let owner_collision = MigrationSpec {
            version: 1002,
            name: "1002_task_world_state",
            owner: MigrationOwner::V1TaskWorld,
            sql: "CREATE TABLE must_not_run_owner_collision (id INTEGER);",
        };
        assert!(apply_product_migration(&mut conn, &owner_collision).is_err());
        assert!(!table_exists(&conn, "must_not_run_name_collision").unwrap());
        assert!(!table_exists(&conn, "must_not_run_owner_collision").unwrap());
    }

    #[test]
    fn legacy_metadata_adopts_digest_once_and_rejects_changed_sql() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                owner TEXT NOT NULL,
                applied_at INTEGER NOT NULL
             );
             CREATE TABLE task_world_state (id INTEGER PRIMARY KEY);
             INSERT INTO schema_migrations (version, name, owner, applied_at)
             VALUES (1001, '1001_task_world_state', 'v1_task_world', 0);",
        )
        .unwrap();

        let original = MigrationSpec {
            version: 1001,
            name: "1001_task_world_state",
            owner: MigrationOwner::V1TaskWorld,
            sql: "CREATE TABLE task_world_state (id INTEGER PRIMARY KEY);",
        };
        apply_product_migration(&mut conn, &original).unwrap();

        let adopted_digest: String = conn
            .query_row(
                "SELECT digest FROM schema_migrations WHERE version=1001",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(adopted_digest, original.digest());

        apply_product_migration(&mut conn, &original).unwrap();

        let changed = MigrationSpec {
            version: 1001,
            name: "1001_task_world_state",
            owner: MigrationOwner::V1TaskWorld,
            sql: "CREATE TABLE digest_mismatch_body_must_not_run (id INTEGER PRIMARY KEY);",
        };
        let error = apply_product_migration(&mut conn, &changed).unwrap_err();
        assert!(error.to_string().contains("digest mismatch"));
        assert!(!table_exists(&conn, "digest_mismatch_body_must_not_run").unwrap());

        let pending = MigrationSpec {
            version: 1002,
            name: "1002_pending_before_digest_check",
            owner: MigrationOwner::V1TaskWorld,
            sql: "CREATE TABLE pending_body_must_roll_back (id INTEGER PRIMARY KEY);",
        };
        let error = apply_product_migrations(&mut conn, &[pending, changed]).unwrap_err();
        assert!(error.to_string().contains("digest mismatch"));
        assert!(!table_exists(&conn, "pending_body_must_roll_back").unwrap());
        assert!(!versions(&conn).contains(&1002));

        let persisted_digest: String = conn
            .query_row(
                "SELECT digest FROM schema_migrations WHERE version=1001",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted_digest, adopted_digest);
    }

    #[test]
    fn shared_migration_namespace_rejects_versions_owned_by_product_lines() {
        assert!(validate_shared_version(999).is_ok());
        assert!(validate_shared_version(1000).is_err());
        assert!(validate_shared_version(2000).is_err());
    }
}
