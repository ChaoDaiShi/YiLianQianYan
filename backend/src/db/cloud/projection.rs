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
            }
            "message" => {
                transaction.execute("DELETE FROM messages WHERE id=?1", [entity_id])?;
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
            _ => {}
        }
        return Ok(());
    }
    let Some(payload) = payload else {
        return Ok(());
    };
    let payload = crate::modules::cloud::domain::sanitize_sync_payload(payload.clone());
    match entity_type {
        "conversation" => {
            let title = json_text(&payload, "title").unwrap_or("恢复的对话");
            let status = json_text(&payload, "run_status")
                .filter(|value| {
                    matches!(
                        *value,
                        "idle" | "running" | "completed" | "failed" | "interrupted"
                    )
                })
                .unwrap_or("completed");
            let created = json_number(&payload, "created_at").unwrap_or(now);
            let updated = json_number(&payload, "updated_at").unwrap_or(now);
            transaction.execute(
                "INSERT INTO conversations (id,title,run_status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT(id) DO UPDATE SET title=excluded.title,run_status=excluded.run_status,updated_at=excluded.updated_at",
                params![entity_id, title, status, created, updated],
            )?;
        }
        "message" => {
            let conversation = json_text(&payload, "conversation_id").unwrap_or("cloud-restored");
            let created = json_number(&payload, "created_at").unwrap_or(now);
            transaction.execute(
                "INSERT OR IGNORE INTO conversations (id,title,run_status,created_at,updated_at) VALUES (?1,'恢复的云端对话','completed',?2,?2)",
                params![conversation, created],
            )?;
            transaction.execute(
                "INSERT INTO messages (id,conversation_id,role,content,created_at) VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT(id) DO UPDATE SET conversation_id=excluded.conversation_id,role=excluded.role,content=excluded.content,created_at=excluded.created_at",
                params![entity_id, conversation, json_text(&payload, "role").unwrap_or("assistant"), json_text(&payload, "content").unwrap_or(""), created],
            )?;
            transaction.execute(
                "UPDATE conversations SET updated_at=MAX(updated_at,?2) WHERE id=?1",
                params![conversation, created],
            )?;
        }
        "memory" => {
            transaction.execute(
                "INSERT INTO memories (id,content,category,source,source_conversation_id,metadata,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                 ON CONFLICT(id) DO UPDATE SET content=excluded.content,category=excluded.category,source=excluded.source,source_conversation_id=excluded.source_conversation_id,metadata=excluded.metadata,updated_at=excluded.updated_at",
                params![
                    entity_id,
                    json_text(&payload, "content").unwrap_or(""),
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
            transaction.execute(
                "INSERT INTO agent_definitions (id,name,description,instructions,allowed_tools,model,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'cloud_sync',?10,?11)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,instructions=excluded.instructions,allowed_tools=excluded.allowed_tools,model=excluded.model,capabilities=excluded.capabilities,max_iterations=excluded.max_iterations,enabled=excluded.enabled,updated_at=excluded.updated_at",
                params![
                    entity_id,
                    json_text(&payload, "name").unwrap_or("云端智能体"),
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
            transaction.execute(
                "INSERT INTO workflow_graphs (id,name,description,schema_version,definition_json,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,schema_version=excluded.schema_version,definition_json=excluded.definition_json,updated_at=excluded.updated_at",
                params![
                    entity_id,
                    json_text(&payload, "name").unwrap_or("云端工作流"),
                    json_text(&payload, "description").unwrap_or(""),
                    json_number(&payload, "schema_version").unwrap_or(1),
                    json_compact(payload.get("definition")).unwrap_or_else(|| "{}".into()),
                    json_optional_number(&payload, "created_at"),
                    json_optional_number(&payload, "updated_at")
                ],
            )?;
        }
        "artifact_metadata" => {
            transaction.execute(
                "INSERT INTO artifacts (id,workspace_id,task_id,task_execution_id,name,artifact_type,mime_type,size,summary,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name,artifact_type=excluded.artifact_type,mime_type=excluded.mime_type,size=excluded.size,summary=excluded.summary,updated_at=excluded.updated_at",
                params![
                    entity_id,
                    json_text(&payload, "workspace_id").unwrap_or("cloud"),
                    json_text(&payload, "task_id").unwrap_or("cloud"),
                    json_text(&payload, "task_execution_id").unwrap_or("cloud"),
                    json_text(&payload, "name").unwrap_or("云端成果"),
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
        _ => {}
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
