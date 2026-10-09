//! Transactional inert package inventory. Never loads or executes package code.
use crate::db::Database;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const MAX_PACKAGE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 64;
const STATE_KEY: &str = "v1.managed_imports.v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: String,
    pub source: String,
    pub permissions: Vec<String>,
    pub content_hash: String,
    pub files: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub package: Package,
    pub revision: u64,
    pub enabled: bool,
    pub installed: bool,
    pub history: Vec<Package>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportPreview {
    pub preview_id: String,
    pub package: Package,
    pub expected_revision: Option<u64>,
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub removed: Vec<String>,
    pub previous_permissions: Vec<String>,
    pub unchanged: bool,
    pub expires_at: i64,
}
#[derive(Default, Serialize, Deserialize)]
struct Inventory {
    records: BTreeMap<String, InstalledPackage>,
    previews: Vec<ImportPreview>,
}

pub fn markdown_package(
    id: &str,
    name: &str,
    version: &str,
    content: &str,
    source: &str,
) -> Result<Package, String> {
    let mut package = Package {
        id: id.into(),
        name: name.into(),
        version: version.into(),
        kind: "skill".into(),
        source: source.into(),
        permissions: vec!["skill.instructions.read".into()],
        content_hash: String::new(),
        files: BTreeMap::from([("SKILL.md".into(), content.into())]),
    };
    validate_package(&mut package)?;
    Ok(package)
}

pub fn validate_package(package: &mut Package) -> Result<(), String> {
    crate::plugin::manifest::PluginId::new(&package.id).map_err(|_| "invalid_package_id")?;
    if package.id.starts_with('.')
        || package.name.trim().is_empty()
        || package.name.chars().count() > 120
        || package.version.is_empty()
        || package.version.len() > 40
        || !package
            .version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        || !matches!(package.kind.as_str(), "skill" | "plugin")
        || package.permissions.len() > 64
        || package.files.is_empty()
        || package.files.len() > MAX_ENTRIES
    {
        return Err("invalid_package_metadata".into());
    }
    let mut seen = std::collections::HashSet::new();
    let mut total = 0usize;
    for (path, content) in &package.files {
        super::imports::normalize_package_path(path)?;
        if !seen.insert(path.to_ascii_lowercase())
            || !(path.ends_with(".md") || path.ends_with(".json") || path.ends_with(".txt"))
            || content.len() > 512 * 1024
            || content.contains('\0')
        {
            return Err("unsupported_or_duplicate_package_file".into());
        }
        total = total
            .checked_add(content.len())
            .ok_or("package_too_large")?;
    }
    if total == 0 || total > MAX_PACKAGE_BYTES {
        return Err("package_too_large".into());
    }
    if package.kind == "skill"
        && package
            .files
            .get("SKILL.md")
            .is_none_or(|content| content.trim().is_empty())
    {
        return Err("missing_skill_markdown".into());
    }
    let content = serde_json::to_vec(&(
        &package.id,
        &package.name,
        &package.version,
        &package.kind,
        &package.permissions,
        &package.files,
    ))
    .map_err(|_| "invalid_package")?;
    package.content_hash = format!("{:x}", Sha256::digest(content));
    Ok(())
}

fn read_state(conn: &rusqlite::Connection) -> Result<Inventory, String> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [STATE_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "import_store_unavailable")?;
    value
        .map(|value| serde_json::from_str(&value).map_err(|_| "import_store_invalid".into()))
        .unwrap_or_else(|| Ok(Inventory::default()))
}
fn transact<T>(
    db: &Database,
    operation: impl FnOnce(&mut Inventory) -> Result<T, String>,
) -> Result<T, String> {
    let mut conn = db.conn();
    let transaction = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| "import_store_unavailable")?;
    let mut state = read_state(&transaction)?;
    let result = operation(&mut state)?;
    let value = serde_json::to_string(&state).map_err(|_| "invalid_import_state")?;
    if value.len() > 16 * 1024 * 1024 {
        return Err("import_storage_limit".into());
    }
    transaction.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", rusqlite::params![STATE_KEY, value]).map_err(|_| "import_transaction_failed")?;
    transaction
        .commit()
        .map_err(|_| "import_transaction_failed")?;
    Ok(result)
}

