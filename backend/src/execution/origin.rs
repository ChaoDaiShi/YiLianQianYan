//! Read-only source identity. Correlation is never an authorization grant.
use crate::{db::Database, modules::task::artifact::ArtifactSource};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputOrigin {
    pub kind: String,
    pub id: String,
    pub source_workspace_id: Option<String>,
    pub destination_workspace_id: Option<String>,
    pub conversation_id: Option<String>,
    pub automation_id: Option<String>,
    pub graph_id: Option<String>,
    pub node_id: Option<String>,
    pub execution_id: Option<String>,
    pub tool_call_id: Option<String>,
}

impl OutputOrigin {
    pub(crate) fn new(kind: &str, id: &str, workspace: Option<String>) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
            source_workspace_id: workspace,
            destination_workspace_id: None,
            conversation_id: None,
            automation_id: None,
            graph_id: None,
            node_id: None,
            execution_id: None,
            tool_call_id: None,
        }
    }
}

/// A graph without a confirmed contract has no persisted workspace owner.
/// An export destination must never be used to manufacture that owner.
pub fn graph_workspace(db: &Database, graph: &str) -> Result<Option<String>, String> {
    let conn = db.conn();
    graph_workspace_on_connection(&conn, graph)
}

fn graph_workspace_on_connection(
    conn: &rusqlite::Connection,
    graph: &str,
) -> Result<Option<String>, String> {
    let installed:bool=conn.query_row("SELECT COUNT(*)=2 FROM sqlite_master WHERE type='table' AND name IN ('proofloop_drafts','proofloop_goal_versions')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
    if !installed {
        return Ok(None);
    }
    let mut query=conn.prepare("SELECT DISTINCT g.workspace_id FROM proofloop_drafts d JOIN proofloop_goal_versions g ON g.goal_id=d.goal_id AND g.version=d.goal_version WHERE d.graph_id=?1 AND d.status='confirmed' ORDER BY g.workspace_id LIMIT 2").map_err(|e|e.to_string())?;
    let values = query
        .query_map([graph], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if values.len() > 1 {
        return Err("任务图存在冲突的来源空间，不能推断产物归属".into());
    }
    Ok(values.into_iter().next())
}

pub fn resolve_artifact_origin(
    db: &Database,
    source: &ArtifactSource,
) -> Result<OutputOrigin, String> {
    resolve_artifact_origin_on_connection(&db.conn(), source)
}

pub(crate) fn resolve_artifact_origin_on_connection(
    db: &rusqlite::Connection,
    source: &ArtifactSource,
) -> Result<OutputOrigin, String> {
    match source {
        ArtifactSource::Task {
            task_id,
            execution_id,
        } => {
            let workspace:Option<String>=db.query_row("SELECT t.workspace_id FROM tasks t JOIN task_executions e ON e.task_id=t.id WHERE t.id=?1 AND e.id=?2",rusqlite::params![task_id,execution_id],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
            let mut origin = OutputOrigin::new(
                "task",
                task_id,
                Some(workspace.ok_or("任务与执行来源不匹配")?),
            );
            origin.execution_id = Some(execution_id.clone());
            origin.destination_workspace_id = origin.source_workspace_id.clone();
            Ok(origin)
        }
        ArtifactSource::Node {
            graph_id,
            node_id,
            execution_id,
            workspace_id,
        } => {
            let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM task_node_executions WHERE graph_id=?1 AND node_id=?2 AND execution_id=?3)",rusqlite::params![graph_id,node_id,execution_id],|r|r.get(0)).map_err(|e|e.to_string())?;
            if !exists {
                return Err("节点与执行来源不匹配".into());
            }
            let mut origin = OutputOrigin::new(
                "node",
                graph_id,
                graph_workspace_on_connection(db, graph_id)?,
            );
            origin.graph_id = Some(graph_id.clone());
            origin.node_id = Some(node_id.clone());
            origin.execution_id = Some(execution_id.clone());
            origin.destination_workspace_id = if workspace_id.is_empty() {
                origin.source_workspace_id.clone()
            } else {
                Some(workspace_id.clone())
            };
            Ok(origin)
        }
    }
}

pub fn resolve_conversation_origin(
    db: &Database,
    conversation: &str,
) -> Result<OutputOrigin, String> {
    let exists: bool = db
        .conn()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1)",
            [conversation],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err("来源对话不存在".into());
    }
    let mut origin = OutputOrigin::new(
        "conversation",
        conversation,
        db.conversation_workspace(conversation)?,
    );
    origin.conversation_id = Some(conversation.into());
    Ok(origin)
}

pub fn resolve_personal_origin(
    db: &Database,
    automation: &str,
    call: &str,
) -> Result<OutputOrigin, String> {
    use crate::modules::automation::model::AutomationTarget;
    let delegation = db.get_automation(automation)?.ok_or("来源委托不存在")?;
    let Some(AutomationTarget::PersonalAssistant {
        conversation_id, ..
    }) = delegation.target
    else {
        return Err("来源不是个人委托".into());
    };
    let exists:bool=db.conn().query_row("SELECT EXISTS(SELECT 1 FROM conversation_execution_records WHERE conversation_id=?1 AND tool_call_id=?2 AND name='write_file' AND execution_status='succeeded')",rusqlite::params![conversation_id,call],|r|r.get(0)).map_err(|e|e.to_string())?;
    if !exists {
        return Err("文件创建记录不属于此委托或未成功".into());
    }
    if db.conversation_workspace(&conversation_id)?.as_deref()
        != Some(delegation.workspace_id.as_str())
    {
        return Err("委托和来源对话空间不一致".into());
    }
    let mut origin = OutputOrigin::new(
        "personal_delegation",
        automation,
        Some(delegation.workspace_id),
    );
    origin.automation_id = Some(automation.into());
    origin.conversation_id = Some(conversation_id);
    origin.tool_call_id = Some(call.into());
    Ok(origin)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn integration_origin_unbound_graph_never_inherits_export_destination() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(graph_workspace(&db, "unknown-graph").unwrap(), None);
        assert!(resolve_artifact_origin(
            &db,
            &ArtifactSource::Node {
                graph_id: "g".into(),
                node_id: "n".into(),
                execution_id: "not-real".into(),
                workspace_id: "chosen-destination".into()
            }
        )
        .is_err());
    }
    #[test]
    fn integration_origin_conversation_uses_persisted_workspace_and_rejects_missing_source() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let conversation = db.create_conversation("source").unwrap();
        assert_eq!(
            resolve_conversation_origin(&db, &conversation.id)
                .unwrap()
                .source_workspace_id,
            None
        );
        assert!(resolve_conversation_origin(&db, "not-real").is_err());
        assert!(resolve_personal_origin(&db, "not-real", "same-id").is_err());
    }
}
