use std::sync::Arc;
use uuid::Uuid;

use yilian_backend::db::Database;
use yilian_backend::integrations::secret::memory_store::InMemorySecretStore;
use yilian_backend::integrations::secret::{
    revoke_session_token_ref, scoped_cloud_access_token_ref, SecretStore,
};
use yilian_backend::modules::cloud::contracts::{
    canonical_scope, ScopeError, ScopeKey, ScopeLease, ScopedDb, SyncError,
};
use yilian_backend::modules::cloud::profile::{ActivationChoice, ProfileKind, ProfileManager};
use yilian_backend::modules::cloud::revocation::{RemoteRevocation, RevocationService};

struct TempDir(std::path::PathBuf);
impl TempDir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("yilian-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_s3_1_profile_physical_isolation_prevents_business_data_leak() {
    let tmp = TempDir::new();
    let app_data = tmp.path();

    // 1. Initial startup creates guest profile
    let registry = ProfileManager::load_or_initialize(app_data).expect("init guest profile");
    assert_eq!(registry.profiles.len(), 1);
    let guest_entry = registry.profiles.get(&registry.active_profile_id).unwrap();
    assert_eq!(guest_entry.kind, ProfileKind::Guest);

    let guest_db_path = app_data
        .join("profiles")
        .join(&guest_entry.directory_name)
        .join("app.db");
    let guest_db = Database::new(&guest_db_path).unwrap();

    // Insert sensitive guest conversation & message
    let conv_id = Uuid::new_v4().to_string();
    let msg_id = Uuid::new_v4().to_string();
    guest_db
        .conn()
        .execute(
            "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?1, 'Guest Secret Talk', 1000, 1000)",
            rusqlite::params![conv_id],
        )
        .unwrap();
    guest_db
        .conn()
        .execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?1, ?2, 'user', 'My private guest secret', 1000)",
            rusqlite::params![msg_id, conv_id],
        )
        .unwrap();

    // 2. Prepare Account A profile without importing guest data
    let scope_a = ScopeKey {
        service_origin: "https://cloud.example.com/api/v1".into(),
        account_id: "user-alpha".into(),
    };
    let plan_a =
        ProfileManager::prepare_profile(app_data, &scope_a, ActivationChoice::OpenAccountOnly)
            .expect("prepare account a");
    ProfileManager::persist_activation(app_data, &plan_a).expect("persist activation a");

    // Account A's database must be completely isolated and empty of Guest data
    let account_a_db_path = app_data
        .join("profiles")
        .join(&plan_a.profile_id)
        .join("app.db");
    let account_a_db = Database::new(&account_a_db_path).unwrap();

    let conv_count_a: i64 = account_a_db
        .conn()
        .query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0))
        .unwrap();
    let msg_count_a: i64 = account_a_db
        .conn()
        .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        conv_count_a, 0,
        "Account A DB must not contain guest conversations"
    );
    assert_eq!(
        msg_count_a, 0,
        "Account A DB must not contain guest messages"
    );

    // Account A inserts its own data
    let a_conv_id = Uuid::new_v4().to_string();
    account_a_db
        .conn()
        .execute(
            "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?1, 'Alpha Secret Work', 2000, 2000)",
            rusqlite::params![a_conv_id],
        )
        .unwrap();

    // 3. Prepare Account B profile
    let scope_b = ScopeKey {
        service_origin: "https://cloud.example.com/api/v1".into(),
        account_id: "user-beta".into(),
    };
    let plan_b =
        ProfileManager::prepare_profile(app_data, &scope_b, ActivationChoice::OpenAccountOnly)
            .expect("prepare account b");
    ProfileManager::persist_activation(app_data, &plan_b).expect("persist activation b");

    let account_b_db_path = app_data
        .join("profiles")
        .join(&plan_b.profile_id)
        .join("app.db");
    let account_b_db = Database::new(&account_b_db_path).unwrap();

    let conv_count_b: i64 = account_b_db
        .conn()
        .query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        conv_count_b, 0,
        "Account B DB must not contain Account A conversations"
    );

    // Guest database still contains its original data intact
    let guest_conv_count: i64 = guest_db
        .conn()
        .query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        guest_conv_count, 1,
        "Guest DB retains its own data undisturbed"
    );
}

