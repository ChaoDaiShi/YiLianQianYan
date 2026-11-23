use super::model::*;
use crate::db::Database;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const KEY: &str = "software_mount.inventory.v1";
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Receipt {
    pub fingerprint: String,
    pub record: OperationRecord,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Inventory {
    pub schema_version: u32,
    pub revision: u64,
    pub registrations: BTreeMap<String, Registration>,
    pub sessions: BTreeMap<String, Session>,
    pub operations: BTreeMap<String, Receipt>,
}
impl Default for Inventory {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            registrations: BTreeMap::new(),
            sessions: BTreeMap::new(),
            operations: BTreeMap::new(),
        }
    }
}
fn db_error(error: impl std::fmt::Display) -> SoftwareError {
    SoftwareError::new("software_storage_unavailable", error.to_string())
}
pub(super) fn load(db: &Database) -> Result<Inventory, SoftwareError> {
    let raw: Option<String> = db
        .conn()
        .query_row("SELECT value FROM settings WHERE key=?1", [KEY], |r| {
            r.get(0)
        })
        .optional()
        .map_err(db_error)?;
    match raw {
        None => Ok(Inventory::default()),
        Some(raw) => {
            if raw.len() > 8 * 1024 * 1024 {
                return Err(db_error("Software inventory exceeds its storage limit"));
            }
            let state: Inventory = serde_json::from_str(&raw).map_err(db_error)?;
            if state.schema_version != 1 {
                return Err(db_error("Unsupported software inventory version"));
            }
            Ok(state)
        }
    }
}
/// Compare-and-swap the whole bounded projection under an immediate transaction.
pub(super) fn save(db: &Database, before: u64, next: &mut Inventory) -> Result<(), SoftwareError> {
    let mut conn = db.conn();
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let raw: Option<String> = tx
        .query_row("SELECT value FROM settings WHERE key=?1", [KEY], |r| {
            r.get(0)
        })
        .optional()
        .map_err(db_error)?;
    let actual = raw
        .as_deref()
        .map(serde_json::from_str::<Inventory>)
        .transpose()
        .map_err(db_error)?
        .map(|v| v.revision)
        .unwrap_or(0);
    if actual != before {
        return Err(SoftwareError::new(
            "stale_software_inventory",
            "Software state changed in another runtime; reload before continuing",
        ));
    }
    next.revision = before
        .checked_add(1)
        .ok_or_else(|| db_error("Software revision overflow"))?;
    let raw = serde_json::to_string(next).map_err(db_error)?;
    if raw.len() > 8 * 1024 * 1024 {
        return Err(db_error("Software inventory exceeds its storage limit"));
    }
    tx.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", rusqlite::params![KEY,raw]).map_err(db_error)?;
    tx.commit().map_err(db_error)
}
pub(super) fn workspace(db: &Database, id: Option<&str>) -> Result<(), SoftwareError> {
    if let Some(id) = id {
        let status: Option<String> = db
            .conn()
            .query_row("SELECT status FROM workspaces WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(db_error)?;
        if status.as_deref() != Some("active") {
            return Err(SoftwareError::new(
                "software_workspace_unavailable",
                "The selected project is absent or archived",
            ));
        }
    }
    Ok(())
}