pub fn inspect(db: &Database, mut package: Package, now: i64) -> Result<ImportPreview, String> {
    validate_package(&mut package)?;
    transact(db, |state| {
        state.previews.retain(|preview| preview.expires_at > now);
        if state.previews.len() >= 4 {
            return Err("too_many_pending_previews".into());
        }
        let current = state.records.get(&package.id);
        let before = current.map(|record| &record.package.files);
        let preview = ImportPreview {
            preview_id: uuid::Uuid::new_v4().to_string(),
            expected_revision: current.map(|record| record.revision),
            added: package
                .files
                .keys()
                .filter(|path| before.is_none_or(|files| !files.contains_key(*path)))
                .cloned()
                .collect(),
            changed: package
                .files
                .iter()
                .filter(|(path, value)| {
                    before
                        .and_then(|files| files.get(*path))
                        .is_some_and(|old| old != *value)
                })
                .map(|(path, _)| path.clone())
                .collect(),
            removed: before
                .map(|files| {
                    files
                        .keys()
                        .filter(|path| !package.files.contains_key(*path))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
            previous_permissions: current
                .map(|record| record.package.permissions.clone())
                .unwrap_or_default(),
            unchanged: current.is_some_and(|record| {
                record.installed && record.package.content_hash == package.content_hash
            }),
            package,
            expires_at: now.saturating_add(10 * 60 * 1000),
        };
        state.previews.push(preview.clone());
        Ok(preview)
    })
}

pub fn confirm(db: &Database, preview_id: &str, now: i64) -> Result<InstalledPackage, String> {
    transact(db, |state| {
        let index = state
            .previews
            .iter()
            .position(|preview| preview.preview_id == preview_id && preview.expires_at > now)
            .ok_or("preview_missing_or_expired")?;
        let preview = state.previews[index].clone();
        let current = state.records.get(&preview.package.id);
        if current.map(|record| record.revision) != preview.expected_revision {
            return Err("stale_import_preview".into());
        }
        if current.is_none() && state.records.len() >= 64 {
            return Err("installed_package_limit".into());
        }
        let installed = if preview.unchanged {
            current.cloned().ok_or("stale_import_preview")?
        } else {
            let mut history = current
                .map(|record| record.history.clone())
                .unwrap_or_default();
            if let Some(record) = current {
                history.push(record.package.clone());
            }
            if history.len() > 2 {
                history.remove(0);
            }
            InstalledPackage {
                package: preview.package,
                revision: current.map_or(1, |record| record.revision + 1),
                enabled: false,
                installed: true,
                history,
            }
        };
        state.previews.remove(index);
        state
            .records
            .insert(installed.package.id.clone(), installed.clone());
        Ok(installed)
    })
}

pub fn change(
    db: &Database,
    id: &str,
    revision: u64,
    action: &str,
) -> Result<InstalledPackage, String> {
    transact(db, |state| {
        let record = state.records.get_mut(id).ok_or("package_not_found")?;
        if record.revision != revision {
            return Err("stale_package_revision".into());
        }
        match action {
            "enable" if record.installed => record.enabled = true,
            "disable" if record.installed => record.enabled = false,
            "uninstall" if record.installed => {
                record.installed = false;
                record.enabled = false;
            }
            "restore" if !record.installed => {
                record.installed = true;
                record.enabled = false;
            }
            "rollback" if record.installed => {
                let previous = record.history.pop().ok_or("no_previous_version")?;
                let replaced = std::mem::replace(&mut record.package, previous);
                record.history.push(replaced);
                record.enabled = false;
            }
            _ => return Err("invalid_import_action".into()),
        }
        record.revision = record.revision.checked_add(1).ok_or("revision_overflow")?;
        Ok(record.clone())
    })
}

pub fn list(db: &Database) -> Result<Vec<InstalledPackage>, String> {
    Ok(read_state(&db.conn())?.records.into_values().collect())
}
pub fn get(db: &Database, id: &str) -> Result<Option<InstalledPackage>, String> {
    Ok(read_state(&db.conn())?.records.get(id).cloned())
}
pub fn get_preview(db: &Database, preview_id: &str, now: i64) -> Result<ImportPreview, String> {
    read_state(&db.conn())?
        .previews
        .into_iter()
        .find(|preview| preview.preview_id == preview_id && preview.expires_at > now)
        .ok_or_else(|| "preview_missing_or_expired".into())
}
pub fn enabled_skill(db: &Database, id: &str) -> Result<Option<String>, String> {
    Ok(read_state(&db.conn())?
        .records
        .get(id)
        .filter(|record| record.installed && record.enabled && record.package.kind == "skill")
        .and_then(|record| record.package.files.get("SKILL.md").cloned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Database {
        Database::new(std::path::Path::new(":memory:")).unwrap()
    }
    fn package(content: &str) -> Package {
        markdown_package("demo", "Demo", "1.0.0", content, "markdown").unwrap()
    }
    #[test]
    fn import_preview_confirm_update_disable_uninstall_and_restore_are_transactional() {
        let db = db();
        let first = inspect(
            &db,
            package("# Inert instructions\nDo not run these during inspection."),
            1,
        )
        .unwrap();
        assert!(list(&db).unwrap().is_empty());
        let installed = confirm(&db, &first.preview_id, 2).unwrap();
        assert!(!installed.enabled);
        assert!(confirm(&db, &first.preview_id, 3).is_err());
        let enabled = change(&db, "demo", installed.revision, "enable").unwrap();
        let preview = inspect(&db, package("# Changed"), 4).unwrap();
        assert_eq!(preview.changed, vec!["SKILL.md"]);
        let updated = confirm(&db, &preview.preview_id, 5).unwrap();
        assert!(!updated.enabled);
        assert_eq!(updated.history.len(), 1);
        assert!(change(&db, "demo", enabled.revision, "uninstall").is_err());
        let removed = change(&db, "demo", updated.revision, "uninstall").unwrap();
        assert!(!removed.installed);
        assert!(!removed.enabled);
        let restored = change(&db, "demo", removed.revision, "restore").unwrap();
        assert!(restored.installed);
        let rolled_back = change(&db, "demo", restored.revision, "rollback").unwrap();
        assert!(rolled_back.package.files["SKILL.md"].contains("Inert"));
    }
    #[test]
    fn stale_preview_dedup_and_persistence_failure_leave_inventory_unchanged() {
        let db = db();
        let first = inspect(&db, package("# first"), 1).unwrap();
        let competing = inspect(&db, package("# second"), 1).unwrap();
        let installed = confirm(&db, &first.preview_id, 2).unwrap();
        assert!(confirm(&db, &competing.preview_id, 3).is_err());
        let same = inspect(&db, package("# first"), 4).unwrap();
        assert!(same.unchanged);
        assert_eq!(
            confirm(&db, &same.preview_id, 5).unwrap().revision,
            installed.revision
        );
        let before = serde_json::to_value(list(&db).unwrap()).unwrap();
        db.conn().execute_batch("CREATE TRIGGER fail_import_write BEFORE UPDATE ON settings BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
        assert!(change(&db, "demo", installed.revision, "enable").is_err());
        assert_eq!(serde_json::to_value(list(&db).unwrap()).unwrap(), before);
    }
}