#[test]
fn test_s3_2_canonical_scope_normalization_and_distinct_origins() {
    // Normalization rules:
    // host lowercase, remove default port, strip trailing slash, reject userinfo/query/fragment
    let (key1, hash1) =
        canonical_scope("https://Cloud.Example.COM:443/api/v1/", "user-123").unwrap();
    let (key2, hash2) = canonical_scope("https://cloud.example.com/api/v1", "user-123").unwrap();
    assert_eq!(key1.service_origin, "https://cloud.example.com/api/v1");
    assert_eq!(key1, key2);
    assert_eq!(hash1, hash2);

    // Reject userinfo
    let err_userinfo = canonical_scope("https://admin:pass@cloud.example.com/api", "u1");
    assert!(matches!(err_userinfo, Err(ScopeError::InvalidOrigin(_))));

    // Reject query params
    let err_query = canonical_scope("https://cloud.example.com/api?debug=1", "u1");
    assert!(matches!(err_query, Err(ScopeError::InvalidOrigin(_))));

    // Reject fragment
    let err_frag = canonical_scope("https://cloud.example.com/api#section", "u1");
    assert!(matches!(err_frag, Err(ScopeError::InvalidOrigin(_))));

    // Different origins with same account ID produce different scopes
    let (key_x, hash_x) = canonical_scope("https://server-x.example.com/api", "user-123").unwrap();
    let (key_y, hash_y) = canonical_scope("https://server-y.example.com/api", "user-123").unwrap();
    assert_ne!(key_x.service_origin, key_y.service_origin);
    assert_ne!(
        hash_x, hash_y,
        "Different service origins MUST have different scope IDs"
    );
}

#[test]
fn test_s3_3_scoped_db_lease_validation_and_write_gate() {
    let tmp = TempDir::new();
    let db_path = tmp.path().join("scoped.db");
    let db = Database::new(&db_path).unwrap();
    db.initialize_cloud_schema().unwrap();

    let lease_active = ScopeLease {
        scope_id: "scope-1111".into(),
        generation: "gen-active-1111".into(),
    };
    let scoped_db = ScopedDb::new(db.clone(), lease_active.clone());
    scoped_db
        .init_profile_metadata("https://cloud.example.com/api", "user-1111")
        .unwrap();

    // Active lease matches
    assert!(scoped_db.validate_lease(&lease_active).is_ok());

    // Stale generation lease rejected
    let lease_stale_gen = ScopeLease {
        scope_id: "scope-1111".into(),
        generation: "gen-stale-0000".into(),
    };
    assert!(matches!(
        scoped_db.validate_lease(&lease_stale_gen),
        Err(SyncError::ScopeChanged)
    ));

    // Wrong scope rejected
    let lease_wrong_scope = ScopeLease {
        scope_id: "scope-2222".into(),
        generation: "gen-active-1111".into(),
    };
    assert!(matches!(
        scoped_db.validate_lease(&lease_wrong_scope),
        Err(SyncError::ScopeChanged)
    ));

    // Stored scope validation passes for correct metadata
    assert!(scoped_db.validate_stored_scope().is_ok());

    // Write gate prevents write with mismatched lease
    let write_res = scoped_db.with_active_write(&lease_stale_gen, |conn| {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('test_key', '1')",
            [],
        )?;
        Ok(())
    });
    assert!(matches!(write_res, Err(SyncError::ScopeChanged)));

    // Write gate succeeds with active lease
    let write_ok = scoped_db.with_active_write(&lease_active, |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('test_key', '1')",
            [],
        )?;
        Ok(42)
    });
    assert_eq!(write_ok.unwrap(), 42);
}

