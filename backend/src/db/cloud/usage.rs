use rusqlite::{params, Transaction};
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::modules::cloud::payload::SyncCapabilityUsagePayload;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UsageRecord {
    pub execution_id: String,
    pub capability_id: String,
    pub plugin_id: String,
    pub version: String,
    pub used_at: i64,
    pub execution_status: String,
    pub latency_ms: i64,
    pub approval_required: bool,
    pub risk_level: String,
    pub result_digest: Option<String>,
}

pub fn record_capability_usage_tx(
    tx: &Transaction<'_>,
    record: &UsageRecord,
) -> Result<(), rusqlite::Error> {
    tx.execute(
        "INSERT INTO capability_usage (
            execution_id, schema_version, capability_id, plugin_id, version,
            used_at, execution_status, latency_ms, approval_required, risk_level,
            result_digest, created_at
        ) VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(execution_id) DO UPDATE SET
            execution_status = excluded.execution_status,
            latency_ms = excluded.latency_ms,
            result_digest = excluded.result_digest",
        params![
            record.execution_id,
            record.capability_id,
            record.plugin_id,
            record.version,
            record.used_at,
            record.execution_status,
            record.latency_ms,
            if record.approval_required { 1 } else { 0 },
            record.risk_level,
            record.result_digest,
            record.used_at,
        ],
    )?;
    Ok(())
}

pub fn project_usage_tx(
    tx: &Transaction<'_>,
    usage: &SyncCapabilityUsagePayload,
) -> Result<(), rusqlite::Error> {
    tx.execute(
        "INSERT OR IGNORE INTO capability_usage (
            execution_id, schema_version, capability_id, plugin_id, version,
            used_at, execution_status, latency_ms, approval_required, risk_level,
            result_digest, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            usage.execution_id,
            usage.schema_version,
            usage.capability_id,
            usage.plugin_id,
            usage.version,
            usage.used_at,
            usage.execution_status,
            usage.latency_ms,
            if usage.approval_required { 1 } else { 0 },
            usage.risk_level,
            usage.result_digest,
            usage.used_at,
        ],
    )?;
    Ok(())
}

pub fn list_capability_usage(
    db: &Database,
    limit: usize,
) -> Result<Vec<UsageRecord>, rusqlite::Error> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT execution_id, capability_id, plugin_id, version, used_at,
                execution_status, latency_ms, approval_required, risk_level, result_digest
         FROM capability_usage ORDER BY used_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        Ok(UsageRecord {
            execution_id: row.get(0)?,
            capability_id: row.get(1)?,
            plugin_id: row.get(2)?,
            version: row.get(3)?,
            used_at: row.get(4)?,
            execution_status: row.get(5)?,
            latency_ms: row.get(6)?,
            approval_required: row.get::<_, i64>(7)? != 0,
            risk_level: row.get(8)?,
            result_digest: row.get(9)?,
        })
    })?;
    rows.collect()
}
