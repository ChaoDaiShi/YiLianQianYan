use crate::db::Database;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::modules::cloud::domain::SyncOperation;
use crate::modules::cloud::payload::validate_sync_payload_strictly;

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncPolicy {
    #[serde(default = "default_true")]
    pub sync_conversations: bool,
    #[serde(default = "default_true")]
    pub sync_messages: bool,
    #[serde(default = "default_true")]
    pub sync_memories: bool,
    #[serde(default = "default_true")]
    pub sync_agent_profiles: bool,
    #[serde(default = "default_true")]
    pub sync_workflow_metadata: bool,
    #[serde(default = "default_true")]
    pub sync_artifact_metadata: bool,
    #[serde(default = "default_true")]
    pub sync_task_graphs: bool,
    #[serde(default = "default_true")]
    pub sync_canvas_views: bool,
    #[serde(default = "default_true")]
    pub sync_settings: bool,
    #[serde(default = "default_true")]
    pub sync_capability_usage: bool,
}

impl Default for SyncPolicy {
    fn default() -> Self {
        Self {
            sync_conversations: true,
            sync_messages: true,
            sync_memories: true,
            sync_agent_profiles: true,
            sync_workflow_metadata: true,
            sync_artifact_metadata: true,
            sync_task_graphs: true,
            sync_canvas_views: true,
            sync_settings: true,
            sync_capability_usage: true,
        }
    }
}

impl SyncPolicy {
    pub fn allows_type(
        &self,
        entity_type: &str,
    ) -> Result<bool, crate::modules::cloud::contracts::SyncError> {
        use crate::modules::cloud::contracts::SyncError;
        Ok(match entity_type {
            "conversation" | "message" => self.sync_conversations && self.sync_messages,
            "memory" => self.sync_memories,
            "agent_profile" => self.sync_agent_profiles,
            "workflow_metadata" => self.sync_workflow_metadata,
            "artifact_metadata" => self.sync_artifact_metadata,
            "task_graph" | "canvas_view" => self.sync_task_graphs && self.sync_canvas_views,
            "settings" => self.sync_settings,
            "capability_usage" => self.sync_capability_usage,
            other => return Err(SyncError::UnsupportedSchema(other.to_owned())),
        })
    }
}

