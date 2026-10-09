use crate::db::Database;
use rusqlite::{params, OptionalExtension, TransactionBehavior};

impl Database {
    pub fn capture_sync_candidates(&self) -> Result<usize, rusqlite::Error> {
        use sha2::{Digest, Sha256};
        use std::collections::HashSet;

        let identity = self.get_or_create_cloud_identity()?;
        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let sources = collect_sync_sources(&transaction)?;
        let current_keys = sources
            .iter()
            .map(|source| (source.entity_type.to_string(), source.entity_id.clone()))
            .collect::<HashSet<_>>();
        let now = chrono::Utc::now().timestamp_millis();
        let mut captured = 0usize;

        for source in sources {
            let payload = crate::modules::cloud::domain::sanitize_sync_payload(source.payload);
            let payload_json = serde_json::to_string(&payload)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
            let fingerprint = Sha256::digest(payload_json.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let existing: Option<(String, i64, bool)> = transaction
                .query_row(
                    "SELECT fingerprint,revision,deleted FROM cloud_entity_revisions
                     WHERE entity_type=?1 AND entity_id=?2",
                    params![source.entity_type, source.entity_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2)? != 0)),
                )
                .optional()?;
            if existing
                .as_ref()
                .is_some_and(|(stored, _, deleted)| stored == &fingerprint && !deleted)
            {
                continue;
            }
            let base_revision = existing.as_ref().map(|item| item.1).unwrap_or(0);
            let revision = base_revision + 1;
            transaction.execute(
                "INSERT INTO cloud_sync_outbox (
                    change_id,entity_type,entity_id,base_revision,revision,operation,payload_json,
                    device_id,state,attempts,created_at,updated_at
                 ) VALUES (?1,?2,?3,?4,?5,'upsert',?6,?7,'pending',0,?8,?8)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    source.entity_type,
                    source.entity_id,
                    base_revision,
                    revision,
                    payload_json,
                    identity.device_id,
                    now
                ],
            )?;
            transaction.execute(
                "INSERT INTO cloud_entity_revisions (entity_type,entity_id,fingerprint,revision,deleted,updated_at)
                 VALUES (?1,?2,?3,?4,0,?5)
                 ON CONFLICT(entity_type,entity_id) DO UPDATE SET fingerprint=excluded.fingerprint,
                    revision=excluded.revision,deleted=0,updated_at=excluded.updated_at",
                params![source.entity_type, source.entity_id, fingerprint, revision, now],
            )?;
            captured += 1;
        }

        let tracked = {
            let mut statement = transaction.prepare(
                "SELECT entity_type,entity_id,revision FROM cloud_entity_revisions WHERE deleted=0",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (entity_type, entity_id, base_revision) in tracked {
            if current_keys.contains(&(entity_type.clone(), entity_id.clone())) {
                continue;
            }
            let revision = base_revision + 1;
            transaction.execute(
                "INSERT INTO cloud_sync_outbox (
                    change_id,entity_type,entity_id,base_revision,revision,operation,payload_json,
                    device_id,state,attempts,created_at,updated_at
                 ) VALUES (?1,?2,?3,?4,?5,'delete',NULL,?6,'pending',0,?7,?7)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    entity_type,
                    entity_id,
                    base_revision,
                    revision,
                    identity.device_id,
                    now
                ],
            )?;
            transaction.execute(
                "UPDATE cloud_entity_revisions SET fingerprint='',revision=?3,deleted=1,updated_at=?4
                 WHERE entity_type=?1 AND entity_id=?2",
                params![entity_type, entity_id, revision, now],
            )?;
            captured += 1;
        }
        transaction.commit()?;
        Ok(captured)
    }
}

pub(super) struct SyncSource {
    pub(super) entity_type: &'static str,
    pub(super) entity_id: String,
    pub(super) payload: serde_json::Value,
}