#[test]
fn test_s3_4_selective_guest_import_on_profile_preparation() {
    let tmp = TempDir::new();
    let app_data = tmp.path();

    let registry = ProfileManager::load_or_initialize(app_data).unwrap();
    let guest_entry = registry.profiles.get(&registry.active_profile_id).unwrap();
    let guest_db_path = app_data
        .join("profiles")
        .join(&guest_entry.directory_name)
        .join("app.db");
    let guest_db = Database::new(&guest_db_path).unwrap();

    // Insert 2 guest conversations and 1 memory
    let conv1 = Uuid::new_v4().to_string();
    let conv2 = Uuid::new_v4().to_string();
    let mem1 = Uuid::new_v4().to_string();

    guest_db
        .conn()
        .execute(
            "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?1, 'Keep Me', 100, 100)",
            rusqlite::params![conv1],
        )
        .unwrap();
    guest_db
        .conn()
        .execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?1, ?2, 'user', 'Keep message', 100)",
            rusqlite::params![Uuid::new_v4().to_string(), conv1],
        )
        .unwrap();
    guest_db
        .conn()
        .execute(
            "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?1, 'Leave Me', 200, 200)",
            rusqlite::params![conv2],
        )
        .unwrap();
    guest_db
        .conn()
        .execute(
            "INSERT INTO memories (id, content, category, source, created_at, updated_at) VALUES (?1, 'Important Memory', 'fact', 'user', 100, 100)",
            rusqlite::params![mem1],
        )
        .unwrap();

    // Prepare profile importing only conv1 and mem1
    let scope = ScopeKey {
        service_origin: "https://cloud.example.com".into(),
        account_id: "user-selective".into(),
    };
    let plan = ProfileManager::prepare_profile(
        app_data,
        &scope,
        ActivationChoice::ImportSelectedGuest {
            entity_ids: vec![conv1.clone(), mem1.clone()],
        },
    )
    .unwrap();

    assert_eq!(plan.import_count, 2);

    let account_db_path = app_data
        .join("profiles")
        .join(&plan.profile_id)
        .join("app.db");
    let account_db = Database::new(&account_db_path).unwrap();

    // Verified: conv1 and mem1 were imported, conv2 was not
    let conv_count: i64 = account_db
        .conn()
        .query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0))
        .unwrap();
    let mem_count: i64 = account_db
        .conn()
        .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
        .unwrap();
    assert_eq!(conv_count, 1);
    assert_eq!(mem_count, 1);

    // Verify conv title is 'Keep Me'
    let title: String = account_db
        .conn()
        .query_row("SELECT title FROM conversations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(title, "Keep Me");

    // Imported conversation receives a NEW id, preserving privacy boundaries
    let imported_conv_id: String = account_db
        .conn()
        .query_row("SELECT id FROM conversations", [], |r| r.get(0))
        .unwrap();
    assert_ne!(imported_conv_id, conv1);
}