pub(crate) fn read_sync_policy_tx(
    tx: &rusqlite::Transaction<'_>,
) -> Result<SyncPolicy, rusqlite::Error> {
    use rusqlite::OptionalExtension;
    let encoded: Option<String> = tx
        .query_row(
            "SELECT policy_json FROM cloud_sync_policy WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    encoded
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .transpose()
        .map(|policy| policy.unwrap_or_default())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaptureReport {
    pub captured: usize,
    pub skipped_by_policy: usize,
    pub rejected: usize,
}

impl Database {
    pub fn cloud_sync_policy(&self) -> Result<SyncPolicy, rusqlite::Error> {
        let conn = self.conn();
        let encoded: Option<String> = conn
            .query_row(
                "SELECT policy_json FROM cloud_sync_policy WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        encoded
            .map(|value| {
                serde_json::from_str(&value).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .transpose()
            .map(|policy| policy.unwrap_or_default())
    }

    pub fn save_cloud_sync_policy(&self, policy: &SyncPolicy) -> Result<(), rusqlite::Error> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = read_sync_policy_tx(&tx)?;
        let any_re_enabled = (!old.sync_conversations && policy.sync_conversations)
            || (!old.sync_messages && policy.sync_messages)
            || (!old.sync_memories && policy.sync_memories)
            || (!old.sync_agent_profiles && policy.sync_agent_profiles)
            || (!old.sync_workflow_metadata && policy.sync_workflow_metadata)
            || (!old.sync_artifact_metadata && policy.sync_artifact_metadata)
            || (!old.sync_task_graphs && policy.sync_task_graphs)
            || (!old.sync_canvas_views && policy.sync_canvas_views)
            || (!old.sync_settings && policy.sync_settings)
            || (!old.sync_capability_usage && policy.sync_capability_usage);

        let now = chrono::Utc::now().timestamp_millis();
        if any_re_enabled {
            tx.execute(
                "UPDATE cloud_sync_cursor
                 SET download_cursor='0', cursor='0', updated_at=?1
                 WHERE singleton=1",
                params![now],
            )?;
        }

        let encoded = serde_json::to_string(policy)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        tx.execute(
            "INSERT INTO cloud_sync_policy(singleton, policy_json, updated_at) VALUES(1, ?1, ?2)
             ON CONFLICT(singleton) DO UPDATE SET policy_json=excluded.policy_json, updated_at=excluded.updated_at",
            params![encoded, now],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn capture_sync_candidates(&self) -> Result<usize, rusqlite::Error> {
        let report = self.capture_sync_candidates_with_policy(&self.cloud_sync_policy()?)?;
        Ok(report.captured)
    }

    pub fn capture_sync_candidates_with_policy(
        &self,
        policy: &SyncPolicy,
    ) -> Result<CaptureReport, rusqlite::Error> {
        let identity = self.get_or_create_cloud_identity()?;
        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut report = CaptureReport::default();
        let sources = collect_sync_sources(&transaction, policy, &mut report)?;
        let now = chrono::Utc::now().timestamp_millis();

        for source in sources {
            if validate_sync_payload_strictly(
                source.entity_type,
                SyncOperation::Upsert,
                Some(&source.payload),
            )
            .is_err()
            {
                report.rejected += 1;
                continue;
            }

            let payload_json = serde_json::to_string(&source.payload)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
            let fingerprint = crate::modules::cloud::contracts::compute_payload_fingerprint(Some(
                &source.payload,
            ));
            let existing: Option<(String, i64, i64, bool)> = transaction
                .query_row(
                    "SELECT fingerprint, revision, local_generation, deleted FROM cloud_entity_revisions
                     WHERE entity_type=?1 AND entity_id=?2",
                    params![source.entity_type, source.entity_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get::<_, i64>(3)? != 0,
                        ))
                    },
                )
                .optional()?;
            if existing
                .as_ref()
                .is_some_and(|(stored, _, _, deleted)| stored == &fingerprint && !deleted)
            {
                continue;
            }
            let base_revision = existing.as_ref().map(|item| item.1).unwrap_or(0);
            let local_gen = existing.as_ref().map(|item| item.2).unwrap_or(0) + 1;
            let revision = base_revision + 1;
            transaction.execute(
                "INSERT INTO cloud_sync_outbox (
                    change_id, entity_type, entity_id, base_revision, revision, operation, payload_json,
                    device_id, state, attempts, local_generation, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'upsert', ?6, ?7, 'pending', 0, ?8, ?9, ?9)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    source.entity_type,
                    source.entity_id,
                    base_revision,
                    revision,
                    payload_json,
                    identity.device_id,
                    local_gen,
                    now
                ],
            )?;
            transaction.execute(
                "INSERT INTO cloud_entity_revisions (entity_type, entity_id, fingerprint, revision, local_generation, deleted, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)
                 ON CONFLICT(entity_type, entity_id) DO UPDATE SET fingerprint=excluded.fingerprint,
                    revision=excluded.revision, local_generation=excluded.local_generation,
                    deleted=0, updated_at=excluded.updated_at",
                params![
                    source.entity_type,
                    source.entity_id,
                    fingerprint,
                    revision,
                    local_gen,
                    now
                ],
            )?;
            report.captured += 1;
        }

        transaction.commit()?;
        Ok(report)
    }
}

pub fn record_local_deletion_tx(
    tx: &rusqlite::Transaction<'_>,
    entity_type: &str,
    entity_id: &str,
    device_id: &str,
) -> Result<(), rusqlite::Error> {
    let now = chrono::Utc::now().timestamp_millis();
    let existing: Option<(i64, i64, bool)> = tx
        .query_row(
            "SELECT revision, local_generation, deleted FROM cloud_entity_revisions
             WHERE entity_type=?1 AND entity_id=?2",
            params![entity_type, entity_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2)? != 0)),
        )
        .optional()?;

    if let Some((rev, local_gen, deleted)) = existing {
        if deleted {
            return Ok(());
        }
        let next_rev = rev + 1;
        let next_gen = local_gen + 1;
        tx.execute(
            "INSERT INTO cloud_sync_outbox (
                change_id, entity_type, entity_id, base_revision, revision,
                operation, payload_json, device_id, state, attempts,
                local_generation, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, 'delete', NULL, ?6, 'pending', 0, ?7, ?8, ?8)",
            params![
                uuid::Uuid::new_v4().to_string(),
                entity_type,
                entity_id,
                rev,
                next_rev,
                device_id,
                next_gen,
                now,
            ],
        )?;
        tx.execute(
            "UPDATE cloud_entity_revisions
             SET fingerprint='', revision=?3, local_generation=?4, deleted=1, updated_at=?5
             WHERE entity_type=?1 AND entity_id=?2",
            params![entity_type, entity_id, next_rev, next_gen, now],
        )?;
    }
    Ok(())
}

