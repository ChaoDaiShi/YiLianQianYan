//! A read-only index over existing domain records. No lifecycle state is owned here.
use crate::{
    app::state::AppServer,
    modules::{
        automation::model::{Automation, AutomationTarget, Occurrence},
        task::TaskGraphId,
    },
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default, Deserialize)]
pub struct WorkQuery {
    pub workspace_id: Option<String>,
    pub limit: Option<usize>,
}
#[derive(Clone, Debug, Serialize)]
pub struct WorkItem {
    pub id: String,
    pub kind: &'static str,
    pub title: String,
    pub workspace_id: Option<String>,
    pub status: String,
    pub attention_required: bool,
    pub href: String,
    pub updated_at: i64,
    pub source_ids: BTreeMap<&'static str, String>,
}
type Error = (StatusCode, String);
fn internal(e: String) -> Error {
    (StatusCode::INTERNAL_SERVER_ERROR, e)
}
fn attention(status: &str) -> bool {
    matches!(
        status,
        "waiting_approval"
            | "waiting_input"
            | "waiting_user"
            | "failed"
            | "interrupted"
            | "blocked"
            | "needs_review"
    )
}
fn source(key: &'static str, id: &str) -> BTreeMap<&'static str, String> {
    BTreeMap::from([(key, id.into())])
}
fn href(path: &str, key: &str) -> String {
    let mut value = String::from(path);
    for byte in key.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            value.push(byte as char)
        } else {
            value.push_str(&format!("%{byte:02X}"))
        }
    }
    value
}

pub async fn list(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<WorkQuery>,
) -> Result<Json<Value>, Error> {
    if query
        .workspace_id
        .as_ref()
        .is_some_and(|id| id.is_empty() || id.len() > 200 || id.chars().any(char::is_control))
    {
        return Err((StatusCode::BAD_REQUEST, "invalid workspace id".into()));
    }
    let limit = query.limit.unwrap_or(100).clamp(1, 200);
    let (items, truncated) =
        project(&server, query.workspace_id.as_deref(), limit).map_err(internal)?;
    let issues = server
        .db
        .integration_projection_issues()
        .map_err(internal)?;
    Ok(Json(
        json!({"items":items,"limit":limit,"truncated":truncated,"projection_issues":issues}),
    ))
}

/// Explicit repair only rebuilds local projections and basic evidence checks.
/// Engine continuations, side-effect tools and channel sends are never invoked.
pub async fn reconcile(State(server): State<Arc<AppServer>>) -> Result<Json<Value>, Error> {
    server.db.recover_integration_handoffs().map_err(internal)?;
    crate::execution::reconciliation::reconcile_once(&server)
        .await
        .map_err(internal)?;
    Ok(Json(
        json!({"projection_issues":server.db.integration_projection_issues().map_err(internal)?}),
    ))
}

