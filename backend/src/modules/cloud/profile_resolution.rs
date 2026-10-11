use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use uuid::Uuid;

use crate::{
    db::Database,
    modules::cloud::contracts::{ScopeError, ScopeLease},
};

use super::profile::{
    ProfileEntry, ProfileKind, ProfileManager, ProfileRegistry, PROFILES_DIRNAME,
    PROFILE_REGISTRY_FILENAME,
};

pub(super) fn resolve_active_profile_db_path(
    app_data_dir: &Path,
) -> Result<(PathBuf, ScopeLease, String), ScopeError> {
    let registry_path = app_data_dir.join(PROFILE_REGISTRY_FILENAME);
    if !registry_path.exists() {
        if let Some(backup_path) = ProfileManager::quarantine_legacy_db_if_needed(app_data_dir)? {
            let guest_uuid = Uuid::new_v4().to_string();
            let profile_id = format!("guest-migrated-{guest_uuid}");
            let generation = Uuid::new_v4().to_string();
            let scope_id = format!("guest-{guest_uuid}");
            let profile_dir = app_data_dir.join(PROFILES_DIRNAME).join(&profile_id);
            ProfileManager::ensure_profile_directories(&profile_dir)?;
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
            ProfileManager::save_registry(
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
    let mut registry = ProfileManager::load_or_initialize(app_data_dir)?;
    let mut registry_changed = false;

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
        .map(|entry| entry.reauthentication_required)
        .unwrap_or(false);
    if active_needs_reauth {
        let guest_entry = match registry
            .profiles
            .values()
            .find(|entry| entry.kind == ProfileKind::Guest)
            .cloned()
        {
            Some(guest) => guest,
            None => create_guest_profile(app_data_dir, &mut registry)?,
        };
        registry.active_profile_id = guest_entry.profile_id;
        registry.active_generation = Uuid::new_v4().to_string();
        registry_changed = true;
    }

    if registry_changed {
        ProfileManager::save_registry(app_data_dir, &registry)?;
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
    ProfileManager::ensure_profile_directories(&profile_dir)?;
    let db_path = profile_dir.join("app.db");
    let lease = ScopeLease {
        scope_id: active_entry.scope_id.clone(),
        generation: registry.active_generation.clone(),
    };
    Ok((db_path, lease, registry.active_profile_id))
}

fn create_guest_profile(
    app_data_dir: &Path,
    registry: &mut ProfileRegistry,
) -> Result<ProfileEntry, ScopeError> {
    let guest_uuid = Uuid::new_v4().to_string();
    let profile_id = format!("guest-{guest_uuid}");
    let now = chrono::Utc::now().timestamp_millis();
    let profile_dir = app_data_dir.join(PROFILES_DIRNAME).join(&profile_id);
    ProfileManager::ensure_profile_directories(&profile_dir)?;
    let db_path = profile_dir.join("app.db");
    let db = Database::new(&db_path)
        .map_err(|error| ScopeError::MigrationFailed(format!("init guest db: {error}")))?;
    db.initialize_cloud_schema().map_err(|error| {
        ScopeError::MigrationFailed(format!("init guest cloud schema: {error}"))
    })?;
    let scoped = crate::db::cloud::scoped_db::ScopedDb::new(
        db,
        ScopeLease {
            scope_id: profile_id.clone(),
            generation: guest_uuid.clone(),
        },
    );
    scoped
        .init_profile_metadata("local://guest", &guest_uuid)
        .map_err(|error| ScopeError::MigrationFailed(format!("init guest meta: {error}")))?;
    let entry = ProfileEntry {
        profile_id: profile_id.clone(),
        kind: ProfileKind::Guest,
        scope_key: None,
        scope_id: profile_id.clone(),
        directory_name: profile_id.clone(),
        created_at: now,
        last_activated_at: now,
        reauthentication_required: false,
    };
    registry.profiles.insert(profile_id, entry.clone());
    Ok(entry)
}