pub struct SyncSource {
    pub entity_type: &'static str,
    pub entity_id: String,
    pub payload: serde_json::Value,
}

pub fn collect_sync_sources(
    transaction: &rusqlite::Transaction<'_>,
    policy: &SyncPolicy,
    report: &mut CaptureReport,
) -> Result<Vec<SyncSource>, rusqlite::Error> {
    let mut sources = Vec::new();
    if policy.sync_conversations {
        let mut statement =
            transaction.prepare("SELECT id, title, created_at, updated_at FROM conversations")?;
        let rows = statement.query_map([], |row| {
            Ok(SyncSource {
                entity_type: "conversation",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "schema_version": 1,
                    "title": row.get::<_, String>(1)?,
                    "created_at": row.get::<_, i64>(2)?,
                    "updated_at": row.get::<_, i64>(3)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    } else {
        report.skipped_by_policy += 1;
    }

    if policy.sync_messages {
        let mut statement = transaction.prepare(
            "SELECT id, conversation_id, role, content, created_at FROM messages
             WHERE role IN ('user', 'assistant')",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SyncSource {
                entity_type: "message",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "schema_version": 1,
                    "conversation_id": row.get::<_, String>(1)?,
                    "role": row.get::<_, String>(2)?,
                    "content": row.get::<_, String>(3)?,
                    "created_at": row.get::<_, i64>(4)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    } else {
        report.skipped_by_policy += 1;
    }

    if policy.sync_memories {
        let mut statement = transaction.prepare(
            "SELECT id, content, category, source, source_conversation_id, created_at, updated_at FROM memories",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SyncSource {
                entity_type: "memory",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "schema_version": 1,
                    "content": row.get::<_, String>(1)?,
                    "category": row.get::<_, String>(2)?,
                    "source": row.get::<_, String>(3)?,
                    "source_conversation_id": row.get::<_, Option<String>>(4)?,
                    "created_at": row.get::<_, i64>(5)?,
                    "updated_at": row.get::<_, i64>(6)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    } else {
        report.skipped_by_policy += 1;
    }

    if policy.sync_agent_profiles {
        let mut statement = transaction.prepare(
            "SELECT id, name, description, instructions, allowed_tools, model, capabilities, max_iterations, enabled, created_at, updated_at FROM agent_definitions",
        )?;
        let rows = statement.query_map([], |row| {
            let allowed: String = row.get(4)?;
            let capabilities: String = row.get(6)?;
            Ok(SyncSource {
                entity_type: "agent_profile",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "schema_version": 1,
                    "name": row.get::<_, String>(1)?,
                    "description": row.get::<_, String>(2)?,
                    "instructions": row.get::<_, String>(3)?,
                    "allowed_tools": parse_json_array(&allowed),
                    "model": row.get::<_, Option<String>>(5)?,
                    "capabilities": parse_json_array(&capabilities),
                    "max_iterations": row.get::<_, i64>(7)?,
                    "enabled": row.get::<_, i64>(8)? != 0,
                    "created_at": row.get::<_, i64>(9)?,
                    "updated_at": row.get::<_, i64>(10)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    } else {
        report.skipped_by_policy += 1;
    }

    if policy.sync_workflow_metadata {
        let mut statement = transaction.prepare(
            "SELECT id, name, description, definition_json, created_at, updated_at FROM workflow_graphs",
        )?;
        let rows = statement.query_map([], |row| {
            let definition: String = row.get(3)?;
            Ok(SyncSource {
                entity_type: "workflow_metadata",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "schema_version": 1,
                    "name": row.get::<_, String>(1)?,
                    "description": row.get::<_, String>(2)?,
                    "definition": parse_json_or_string(definition),
                    "created_at": row.get::<_, Option<i64>>(4)?,
                    "updated_at": row.get::<_, Option<i64>>(5)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    } else {
        report.skipped_by_policy += 1;
    }

    if policy.sync_artifact_metadata {
        let mut statement = transaction.prepare(
            "SELECT id, workspace_id, task_id, task_execution_id, name, artifact_type, mime_type, size, summary, created_at, updated_at FROM artifacts",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SyncSource {
                entity_type: "artifact_metadata",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "schema_version": 1,
                    "workspace_id": row.get::<_, String>(1)?,
                    "task_id": row.get::<_, String>(2)?,
                    "task_execution_id": row.get::<_, String>(3)?,
                    "name": row.get::<_, String>(4)?,
                    "artifact_type": row.get::<_, String>(5)?,
                    "mime_type": row.get::<_, Option<String>>(6)?,
                    "size": row.get::<_, Option<i64>>(7)?,
                    "summary": row.get::<_, String>(8)?,
                    "created_at": row.get::<_, i64>(9)?,
                    "updated_at": row.get::<_, i64>(10)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    } else {
        report.skipped_by_policy += 1;
    }

    if policy.sync_task_graphs
        && super::projection::sqlite_table_exists(transaction, "task_world_supervisor_snapshots")?
    {
        let mut statement = transaction.prepare(
            "SELECT graph_id, graph_revision, graph_json, updated_at FROM task_world_supervisor_snapshots",
        )?;
        let rows = statement.query_map([], |row| {
            let graph: String = row.get(2)?;
            Ok(SyncSource {
                entity_type: "task_graph",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "schema_version": 1,
                    "graph_revision": row.get::<_, i64>(1)?,
                    "graph": parse_json_or_string(graph),
                    "updated_at": row.get::<_, i64>(3)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }

    if policy.sync_canvas_views
        && super::projection::sqlite_table_exists(transaction, "task_canvas_views")?
    {
        let mut statement = transaction.prepare(
            "SELECT graph_id, view_revision, graph_revision_seen, viewport_json, node_layouts_json, selection_json, groups_json, updated_at FROM task_canvas_views",
        )?;
        let rows = statement.query_map([], |row| {
            let viewport: String = row.get(3)?;
            let layouts: String = row.get(4)?;
            let selection: String = row.get(5)?;
            let groups: String = row.get(6)?;
            Ok(SyncSource {
                entity_type: "canvas_view",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "schema_version": 1,
                    "view_revision": row.get::<_, i64>(1)?,
                    "graph_revision_seen": row.get::<_, i64>(2)?,
                    "viewport": parse_json_or_string(viewport),
                    "node_layouts": parse_json_or_string(layouts),
                    "selection": parse_json_or_string(selection),
                    "groups": parse_json_or_string(groups),
                    "updated_at": row.get::<_, i64>(7)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }

    if policy.sync_settings {
        let stored: Option<String> = transaction
            .query_row(
                "SELECT value FROM settings WHERE key='v1.system_preferences.v1'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(stored) = stored {
            sources.push(SyncSource {
                entity_type: "settings",
                entity_id: "system_preferences".into(),
                payload: serde_json::json!({
                    "schema_version": 1,
                    "section": "system_preferences",
                    "preferences": parse_json_or_string(stored)
                }),
            });
        }
    } else {
        report.skipped_by_policy += 1;
    }

    if policy.sync_capability_usage
        && super::projection::sqlite_table_exists(transaction, "capability_usage")?
    {
        let mut statement = transaction.prepare(
            "SELECT execution_id, capability_id, plugin_id, version, used_at, execution_status, latency_ms, approval_required, risk_level, result_digest FROM capability_usage",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SyncSource {
                entity_type: "capability_usage",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "schema_version": 1,
                    "execution_id": row.get::<_, String>(0)?,
                    "capability_id": row.get::<_, String>(1)?,
                    "plugin_id": row.get::<_, String>(2)?,
                    "version": row.get::<_, String>(3)?,
                    "used_at": row.get::<_, i64>(4)?,
                    "execution_status": row.get::<_, String>(5)?,
                    "latency_ms": row.get::<_, i64>(6)?,
                    "approval_required": row.get::<_, i64>(7)? != 0,
                    "risk_level": row.get::<_, String>(8)?,
                    "result_digest": row.get::<_, Option<String>>(9)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }

    Ok(sources)
}

pub fn parse_json_or_string(value: String) -> serde_json::Value {
    serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value))
}

fn parse_json_array(value: &str) -> Vec<String> {
    serde_json::from_str(value).unwrap_or_default()
}
