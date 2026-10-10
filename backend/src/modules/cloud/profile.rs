use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::state::AppServer;
use crate::db::Database;
use crate::modules::cloud::contracts::{canonical_scope, ScopeError, ScopeKey, ScopeLease};

pub const PROFILE_REGISTRY_FILENAME: &str = "profile-registry.json";
pub const PROFILES_DIRNAME: &str = "profiles";
pub const LEGACY_QUARANTINE_DIRNAME: &str = "legacy-quarantine";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActivationChoice {
    OpenAccountOnly,
    ImportSelectedGuest { entity_ids: Vec<String> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivationPlan {
    pub profile_id: String,
    pub lease: ScopeLease,
    pub requires_restart: bool,
    pub import_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchState {
    Idle,
    Quiescing,
    Prepared,
    AwaitingRestart,
    Active,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileKind {
    Guest,
    Account,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileEntry {
    pub profile_id: String,
    pub kind: ProfileKind,
    pub scope_key: Option<ScopeKey>,
    pub scope_id: String,
    pub directory_name: String,
    pub created_at: i64,
    pub last_activated_at: i64,
    #[serde(default)]
    pub reauthentication_required: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationJournalStatus {
    Pending,
    Committed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingActivationJournal {
    pub target_profile_id: String,
    pub target_generation: String,
    pub prepared_at: i64,
    pub status: ActivationJournalStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileRegistry {
    pub schema_version: u32,
    pub active_profile_id: String,
    pub active_generation: String,
    pub profiles: BTreeMap<String, ProfileEntry>,
    pub pending_activation: Option<PendingActivationJournal>,
}

impl Default for ProfileRegistry {
    fn default() -> Self {
        Self {
            schema_version: 1,
            active_profile_id: String::new(),
            active_generation: String::new(),
            profiles: BTreeMap::new(),
            pending_activation: None,
        }
    }
}

pub struct ProfileManager;

impl ProfileManager {
    pub fn profile_db_path(app_data_dir: &Path, plan: &ActivationPlan) -> PathBuf {
        app_data_dir
            .join(PROFILES_DIRNAME)
            .join(&plan.profile_id)
            .join("app.db")
    }

    pub fn prepare_guest_activation(app_data_dir: &Path) -> Result<ActivationPlan, ScopeError> {
        let registry = Self::load_or_initialize(app_data_dir)?;
        let entry = registry
            .profiles
            .values()
            .find(|entry| entry.kind == ProfileKind::Guest)
            .ok_or_else(|| ScopeError::MigrationFailed("guest profile is missing".into()))?;
        Ok(ActivationPlan {
            profile_id: entry.profile_id.clone(),
            lease: ScopeLease {
                scope_id: entry.scope_id.clone(),
                generation: Uuid::new_v4().to_string(),
            },
            requires_restart: true,
            import_count: 0,
        })
    }

    /// Load or initialize the profile registry on disk.
    /// If no active profile exists, creates a fresh guest profile and activates it.
    pub fn load_or_initialize(app_data_dir: &Path) -> Result<ProfileRegistry, ScopeError> {
        let registry_path = app_data_dir.join(PROFILE_REGISTRY_FILENAME);
        let profiles_dir = app_data_dir.join(PROFILES_DIRNAME);
        std::fs::create_dir_all(&profiles_dir)
            .map_err(|e| ScopeError::Io(format!("create profiles dir: {e}")))?;

        if registry_path.exists() {
            let data = std::fs::read_to_string(&registry_path)
                .map_err(|e| ScopeError::Io(format!("read profile registry: {e}")))?;
            let mut registry: ProfileRegistry = serde_json::from_str(&data)
                .map_err(|e| ScopeError::Io(format!("parse profile registry: {e}")))?;

            // If pending activation was left, check its state
            if let Some(journal) = registry.pending_activation.take() {
                if journal.status == ActivationJournalStatus::Committed {
                    registry.active_profile_id = journal.target_profile_id;
                    registry.active_generation = journal.target_generation;
                }
                Self::save_registry(app_data_dir, &registry)?;
            }

            if !registry.active_profile_id.is_empty()
                && registry.profiles.contains_key(&registry.active_profile_id)
            {
                return Ok(registry);
            }
        }

        // Initialize a new default guest profile
        let guest_uuid = Uuid::new_v4().to_string();
        let guest_profile_id = format!("guest-{guest_uuid}");
        let guest_generation = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();

        let guest_dir = profiles_dir.join(&guest_profile_id);
        Self::ensure_profile_directories(&guest_dir)?;

        let guest_db_path = guest_dir.join("app.db");
        let db = Database::new(&guest_db_path)
            .map_err(|e| ScopeError::MigrationFailed(format!("init guest db: {e}")))?;
        db.initialize_cloud_schema()
            .map_err(|e| ScopeError::MigrationFailed(format!("init guest cloud schema: {e}")))?;

        let guest_scope_id = format!("guest-{guest_uuid}");
        let scoped_db = crate::db::cloud::scoped_db::ScopedDb::new(
            db,
            ScopeLease {
                scope_id: guest_scope_id.clone(),
                generation: guest_generation.clone(),
            },
        );
        scoped_db
            .init_profile_metadata("local://guest", &guest_uuid)
            .map_err(|e| {
                ScopeError::MigrationFailed(format!("init guest profile metadata: {e}"))
            })?;

        let guest_entry = ProfileEntry {
            profile_id: guest_profile_id.clone(),
            kind: ProfileKind::Guest,
            scope_key: None,
            scope_id: guest_scope_id,
            directory_name: guest_profile_id.clone(),
            created_at: now,
            last_activated_at: now,
            reauthentication_required: false,
        };

        let mut registry = ProfileRegistry {
            schema_version: 1,
            active_profile_id: guest_profile_id.clone(),
            active_generation: guest_generation,
            profiles: BTreeMap::new(),
            pending_activation: None,
        };
        registry.profiles.insert(guest_profile_id, guest_entry);

        Self::save_registry(app_data_dir, &registry)?;
        Ok(registry)
    }

    /// Atomically save registry JSON with .tmp replace.
    pub fn save_registry(
        app_data_dir: &Path,
        registry: &ProfileRegistry,
    ) -> Result<(), ScopeError> {
        let registry_path = app_data_dir.join(PROFILE_REGISTRY_FILENAME);
        let tmp_path = app_data_dir.join(format!("{PROFILE_REGISTRY_FILENAME}.tmp"));
        let json = serde_json::to_string_pretty(registry)
            .map_err(|e| ScopeError::Io(format!("serialize registry: {e}")))?;
        std::fs::write(&tmp_path, json)
            .map_err(|e| ScopeError::Io(format!("write tmp registry: {e}")))?;
        std::fs::rename(&tmp_path, &registry_path)
            .map_err(|e| ScopeError::Io(format!("atomic rename registry: {e}")))?;
        Ok(())
    }

    /// Set reauthentication_required flag for a given profile.
    pub fn set_reauthentication_required(
        app_data_dir: &Path,
        profile_id: &str,
        required: bool,
    ) -> Result<(), ScopeError> {
        let mut registry = Self::load_or_initialize(app_data_dir)?;
        if let Some(entry) = registry.profiles.get_mut(profile_id) {
            entry.reauthentication_required = required;
            Self::save_registry(app_data_dir, &registry)?;
        }
        Ok(())
    }

    /// Ensure standard subdirectories exist within a profile directory.
    pub fn ensure_profile_directories(profile_dir: &Path) -> Result<(), ScopeError> {
        for sub in &["resources", "indexes", "cache"] {
            std::fs::create_dir_all(profile_dir.join(sub))
                .map_err(|e| ScopeError::Io(format!("create subfolder {sub}: {e}")))?;
        }
        Ok(())
    }

    /// Prepare an account profile without switching runtime.
    pub fn prepare_profile(
        app_data_dir: &Path,
        scope_key: &ScopeKey,
        choice: ActivationChoice,
    ) -> Result<ActivationPlan, ScopeError> {
        let (canonical_key, scope_id) =
            canonical_scope(&scope_key.service_origin, &scope_key.account_id)?;

        let mut registry = Self::load_or_initialize(app_data_dir)?;
        let profile_id = format!("account-{scope_id}");
        let profiles_dir = app_data_dir.join(PROFILES_DIRNAME);
        let profile_dir = profiles_dir.join(&profile_id);
        Self::ensure_profile_directories(&profile_dir)?;

        let db_path = profile_dir.join("app.db");
        let db = Database::new(&db_path)
            .map_err(|e| ScopeError::MigrationFailed(format!("open account db: {e}")))?;
        db.initialize_cloud_schema()
            .map_err(|e| ScopeError::MigrationFailed(format!("init account cloud schema: {e}")))?;

        let generation = Uuid::new_v4().to_string();
        let lease = ScopeLease {
            scope_id: scope_id.clone(),
            generation: generation.clone(),
        };

        let scoped_db = crate::db::cloud::scoped_db::ScopedDb::new(db.clone(), lease.clone());
        scoped_db
            .init_profile_metadata(&canonical_key.service_origin, &canonical_key.account_id)
            .map_err(|e| ScopeError::MigrationFailed(format!("init account metadata: {e}")))?;

        let mut import_count = 0;
        if let ActivationChoice::ImportSelectedGuest { ref entity_ids } = choice {
            if !entity_ids.is_empty() {
                if let Some(active_entry) = registry.profiles.get(&registry.active_profile_id) {
                    if active_entry.kind == ProfileKind::Guest {
                        let guest_db_path = profiles_dir
                            .join(&active_entry.directory_name)
                            .join("app.db");
                        if guest_db_path.exists() {
                            let guest_db = Database::new(&guest_db_path).map_err(|e| {
                                ScopeError::MigrationFailed(format!(
                                    "open guest db for import: {e}"
                                ))
                            })?;
                            import_count = Self::import_guest_entities(&guest_db, &db, entity_ids)?;
                        }
                    }
                }
            }
        }

        let now = chrono::Utc::now().timestamp_millis();
        let existing = registry.profiles.get(&profile_id);
        let reauth_req = existing
            .map(|e| e.reauthentication_required)
            .unwrap_or(false);
        let created_at = existing.map(|e| e.created_at).unwrap_or(now);
        let entry = ProfileEntry {
            profile_id: profile_id.clone(),
            kind: ProfileKind::Account,
            scope_key: Some(canonical_key),
            scope_id,
            directory_name: profile_id.clone(),
            created_at,
            last_activated_at: now,
            reauthentication_required: reauth_req,
        };
        registry.profiles.insert(profile_id.clone(), entry);
        Self::save_registry(app_data_dir, &registry)?;

        Ok(ActivationPlan {
            profile_id,
            lease,
            requires_restart: true,
            import_count,
        })
    }

    /// Import selected entities (conversations, memories) from guest DB to account DB.
    fn import_guest_entities(
        guest_db: &Database,
        target_db: &Database,
        entity_ids: &[String],
    ) -> Result<usize, ScopeError> {
        let guest_conn = guest_db.conn();
        let mut target_conn = target_db.conn();
        let tx = target_conn
            .transaction()
            .map_err(|e| ScopeError::MigrationFailed(format!("target tx start: {e}")))?;

        let mut imported = 0;
        for id in entity_ids {
            // Check conversation
            let conv_row: Option<(String, i64, i64)> = guest_conn
                .query_row(
                    "SELECT title, created_at, updated_at FROM conversations WHERE id=?1",
                    rusqlite::params![id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(|e| ScopeError::MigrationFailed(format!("query guest conv: {e}")))?;

            if let Some((title, created_at, updated_at)) = conv_row {
                let new_id = Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![new_id, title, created_at, updated_at],
                ).map_err(|e| ScopeError::MigrationFailed(format!("insert conv: {e}")))?;

                // Copy messages belonging to this conversation
                let mut stmt = guest_conn
                    .prepare("SELECT role, content, tool_calls, tool_call_id, tool_name, tool_result, created_at FROM messages WHERE conversation_id=?1 ORDER BY created_at ASC")
                    .map_err(|e| ScopeError::MigrationFailed(format!("prepare guest msgs: {e}")))?;
                let msg_rows = stmt
                    .query_map(rusqlite::params![id], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, i64>(6)?,
                        ))
                    })
                    .map_err(|e| ScopeError::MigrationFailed(format!("query guest msgs: {e}")))?;

                for msg in msg_rows {
                    let (
                        role,
                        content,
                        tool_calls,
                        tool_call_id,
                        tool_name,
                        tool_result,
                        msg_created_at,
                    ) = msg.map_err(|e| ScopeError::MigrationFailed(format!("read msg: {e}")))?;
                    let new_msg_id = Uuid::new_v4().to_string();
                    tx.execute(
                        "INSERT INTO messages (id, conversation_id, role, content, tool_calls, tool_call_id, tool_name, tool_result, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        rusqlite::params![new_msg_id, new_id, role, content, tool_calls, tool_call_id, tool_name, tool_result, msg_created_at],
                    ).map_err(|e| ScopeError::MigrationFailed(format!("insert msg: {e}")))?;
                }
                imported += 1;
                continue;
            }

            // Check memory
            let mem_row: Option<(String, String, String, i64, i64)> = guest_conn
                .query_row(
                    "SELECT content, category, source, created_at, updated_at FROM memories WHERE id=?1",
                    rusqlite::params![id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                )
                .optional()
                .map_err(|e| ScopeError::MigrationFailed(format!("query guest memory: {e}")))?;

            if let Some((content, category, source, created_at, updated_at)) = mem_row {
                let new_mem_id = Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO memories (id, content, category, source, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![new_mem_id, content, category, source, created_at, updated_at],
                ).map_err(|e| ScopeError::MigrationFailed(format!("insert memory: {e}")))?;
                imported += 1;
            }
        }

        tx.commit()
            .map_err(|e| ScopeError::MigrationFailed(format!("commit imported entities: {e}")))?;
        Ok(imported)
    }

    /// Stop new requests, cancel in-flight tasks and executions, and wait for them to finish.
    pub async fn quiesce_runtime(server: &AppServer, timeout: Duration) -> Result<(), ScopeError> {
        // Cancel all active conversation tasks
        {
            let tasks = server.active_tasks.lock();
            for token in tasks.values() {
                token.cancel();
            }
        }
        // Cancel active workflow runs
        {
            let workflow_runs = server.active_workflow_runs.lock();
            for token in workflow_runs.values() {
                token.cancel();
            }
        }
        // Cancel active task executions
        {
            let task_execs = server.active_task_executions.lock();
            for token in task_execs.values() {
                token.cancel();
            }
        }

        // Shutdown MCP and managed processes
        server.mcp_runtime_manager.shutdown_all().await;
        server.managed_process_registry.shutdown_all();

        // Wait with bounded timeout to ensure active operations terminate
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            let tasks_empty = server.active_tasks.lock().is_empty();
            let runs_empty = server.active_workflow_runs.lock().is_empty();
            let execs_empty = server.active_task_executions.lock().is_empty();
            if tasks_empty && runs_empty && execs_empty {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Err(ScopeError::QuiesceTimeout)
    }

    /// Persist activation plan into registry with atomic journal.
    pub fn persist_activation(
        app_data_dir: &Path,
        plan: &ActivationPlan,
    ) -> Result<(), ScopeError> {
        let mut registry = Self::load_or_initialize(app_data_dir)?;
        let now = chrono::Utc::now().timestamp_millis();

        // Write pending journal first
        registry.pending_activation = Some(PendingActivationJournal {
            target_profile_id: plan.profile_id.clone(),
            target_generation: plan.lease.generation.clone(),
            prepared_at: now,
            status: ActivationJournalStatus::Pending,
        });
        Self::save_registry(app_data_dir, &registry)?;

        // Now commit journal and update active profile
        if let Some(entry) = registry.profiles.get_mut(&plan.profile_id) {
            entry.last_activated_at = now;
        }
        registry.active_profile_id = plan.profile_id.clone();
        registry.active_generation = plan.lease.generation.clone();
        if let Some(ref mut journal) = registry.pending_activation {
            journal.status = ActivationJournalStatus::Committed;
        }
        Self::save_registry(app_data_dir, &registry)?;

        Ok(())
    }

    /// Full profile activation orchestration: prepare -> quiesce -> journal -> commit.
    pub async fn activate_profile(
        app_data_dir: &Path,
        server: &AppServer,
        scope_key: &ScopeKey,
        choice: ActivationChoice,
        timeout: Duration,
    ) -> Result<ActivationPlan, ScopeError> {
        let plan = Self::prepare_profile(app_data_dir, scope_key, choice)?;
        Self::activate_prepared_profile(app_data_dir, server, &plan, timeout).await?;
        Ok(plan)
    }

    pub async fn activate_prepared_profile(
        app_data_dir: &Path,
        server: &AppServer,
        plan: &ActivationPlan,
        timeout: Duration,
    ) -> Result<(), ScopeError> {
        Self::quiesce_runtime(server, timeout).await?;
        let previous = server.scoped_db.lease();
        server.scoped_db.activate_lease(plan.lease.clone());
        if let Err(error) = Self::persist_activation(app_data_dir, plan) {
            server.scoped_db.activate_lease(previous);
            return Err(error);
        }
        Ok(())
    }

    /// Resolves the active profile's `app.db` path and ScopeLease.
    /// Ensures legacy quarantine is checked, registry is loaded, and returns active db path.
    pub fn resolve_active_profile_db_path(
        app_data_dir: &Path,
    ) -> Result<(PathBuf, ScopeLease, String), ScopeError> {
        let registry_path = app_data_dir.join(PROFILE_REGISTRY_FILENAME);
        if !registry_path.exists() {
            if let Some(backup_path) = Self::quarantine_legacy_db_if_needed(app_data_dir)? {
                let guest_uuid = Uuid::new_v4().to_string();
                let profile_id = format!("guest-migrated-{guest_uuid}");
                let generation = Uuid::new_v4().to_string();
                let scope_id = format!("guest-{guest_uuid}");
                let profile_dir = app_data_dir.join(PROFILES_DIRNAME).join(&profile_id);
                Self::ensure_profile_directories(&profile_dir)?;
                let db_path = profile_dir.join("app.db");
                super::profile_migration::backup_sqlite_db(&backup_path, &db_path).map_err(
                    |error| ScopeError::MigrationFailed(format!("adopt legacy database: {error}")),
                )?;
                let db = Database::new(&db_path).map_err(|error| {
                    ScopeError::MigrationFailed(format!("migrate adopted database: {error}"))
                })?;
                let scoped = crate::db::cloud::scoped_db::ScopedDb::new(
                    db,
                    ScopeLease {
                        scope_id: scope_id.clone(),
                        generation: generation.clone(),
                    },
                );
                scoped
                    .init_profile_metadata("local://guest", &guest_uuid)
                    .map_err(|error| {
                        ScopeError::MigrationFailed(format!("scope adopted database: {error}"))
                    })?;
                let now = chrono::Utc::now().timestamp_millis();
                let mut profiles = BTreeMap::new();
                profiles.insert(
                    profile_id.clone(),
                    ProfileEntry {
                        profile_id: profile_id.clone(),
                        kind: ProfileKind::Guest,
                        scope_key: None,
                        scope_id,
                        directory_name: profile_id.clone(),
                        created_at: now,
                        last_activated_at: now,
                        reauthentication_required: false,
                    },
                );
                Self::save_registry(
                    app_data_dir,
                    &ProfileRegistry {
                        schema_version: 1,
                        active_profile_id: profile_id,
                        active_generation: generation,
                        profiles,
                        pending_activation: None,
                    },
                )?;
            }
        }
        let mut registry = Self::load_or_initialize(app_data_dir)?;
        let mut registry_changed = false;

        // Backward compatibility: detect historically logged-out accounts
        for entry in registry.profiles.values_mut() {
            if entry.kind == ProfileKind::Account && !entry.reauthentication_required {
                let db_path = app_data_dir
                    .join(PROFILES_DIRNAME)
                    .join(&entry.directory_name)
                    .join("app.db");
                if db_path.exists() {
                    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                        let is_logged_out: bool = conn
                            .query_row(
                                "SELECT 1 FROM cloud_identity WHERE singleton=1 AND status='offline_authenticated' AND current_session_id IS NULL",
                                [],
                                |_| Ok(true),
                            )
                            .unwrap_or(false);
                        if is_logged_out {
                            entry.reauthentication_required = true;
                            registry_changed = true;
                        }
                    }
                }
            }
        }

        let active_needs_reauth = registry
            .profiles
            .get(&registry.active_profile_id)
            .map(|e| e.reauthentication_required)
            .unwrap_or(false);

        if active_needs_reauth {
            let guest_entry_opt = registry
                .profiles
                .values()
                .find(|e| e.kind == ProfileKind::Guest)
                .cloned();

            let guest_entry = match guest_entry_opt {
                Some(g) => g,
                None => {
                    let guest_uuid = Uuid::new_v4().to_string();
                    let guest_profile_id = format!("guest-{guest_uuid}");
                    let now = chrono::Utc::now().timestamp_millis();
                    let guest_dir = app_data_dir.join(PROFILES_DIRNAME).join(&guest_profile_id);
                    Self::ensure_profile_directories(&guest_dir)?;
                    let db_path = guest_dir.join("app.db");
                    let db = Database::new(&db_path)
                        .map_err(|e| ScopeError::MigrationFailed(format!("init guest db: {e}")))?;
                    db.initialize_cloud_schema().map_err(|e| {
                        ScopeError::MigrationFailed(format!("init guest cloud schema: {e}"))
                    })?;
                    let scoped = crate::db::cloud::scoped_db::ScopedDb::new(
                        db,
                        ScopeLease {
                            scope_id: guest_profile_id.clone(),
                            generation: guest_uuid.clone(),
                        },
                    );
                    scoped
                        .init_profile_metadata("local://guest", &guest_uuid)
                        .map_err(|e| {
                            ScopeError::MigrationFailed(format!("init guest meta: {e}"))
                        })?;
                    let g = ProfileEntry {
                        profile_id: guest_profile_id.clone(),
                        kind: ProfileKind::Guest,
                        scope_key: None,
                        scope_id: guest_profile_id.clone(),
                        directory_name: guest_profile_id.clone(),
                        created_at: now,
                        last_activated_at: now,
                        reauthentication_required: false,
                    };
                    registry.profiles.insert(guest_profile_id, g.clone());
                    g
                }
            };

            registry.active_profile_id = guest_entry.profile_id.clone();
            registry.active_generation = Uuid::new_v4().to_string();
            registry_changed = true;
        }

        if registry_changed {
            Self::save_registry(app_data_dir, &registry)?;
        }

        let active_entry = registry
            .profiles
            .get(&registry.active_profile_id)
            .ok_or_else(|| {
                ScopeError::MigrationFailed(format!(
                    "Active profile entry not found: {}",
                    registry.active_profile_id
                ))
            })?;

        let profile_dir = app_data_dir
            .join(PROFILES_DIRNAME)
            .join(&active_entry.directory_name);
        Self::ensure_profile_directories(&profile_dir)?;

        let db_path = profile_dir.join("app.db");
        let lease = ScopeLease {
            scope_id: active_entry.scope_id.clone(),
            generation: registry.active_generation.clone(),
        };

        Ok((db_path, lease, registry.active_profile_id))
    }
}