pub fn project(
    server: &AppServer,
    workspace: Option<&str>,
    limit: usize,
) -> Result<(Vec<WorkItem>, bool), String> {
    let limit = limit.clamp(1, 200);
    let fetch = (limit + 1) as i64;
    let mut items = Vec::new();
    // Each source contributes at most L+1 candidates. That is sufficient for
    // the global top-L ordering, without loading every conversation or file.
    let conversation_rows = {
        let conn = server.db.conn();
        let mut stmt=conn.prepare("WITH RECURSIVE personal(id) AS (
          SELECT substr(key,20) FROM settings WHERE key LIKE 'personal_assistant:%'
          UNION SELECT b.conversation_id FROM message_canvas_branches b JOIN messages m ON m.id=b.parent_node_id JOIN personal p ON p.id=m.conversation_id
          UNION SELECT r.conversation_id FROM connector_message_runs r JOIN personal p ON p.id=r.root_conversation_id
        ) SELECT c.id,c.title,c.run_status,c.updated_at,s.value FROM conversations c
        LEFT JOIN settings s ON s.key='conversation_workspace:'||c.id
        WHERE NOT EXISTS(SELECT 1 FROM personal p WHERE p.id=c.id)
          AND EXISTS(SELECT 1 FROM messages m WHERE m.conversation_id=c.id)
          AND (?1 IS NULL OR s.value=?1)
        ORDER BY c.updated_at DESC,c.id DESC LIMIT ?2").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![workspace, fetch], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    for (id, title, status, updated_at, workspace_id) in conversation_rows {
        items.push(WorkItem {
            href: href("/chat/", &id),
            source_ids: source("conversation_id", &id),
            id,
            kind: "conversation",
            title,
            workspace_id,
            attention_required: attention(&status),
            status,
            updated_at,
        });
    }
    let task_rows = {
        let conn = server.db.conn();
        let mut stmt=conn.prepare("SELECT id,title,status,updated_at,workspace_id FROM tasks WHERE (?1 IS NULL OR workspace_id=?1) ORDER BY updated_at DESC,id DESC LIMIT ?2").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![workspace, fetch], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    for (id, title, status, updated_at, ws) in task_rows {
        items.push(WorkItem {
            href: href("/tasks?task=", &id),
            source_ids: source("task_id", &id),
            id,
            kind: "legacy_task",
            title,
            workspace_id: Some(ws),
            attention_required: attention(&status),
            status,
            updated_at,
        });
    }
    let graph_rows = {
        let conn = server.db.conn();
        let mut stmt=conn.prepare("SELECT t.graph_id,max(t.updated_at,coalesce((SELECT max(e.updated_at) FROM task_node_executions e WHERE e.graph_id=t.graph_id),0)) AS touched,(SELECT d.workspace_id FROM proofloop_drafts d WHERE d.graph_id=t.graph_id AND d.status='confirmed' ORDER BY d.updated_at DESC LIMIT 1) AS ws FROM task_world_supervisor_snapshots t WHERE (?1 IS NULL OR ws=?1) ORDER BY touched DESC,t.graph_id DESC LIMIT ?2").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![workspace, fetch], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    for (id, updated_at, _) in graph_rows {
        let workspace_id = crate::execution::origin::graph_workspace(&server.db, &id)?;
        let graph = TaskGraphId::new(&id).map_err(|e| e.to_string())?;
        let p = server
            .task_world
            .status_projection(&graph, updated_at)
            .map_err(|e| e.to_string())?;
        let title = server
            .db
            .confirmed_proof_draft_for_graph(&id)
            .map_err(|e| e.to_string())?
            .map(|draft| draft.contract.user_goal)
            .filter(|goal| !goal.trim().is_empty())
            .map(|goal| crate::utils::text::truncate_chars(goal.trim(), 120))
            .unwrap_or(p.title);
        items.push(WorkItem {
            href: href("/task-world/", &id),
            source_ids: source("graph_id", &id),
            id,
            kind: "task_graph",
            title,
            workspace_id,
            attention_required: p.attention_required,
            status: p.overall_status,
            updated_at,
        });
    }
    let personal_rows = {
        let conn = server.db.conn();
        let mut stmt=conn.prepare("SELECT a.payload,(SELECT o.payload FROM automation_occurrences o WHERE o.automation_id=a.id ORDER BY o.created_at DESC,o.rowid DESC LIMIT 1) FROM automations a WHERE json_extract(a.payload,'$.target.kind')='personal_assistant' AND (?1 IS NULL OR json_extract(a.payload,'$.workspace_id')=?1) ORDER BY max(json_extract(a.payload,'$.updated_at'),coalesce((SELECT max(json_extract(o.payload,'$.updated_at')) FROM automation_occurrences o WHERE o.automation_id=a.id),0)) DESC,a.id DESC LIMIT ?2").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![workspace, fetch], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    for (raw, last) in personal_rows {
        let a: Automation = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        let last = last
            .map(|raw| serde_json::from_str::<Occurrence>(&raw).map_err(|e| e.to_string()))
            .transpose()?;
        let status = if a.archived {
            "archived".into()
        } else {
            last.as_ref().map(|o| o.status.clone()).unwrap_or_else(|| {
                if a.enabled {
                    "scheduled".into()
                } else {
                    "paused".into()
                }
            })
        };
        let mut source_ids = source("automation_id", &a.id);
        if let Some(AutomationTarget::PersonalAssistant {
            conversation_id, ..
        }) = a.target
        {
            source_ids.insert("conversation_id", conversation_id);
        }
        items.push(WorkItem {
            href: href("/automations?assistant=", &a.id),
            id: a.id,
            kind: "personal_delegation",
            title: a.name,
            workspace_id: Some(a.workspace_id),
            attention_required: attention(&status),
            status,
            updated_at: last
                .map(|o| o.updated_at.max(a.updated_at))
                .unwrap_or(a.updated_at),
            source_ids,
        });
    }
    items.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.kind.cmp(b.kind))
            .then_with(|| b.id.cmp(&a.id))
    });
    let truncated = items.len() > limit;
    items.truncate(limit);
    Ok((items, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::MessageRow, safety::ControlSession};
    fn server() -> AppServer {
        let root = std::env::temp_dir().join(format!("work-index-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        AppServer::new_with_control_session(
            &root.join("test.db"),
            root.to_str().unwrap(),
            ControlSession::new("a".repeat(64)).unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn integration_work_index_preserves_kind_identity_and_workspace_filter() {
        let s = server();
        let c = s.db.create_conversation("ordinary").unwrap();
        s.db.add_message(&MessageRow {
            id: "m".into(),
            conversation_id: c.id.clone(),
            role: "user".into(),
            content: "input".into(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at: 1,
        })
        .unwrap();
        let (items, _) = project(&s, None, 100).unwrap();
        assert!(items
            .iter()
            .any(|i| i.kind == "conversation" && i.id == c.id));
        assert!(project(&s, Some("unrelated"), 100).unwrap().0.is_empty());
        s.db.conn()
            .execute(
                "INSERT INTO settings(key,value) VALUES(?1,'deleted-personal-owner')",
                [format!("personal_assistant:{}", c.id)],
            )
            .unwrap();
        assert!(!project(&s, None, 100)
            .unwrap()
            .0
            .iter()
            .any(|i| i.kind == "conversation" && i.id == c.id));
    }
}