pub(super) fn collect_sync_sources(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<Vec<SyncSource>, rusqlite::Error> {
    let mut sources = Vec::new();
    {
        let mut statement = transaction
            .prepare("SELECT id,title,run_status,created_at,updated_at FROM conversations")?;
        let rows = statement.query_map([], |row| {
            Ok(SyncSource {
                entity_type: "conversation",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "title": row.get::<_, String>(1)?,
                    "run_status": row.get::<_, String>(2)?,
                    "created_at": row.get::<_, i64>(3)?,
                    "updated_at": row.get::<_, i64>(4)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    {
        let mut statement = transaction.prepare(
            "SELECT id,conversation_id,role,content,created_at FROM messages
             WHERE role IN ('user','assistant','system')",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SyncSource {
                entity_type: "message",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "conversation_id": row.get::<_, String>(1)?,
                    "role": row.get::<_, String>(2)?,
                    "content": row.get::<_, String>(3)?,
                    "created_at": row.get::<_, i64>(4)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    {
        let mut statement = transaction.prepare(
            "SELECT id,content,category,source,source_conversation_id,metadata,created_at,updated_at FROM memories",
        )?;
        let rows = statement.query_map([], |row| {
            let metadata: Option<String> = row.get(5)?;
            Ok(SyncSource {
                entity_type: "memory",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "content": row.get::<_, String>(1)?,
                    "category": row.get::<_, String>(2)?,
                    "source": row.get::<_, String>(3)?,
                    "source_conversation_id": row.get::<_, Option<String>>(4)?,
                    "metadata": metadata.map(parse_json_or_string),
                    "created_at": row.get::<_, i64>(6)?,
                    "updated_at": row.get::<_, i64>(7)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    {
        let mut statement = transaction.prepare(
            "SELECT id,name,description,instructions,allowed_tools,model,capabilities,max_iterations,enabled,source,created_at,updated_at FROM agent_definitions",
        )?;
        let rows = statement.query_map([], |row| {
            let allowed: String = row.get(4)?;
            let capabilities: String = row.get(6)?;
            Ok(SyncSource {
                entity_type: "agent_profile",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "name": row.get::<_, String>(1)?,
                    "description": row.get::<_, String>(2)?,
                    "instructions": row.get::<_, String>(3)?,
                    "allowed_tools": parse_json_or_string(allowed),
                    "model": row.get::<_, Option<String>>(5)?,
                    "capabilities": parse_json_or_string(capabilities),
                    "max_iterations": row.get::<_, i64>(7)?,
                    "enabled": row.get::<_, i64>(8)? != 0,
                    "source": row.get::<_, String>(9)?,
                    "created_at": row.get::<_, i64>(10)?,
                    "updated_at": row.get::<_, i64>(11)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    {
        let mut statement = transaction.prepare(
            "SELECT id,name,description,schema_version,definition_json,created_at,updated_at FROM workflow_graphs",
        )?;
        let rows = statement.query_map([], |row| {
            let definition: String = row.get(4)?;
            Ok(SyncSource {
                entity_type: "workflow_metadata",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "name": row.get::<_, String>(1)?,
                    "description": row.get::<_, String>(2)?,
                    "schema_version": row.get::<_, i64>(3)?,
                    "definition": parse_json_or_string(definition),
                    "created_at": row.get::<_, Option<i64>>(5)?,
                    "updated_at": row.get::<_, Option<i64>>(6)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    {
        let mut statement = transaction.prepare(
            "SELECT id,workspace_id,task_id,task_execution_id,name,artifact_type,mime_type,size,summary,created_at,updated_at FROM artifacts",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SyncSource {
                entity_type: "artifact_metadata",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
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
    }
    if super::projection::sqlite_table_exists(transaction, "task_world_supervisor_snapshots")? {
        let mut statement = transaction.prepare(
            "SELECT graph_id,graph_revision,graph_json,node_states_json,updated_at FROM task_world_supervisor_snapshots",
        )?;
        let rows = statement.query_map([], |row| {
            let graph: String = row.get(2)?;
            let states: String = row.get(3)?;
            Ok(SyncSource {
                entity_type: "task_graph",
                entity_id: row.get(0)?,
                payload: serde_json::json!({
                    "graph_revision": row.get::<_, i64>(1)?,
                    "graph": parse_json_or_string(graph),
                    "node_states": parse_json_or_string(states),
                    "updated_at": row.get::<_, i64>(4)?
                }),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    if super::projection::sqlite_table_exists(transaction, "task_canvas_views")? {
        let mut statement = transaction.prepare(
            "SELECT graph_id,view_revision,graph_revision_seen,viewport_json,node_layouts_json,selection_json,groups_json,updated_at FROM task_canvas_views",
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
    Ok(sources)
}

pub(super) fn parse_json_or_string(value: String) -> serde_json::Value {
    serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value))
}