#[test]
fn test_s3_5_legacy_database_quarantine_with_sqlite_backup() {
    let tmp = TempDir::new();
    let app_data = tmp.path();

    // Create legacy database at root app-data
    let legacy_db_path = app_data.join("yilianqianyan-v1.db");
    {
        let db = Database::new(&legacy_db_path).unwrap();
        db.conn()
            .execute("CREATE TABLE legacy_test (val TEXT);", [])
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO legacy_test VALUES ('preserved_in_quarantine');",
                [],
            )
            .unwrap();
    }
    assert!(legacy_db_path.exists());

    // Resolve active profile triggers quarantine
    let (db_path, lease, profile_id) =
        ProfileManager::resolve_active_profile_db_path(app_data).unwrap();

    // The source remains recoverable and its consistent copy becomes the active guest profile.
    assert!(legacy_db_path.exists());
    assert!(db_path.exists());
    assert_ne!(db_path, legacy_db_path);
    assert!(profile_id.starts_with("guest-migrated-"));
    assert!(lease.scope_id.starts_with("guest-"));

    let active_conn = rusqlite::Connection::open(&db_path).unwrap();
    let active_value: String = active_conn
        .query_row("SELECT val FROM legacy_test", [], |row| row.get(0))
        .unwrap();
    assert_eq!(active_value, "preserved_in_quarantine");

    // Verify quarantine directory contains intact backup
    let quarantine_root = app_data.join("legacy-quarantine");
    assert!(quarantine_root.exists());
    let mut entries = std::fs::read_dir(&quarantine_root).unwrap();
    let first_entry = entries.next().unwrap().unwrap();
    let quarantined_db = first_entry.path().join("yilianqianyan-v1.db");
    assert!(quarantined_db.exists());

    let q_conn = rusqlite::Connection::open(&quarantined_db).unwrap();
    let val: String = q_conn
        .query_row("SELECT val FROM legacy_test", [], |r| r.get(0))
        .unwrap();
    assert_eq!(val, "preserved_in_quarantine");
}

#[tokio::test]
async fn test_s3_6_logout_and_revocation_service_transparent_reporting() {
    let tmp = TempDir::new();
    let db_path = tmp.path().join("test.db");
    let db = Database::new(&db_path).unwrap();
    db.initialize_cloud_schema().unwrap();

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let revocation_service = RevocationService::new(db.clone(), Arc::clone(&secret_store));

    let lease = ScopeLease {
        scope_id: "test-scope-1".into(),
        generation: "gen-1".into(),
    };

    // Store dummy tokens
    let access_ref = scoped_cloud_access_token_ref(&lease.scope_id);
    secret_store
        .put(
            &access_ref,
            secrecy::SecretString::from("token-12345678901234567890".to_string()),
        )
        .await
        .unwrap();

    // 1. Logout with no session_id -> immediate Confirmed
    let report_none = revocation_service.logout(&lease, None, None, None).await;
    assert!(report_none.local_signed_out);
    assert_eq!(report_none.remote_revocation, RemoteRevocation::Confirmed);

    // 2. Logout with an unreachable remote origin -> reported as Pending,
    // credentials stored in pending queue and revoke-only ref
    let session_id = Uuid::new_v4().to_string();
    secret_store
        .put(
            &access_ref,
            secrecy::SecretString::from("token-12345678901234567890".to_string()),
        )
        .await
        .unwrap();

    let report_unreachable = revocation_service
        .logout(
            &lease,
            Some(&session_id),
            Some("http://127.0.0.1:54321/api"), // Unreachable port
            Some("user-unreachable"),
        )
        .await;

    assert!(report_unreachable.local_signed_out);
    assert_eq!(
        report_unreachable.remote_revocation,
        RemoteRevocation::Pending,
        "Unreachable remote revocation must be reported as Pending"
    );

    // Normal access token was removed
    let token_after = secret_store.get(&access_ref).await.unwrap();
    assert!(
        token_after.is_none(),
        "Normal access token must be revoked locally immediately"
    );

    // Dedicated revoke-only token is preserved
    let revoke_token = secret_store
        .get(&revoke_session_token_ref(&session_id))
        .await
        .unwrap();
    assert!(
        revoke_token.is_some(),
        "Revoke-only token preserved for retry queue"
    );

    // Pending record is persisted in SQLite
    let pending_records = revocation_service
        .list_pending_revocations(chrono::Utc::now().timestamp_millis() + 100_000)
        .unwrap();
    assert_eq!(pending_records.len(), 1);
    assert_eq!(pending_records[0].session_id, session_id);
    assert_eq!(
        pending_records[0].last_error_code.as_deref(),
        Some("REMOTE_UNREACHABLE")
    );
}
