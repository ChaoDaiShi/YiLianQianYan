use crate::modules::cloud::domain::SyncOperation;
use rusqlite::{params, OptionalExtension};

pub(super) fn sqlite_table_exists(
    transaction: &rusqlite::Transaction<'_>,
    table: &str,
) -> Result<bool, rusqlite::Error> {
    transaction
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |_| Ok(true),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
}

pub(super) fn apply_remote_entity(
    transaction: &rusqlite::Transaction<'_>,
    entity_type: &str,
    entity_id: &str,
    operation: SyncOperation,
    payload: Option<&serde_json::Value>,
    now: i64,
) -> Result<(), rusqlite::Error> {
    if operation == SyncOperation::Delete {
        match entity_type {
            "conversation" => {
                transaction
                    .execute("DELETE FROM messages WHERE conversation_id=?1", [entity_id])?;
                transaction.execute("DELETE FROM conversations WHERE id=?1", [entity_id])?;
                if sqlite_table_exists(transaction, "cloud_pending_projections")? {
                    transaction.execute(
                        "DELETE FROM cloud_pending_projections WHERE dependency_type='conversation' AND dependency_id=?1",
                        [entity_id],
                    )?;
                }
            }
            "message" => {
                transaction.execute("DELETE FROM messages WHERE id=?1", [entity_id])?;
                if sqlite_table_exists(transaction, "cloud_pending_projections")? {
                    transaction.execute(
                        "DELETE FROM cloud_pending_projections WHERE entity_type='message' AND entity_id=?1",
                        [entity_id],
                    )?;
                }
            }
            "memory" => {
                transaction.execute("DELETE FROM memories WHERE id=?1", [entity_id])?;
            }
            "agent_profile" => {
                transaction.execute("DELETE FROM agent_definitions WHERE id=?1", [entity_id])?;
            }
            "workflow_metadata" => {
                transaction.execute("DELETE FROM workflow_graphs WHERE id=?1", [entity_id])?;
            }
            "artifact_metadata" => {
                transaction.execute("DELETE FROM artifacts WHERE id=?1", [entity_id])?;
            }
            "task_graph"
                if sqlite_table_exists(transaction, "task_world_supervisor_snapshots")? =>
            {
                transaction.execute(
                    "DELETE FROM task_world_supervisor_snapshots WHERE graph_id=?1",
                    [entity_id],
                )?;
            }
            "canvas_view" if sqlite_table_exists(transaction, "task_canvas_views")? => {
                transaction.execute(
                    "DELETE FROM task_canvas_views WHERE graph_id=?1",
                    [entity_id],
                )?;
            }
            "capability_usage" => {
                transaction.execute(
                    "DELETE FROM capability_usage WHERE execution_id=?1",
                    [entity_id],
                )?;
            }
            "settings" if entity_id == "system_preferences" => {
                transaction.execute(
                    "DELETE FROM settings WHERE key='v1.system_preferences.v1'",
                    [],
                )?;
            }
            _ => {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "unsupported entity type for deletion: {entity_type}"
                )));
            }
        }
        return Ok(());
    }
    let Some(payload) = payload else {
        return Err(rusqlite::Error::InvalidParameterName(
            "missing payload for upsert".into(),
        ));
    };
    let payload = crate::modules::cloud::domain::sanitize_sync_payload(payload.clone());
    match entity_type {
        "conversation" => {
            let title = json_text(&payload, "title").ok_or_else(|| {
                rusqlite::Error::InvalidParameterName(
                    "missing required title in conversation".into(),
                )
            })?;
            let status = "idle";
            let created = json_number(&payload, "created_at").unwrap_or(now);
            let updated = json_number(&payload, "updated_at").unwrap_or(now);
            transaction.execute(
                "INSERT INTO conversations (id,title,run_status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT(id) DO UPDATE SET title=excluded.title,run_status=excluded.run_status,updated_at=excluded.updated_at",
                params![entity_id, title, status, created, updated],
            )?;

            if sqlite_table_exists(transaction, "cloud_pending_projections")? {
                let mut stmt = transaction.prepare(
                    "SELECT id, entity_id, payload_json FROM cloud_pending_projections
                     WHERE dependency_type='conversation' AND dependency_id=?1 ORDER BY sequence ASC",
                )?;
                let rows: Vec<(String, String, Option<String>)> = stmt
                    .query_map([entity_id], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })?
                    .filter_map(Result::ok)
                    .collect();
                for (pending_id, msg_id, payload_str) in rows {
                    if let Some(payload_str) = payload_str {
                        if let Ok(msg_payload) =
                            serde_json::from_str::<serde_json::Value>(&payload_str)
                        {
                            let role = json_text(&msg_payload, "role").unwrap_or("assistant");
                            let content = json_text(&msg_payload, "content").unwrap_or("");
                            let msg_created =
                                json_number(&msg_payload, "created_at").unwrap_or(now);
                            transaction.execute(
                                "INSERT INTO messages (id,conversation_id,role,content,created_at) VALUES (?1,?2,?3,?4,?5)
                                 ON CONFLICT(id) DO UPDATE SET conversation_id=excluded.conversation_id,role=excluded.role,content=excluded.content,created_at=excluded.created_at",
                                params![msg_id, entity_id, role, content, msg_created],
                            )?;
                        }
                    }
                    transaction.execute(
                        "DELETE FROM cloud_pending_projections WHERE id=?1",
                        [pending_id],
                    )?;
                }
            }
        }
        "message" => {
            let conversation = json_text(&payload, "conversation_id").ok_or_else(|| {
                rusqlite::Error::InvalidParameterName(
                    "missing required conversation_id in message".into(),
                )
            })?;
            let role = json_text(&payload, "role").ok_or_else(|| {
                rusqlite::Error::InvalidParameterName("missing required role in message".into())
            })?;
            let content = json_text(&payload, "content").ok_or_else(|| {
                rusqlite::Error::InvalidParameterName("missing required content in message".into())
            })?;
            let created = json_number(&payload, "created_at").unwrap_or(now);

            let conv_exists: bool = transaction
                .query_row(
                    "SELECT 1 FROM conversations WHERE id=?1",
                    [conversation],
                    |_| Ok(true),
                )
                .optional()?
                .unwrap_or(false);

            if !conv_exists {
                if sqlite_table_exists(transaction, "cloud_pending_projections")? {
                    let payload_json_str = serde_json::to_string(&payload).unwrap_or_default();
                    transaction.execute(
                        "INSERT INTO cloud_pending_projections (id, entity_type, entity_id, dependency_type, dependency_id, revision, operation, payload_json, sequence, created_at, updated_at)
                         VALUES (?1, 'message', ?2, 'conversation', ?3, 1, 'upsert', ?4, 0, ?5, ?5)",
                        params![uuid::Uuid::new_v4().to_string(), entity_id, conversation, payload_json_str, now],
                    )?;
                } else {
                    return Err(rusqlite::Error::InvalidParameterName(format!(
                        "conversation {conversation} does not exist and pending projections table unavailable"
                    )));
                }
            } else {
                transaction.execute(
                    "INSERT INTO messages (id,conversation_id,role,content,created_at) VALUES (?1,?2,?3,?4,?5)
                     ON CONFLICT(id) DO UPDATE SET conversation_id=excluded.conversation_id,role=excluded.role,content=excluded.content,created_at=excluded.created_at",
                    params![entity_id, conversation, role, content, created],
                )?;
                transaction.execute(
                    "UPDATE conversations SET updated_at=MAX(updated_at,?2) WHERE id=?1",
                    params![conversation, created],
                )?;
            }
        }
        "memory" => {
            let content = json_text(&payload, "content").ok_or_else(|| {
                rusqlite::Error::InvalidParameterName("missing required content in memory".into())
            })?;
            transaction.execute(
                "INSERT INTO memories (id,content,category,source,source_conversation_id,metadata,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                 ON CONFLICT(id) DO UPDATE SET content=excluded.content,category=excluded.category,source=excluded.source,source_conversation_id=excluded.source_conversation_id,metadata=excluded.metadata,updated_at=excluded.updated_at",
                params![
                    entity_id,
                    content,
                    json_text(&payload, "category").unwrap_or("note"),
                    json_text(&payload, "source").unwrap_or("cloud_sync"),
                    json_optional_text(&payload, "source_conversation_id"),
                    json_compact(payload.get("metadata")),
                    json_number(&payload, "created_at").unwrap_or(now),
                    json_number(&payload, "updated_at").unwrap_or(now)
                ],
            )?;
        }
        "agent_profile" => {
            let name = json_text(&payload, "name").ok_or_else(|| {
                rusqlite::Error::InvalidParameterName(
                    "missing required name in agent_profile".into(),
                )
            })?;
            transaction.execute(
                "INSERT INTO agent_definitions (id,name,description,instructions,allowed_tools,model,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'cloud_sync',?10,?11)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,instructions=excluded.instructions,allowed_tools=excluded.allowed_tools,model=excluded.model,capabilities=excluded.capabilities,max_iterations=excluded.max_iterations,enabled=excluded.enabled,updated_at=excluded.updated_at",
                params![
                    entity_id,
                    name,
                    json_text(&payload, "description").unwrap_or(""),
                    json_text(&payload, "instructions").unwrap_or(""),
                    json_compact(payload.get("allowed_tools")).unwrap_or_else(|| "[]".into()),
                    json_optional_text(&payload, "model"),
                    json_compact(payload.get("capabilities")).unwrap_or_else(|| "[]".into()),
                    json_number(&payload, "max_iterations").unwrap_or(10),
                    if payload.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true) { 1 } else { 0 },
                    json_number(&payload, "created_at").unwrap_or(now),
                    json_number(&payload, "updated_at").unwrap_or(now)
                ],
            )?;
        }
        "workflow_metadata" => {
            let name = json_text(&payload, "name").ok_or_else(|| {
                rusqlite::Error::InvalidParameterName(
                    "missing required name in workflow_metadata".into(),
                )
            })?;
            transaction.execute(
                "INSERT INTO workflow_graphs (id,name,description,schema_version,definition_json,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,schema_version=excluded.schema_version,definition_json=excluded.definition_json,updated_at=excluded.updated_at",
                params![
                    entity_id,
                    name,
                    json_text(&payload, "description").unwrap_or(""),
                    json_number(&payload, "schema_version").unwrap_or(1),
                    json_compact(payload.get("definition")).unwrap_or_else(|| "{}".into()),
                    json_optional_number(&payload, "created_at"),
                    json_optional_number(&payload, "updated_at")
                ],
            )?;
        }
        "artifact_metadata" => {
            let name = json_text(&payload, "name").ok_or_else(|| {
                rusqlite::Error::InvalidParameterName(
                    "missing required name in artifact_metadata".into(),
                )
            })?;
            transaction.execute(
                "INSERT INTO artifacts (id,workspace_id,task_id,task_execution_id,name,artifact_type,mime_type,size,summary,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name,artifact_type=excluded.artifact_type,mime_type=excluded.mime_type,size=excluded.size,summary=excluded.summary,updated_at=excluded.updated_at",
                params![
                    entity_id,
                    json_text(&payload, "workspace_id").unwrap_or("cloud"),
                    json_text(&payload, "task_id").unwrap_or("cloud"),
                    json_text(&payload, "task_execution_id").unwrap_or("cloud"),
                    name,
                    json_text(&payload, "artifact_type").unwrap_or("other"),
                    json_optional_text(&payload, "mime_type"),
                    json_optional_number(&payload, "size"),
                    json_text(&payload, "summary").unwrap_or(""),
                    json_number(&payload, "created_at").unwrap_or(now),
                    json_number(&payload, "updated_at").unwrap_or(now)
                ],
            )?;
        }
        "task_graph" if sqlite_table_exists(transaction, "task_world_supervisor_snapshots")? => {
            transaction.execute(
                "INSERT INTO task_world_supervisor_snapshots (graph_id,graph_revision,graph_json,node_states_json,updated_at) VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT(graph_id) DO UPDATE SET graph_revision=excluded.graph_revision,graph_json=excluded.graph_json,node_states_json=excluded.node_states_json,updated_at=excluded.updated_at WHERE excluded.graph_revision>=task_world_supervisor_snapshots.graph_revision",
                params![
                    entity_id,
                    json_number(&payload, "graph_revision").unwrap_or(1),
                    json_compact(payload.get("graph")).unwrap_or_else(|| "{}".into()),
                    json_compact(payload.get("node_states")).unwrap_or_else(|| "[]".into()),
                    json_number(&payload, "updated_at").unwrap_or(now)
                ],
            )?;
        }
        "canvas_view" if sqlite_table_exists(transaction, "task_canvas_views")? => {
            transaction.execute(
                "INSERT INTO task_canvas_views (graph_id,view_revision,graph_revision_seen,viewport_json,node_layouts_json,selection_json,groups_json,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                 ON CONFLICT(graph_id) DO UPDATE SET view_revision=excluded.view_revision,graph_revision_seen=excluded.graph_revision_seen,viewport_json=excluded.viewport_json,node_layouts_json=excluded.node_layouts_json,selection_json=excluded.selection_json,groups_json=excluded.groups_json,updated_at=excluded.updated_at WHERE excluded.view_revision>=task_canvas_views.view_revision",
                params![
                    entity_id,
                    json_number(&payload, "view_revision").unwrap_or(1),
                    json_number(&payload, "graph_revision_seen").unwrap_or(1),
                    json_compact(payload.get("viewport")).unwrap_or_else(|| "{\"x\":0,\"y\":0,\"zoom\":1}".into()),
                    json_compact(payload.get("node_layouts")).unwrap_or_else(|| "[]".into()),
                    json_compact(payload.get("selection")).unwrap_or_else(|| "[]".into()),
                    json_compact(payload.get("groups")).unwrap_or_else(|| "[]".into()),
                    json_number(&payload, "updated_at").unwrap_or(now)
                ],
            )?;
        }
        "capability_usage" => {
            let usage: crate::modules::cloud::payload::SyncCapabilityUsagePayload =
                serde_json::from_value(payload.clone())
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
            crate::db::cloud::usage::project_usage_tx(transaction, &usage)?;
        }
        "settings" if entity_id == "system_preferences" => {
            let preferences = payload.get("preferences").ok_or_else(|| {
                rusqlite::Error::InvalidParameterName(
                    "missing preferences in settings payload".into(),
                )
            })?;
            transaction.execute(
                "INSERT INTO settings(key,value) VALUES('v1.system_preferences.v1',?1)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [serde_json::to_string(preferences)
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?],
            )?;
        }
        _ => {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "unsupported entity type: {entity_type}"
            )));
        }
    }
    Ok(())
}

fn json_text<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(|item| item.as_str())
}
fn json_optional_text(value: &serde_json::Value, key: &str) -> Option<String> {
    json_text(value, key).map(str::to_string)
}
fn json_number(value: &serde_json::Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|item| item.as_i64())
}
fn json_optional_number(value: &serde_json::Value, key: &str) -> Option<i64> {
    json_number(value, key)
}
fn json_compact(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .filter(|item| !item.is_null())
        .and_then(|item| serde_json::to_string(item).ok())
}
