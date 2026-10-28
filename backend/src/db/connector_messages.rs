//! Message branches retain ordinary conversations as their execution/history owner.
//! A fork copies its complete ancestral protocol prefix, then exposes only its new rows.
use super::{
    connectors::ConnectorLayout,
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database, MessageRow,
};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

mod overview;
#[cfg(test)]
mod tests;
pub use overview::MessageCanvasOverview;

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1023,
    "1023_connector_message_branches",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE connector_message_graphs (
        canvas_id TEXT NOT NULL REFERENCES connector_canvases(id) ON DELETE CASCADE,
        root_conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
        selected_node_id TEXT,
        layout_json TEXT NOT NULL,
        PRIMARY KEY(canvas_id,root_conversation_id));
     CREATE TABLE connector_message_runs (
        canvas_id TEXT NOT NULL,
        root_conversation_id TEXT NOT NULL,
        conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
        parent_node_id TEXT,
        prefix_count INTEGER NOT NULL,
        PRIMARY KEY(canvas_id,root_conversation_id,conversation_id),
        FOREIGN KEY(canvas_id,root_conversation_id)
            REFERENCES connector_message_graphs(canvas_id,root_conversation_id) ON DELETE CASCADE);
     CREATE TRIGGER connector_message_canvas_delete AFTER DELETE ON connector_canvases BEGIN
        DELETE FROM connector_message_runs WHERE canvas_id=OLD.id;
        DELETE FROM connector_message_graphs WHERE canvas_id=OLD.id;
     END;
     CREATE TRIGGER connector_message_conversation_delete AFTER DELETE ON conversations BEGIN
        DELETE FROM connector_message_runs WHERE conversation_id=OLD.id OR root_conversation_id=OLD.id;
        DELETE FROM connector_message_graphs WHERE root_conversation_id=OLD.id;
     END;
     CREATE TRIGGER connector_message_selection_delete AFTER DELETE ON messages BEGIN
        UPDATE connector_message_graphs SET selected_node_id=NULL WHERE selected_node_id=OLD.id;
        UPDATE connector_message_graphs SET selected_node_id=NULL WHERE selected_node_id IN (
            SELECT m.id FROM messages m WHERE m.conversation_id IN (
                WITH RECURSIVE descendants(id) AS (
                    SELECT conversation_id FROM connector_message_runs WHERE parent_node_id=OLD.id
                    UNION
                    SELECT r.conversation_id FROM connector_message_runs r
                    JOIN messages p ON p.id=r.parent_node_id
                    JOIN descendants d ON d.id=p.conversation_id
                ) SELECT id FROM descendants));
        DELETE FROM connector_message_runs WHERE conversation_id IN (
            WITH RECURSIVE descendants(id) AS (
                SELECT conversation_id FROM connector_message_runs WHERE parent_node_id=OLD.id
                UNION
                SELECT r.conversation_id FROM connector_message_runs r
                JOIN messages p ON p.id=r.parent_node_id
                JOIN descendants d ON d.id=p.conversation_id
            ) SELECT id FROM descendants);
     END;",
);

#[derive(Debug, Clone, Serialize)]
pub struct ConnectorMessageNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub role: String,
    pub content: String,
    pub conversation_id: String,
    pub created_at: i64,
    pub status: String,
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectorMessageRun {
    pub conversation_id: String,
    pub parent_node_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectorMessageGraph {
    pub canvas_id: String,
    pub conversation_id: String,
    pub selected_node_id: Option<String>,
    pub layout: ConnectorLayout,
    pub nodes: Vec<ConnectorMessageNode>,
    pub runs: Vec<ConnectorMessageRun>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectorMessageSource {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectorMessageGraphRef {
    pub conversation_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectorMessageSources {
    pub conversations: Vec<ConnectorMessageSource>,
    pub graphs: Vec<ConnectorMessageGraphRef>,
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn visible(message: &MessageRow) -> bool {
    // Canvas speech nodes represent input and output; protocol/tool phases stay in history.
    message.role == "user"
        || (message.role == "assistant"
            && message.tool_calls.is_none()
            && !message.content.is_empty())
}

fn messages(conn: &rusqlite::Connection, conversation: &str) -> Result<Vec<MessageRow>, String> {
    let mut query = conn.prepare("SELECT id,conversation_id,role,content,tool_calls,tool_call_id,tool_name,tool_result,created_at FROM messages WHERE conversation_id=?1 ORDER BY created_at,rowid").map_err(err)?;
    let rows = query
        .query_map([conversation], |row| {
            Ok(MessageRow {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                tool_calls: row.get(4)?,
                tool_call_id: row.get(5)?,
                tool_name: row.get(6)?,
                tool_result: row.get(7)?,
                created_at: row.get(8)?,
            })
        })
        .map_err(err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(err)
}

fn graph_on(
    conn: &rusqlite::Connection,
    canvas: &str,
    root: &str,
) -> Result<ConnectorMessageGraph, String> {
    let (selected, layout) = conn.query_row("SELECT selected_node_id,layout_json FROM connector_message_graphs WHERE canvas_id=?1 AND root_conversation_id=?2",params![canvas,root],|row|Ok((row.get::<_,Option<String>>(0)?, row.get::<_,String>(1)?))).optional().map_err(err)?.ok_or("消息画布不存在")?;
    let mut query = conn.prepare("SELECT r.conversation_id,r.parent_node_id,r.prefix_count,c.run_status,c.run_error FROM connector_message_runs r JOIN conversations c ON c.id=r.conversation_id WHERE r.canvas_id=?1 AND r.root_conversation_id=?2 ORDER BY c.created_at,r.rowid").map_err(err)?;
    let runs = query
        .query_map(params![canvas, root], |row| {
            Ok((
                ConnectorMessageRun {
                    conversation_id: row.get(0)?,
                    parent_node_id: row.get(1)?,
                    status: row.get(3)?,
                    error: row.get(4)?,
                },
                row.get::<_, usize>(2)?,
            ))
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let mut graph = ConnectorMessageGraph {
        canvas_id: canvas.into(),
        conversation_id: root.into(),
        selected_node_id: selected,
        layout: serde_json::from_str(&layout).map_err(err)?,
        nodes: vec![],
        runs: vec![],
    };
    for (run, prefix) in runs {
        let mut parent = run.parent_node_id.clone();
        let reasoning = super::conversations::reasoning_map(
            conn,
            &run.conversation_id,
            "chat_reasoning:",
            "assistant",
        )?;
        for message in messages(conn, &run.conversation_id)?
            .into_iter()
            .skip(prefix)
            .filter(visible)
        {
            graph.nodes.push(ConnectorMessageNode {
                id: message.id.clone(),
                parent_id: parent,
                role: message.role,
                content: message.content,
                conversation_id: run.conversation_id.clone(),
                created_at: message.created_at,
                status: run.status.clone(),
                error: run.error.clone(),
                reasoning: reasoning.get(&message.id).cloned(),
            });
            parent = Some(message.id);
        }
        graph.runs.push(run);
    }
    Ok(graph)
}

impl Database {
    pub fn initialize_connector_messages_schema(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&SCHEMA)?;
        self.initialize_message_canvas_schema()
    }

    /// Ordinary roots belong in the source picker; execution forks stay within their graph.
    pub fn connector_message_sources(
        &self,
        canvas: &str,
    ) -> Result<ConnectorMessageSources, String> {
        let conn = self.conn();
        let agent: String = conn
            .query_row(
                "SELECT agent_id FROM connector_canvases WHERE id=?1",
                [canvas],
                |row| row.get(0),
            )
            .map_err(err)?;
        let mut query = conn.prepare("SELECT c.id,c.title FROM conversations c WHERE
            EXISTS(SELECT 1 FROM connector_message_graphs g WHERE g.canvas_id=?1 AND g.root_conversation_id=c.id)
            OR (EXISTS(SELECT 1 FROM connector_bindings b WHERE b.kind='conversation_agent' AND b.agent_id=?2 AND b.source_id='conversation:' || c.id)
                AND (NOT EXISTS(SELECT 1 FROM connector_message_runs r WHERE r.conversation_id=c.id AND r.conversation_id<>r.root_conversation_id)
                    OR EXISTS(SELECT 1 FROM connector_message_graphs g WHERE g.root_conversation_id=c.id)))
            ORDER BY c.updated_at DESC,c.id").map_err(err)?;
        let conversations = query
            .query_map(params![canvas, agent], |row| {
                Ok(ConnectorMessageSource {
                    id: row.get(0)?,
                    title: row.get(1)?,
                })
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        let mut query = conn.prepare("SELECT root_conversation_id FROM connector_message_graphs WHERE canvas_id=?1 ORDER BY root_conversation_id").map_err(err)?;
        let graphs = query
            .query_map([canvas], |row| {
                Ok(ConnectorMessageGraphRef {
                    conversation_id: row.get(0)?,
                })
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        Ok(ConnectorMessageSources {
            conversations,
            graphs,
        })
    }

    pub fn connector_message_graph(
        &self,
        canvas: &str,
        root: &str,
    ) -> Result<ConnectorMessageGraph, String> {
        graph_on(&self.conn(), canvas, root)
    }

    pub fn create_connector_message_graph(
        &self,
        canvas: &str,
        root: &str,
    ) -> Result<ConnectorMessageGraph, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(err)?;
        let agent: String = tx
            .query_row(
                "SELECT agent_id FROM connector_canvases WHERE id=?1",
                [canvas],
                |row| row.get(0),
            )
            .map_err(err)?;
        tx.query_row("SELECT id FROM conversations WHERE id=?1", [root], |row| {
            row.get::<_, String>(0)
        })
        .map_err(err)?;
        let current:Option<String>=tx.query_row("SELECT agent_id FROM connector_bindings WHERE kind='conversation_agent' AND source_id=?1",[format!("conversation:{root}")],|row|row.get(0)).optional().map_err(err)?;
        if current.as_deref().is_some_and(|bound| bound != agent) {
            return Err("源会话已绑定其他智能体".into());
        }
        if current.is_none() {
            bind_on(&tx, root, &agent)?;
        }
        tx.execute("INSERT INTO connector_message_graphs(canvas_id,root_conversation_id,layout_json) VALUES(?1,?2,?3) ON CONFLICT DO NOTHING",params![canvas,root,serde_json::to_string(&ConnectorLayout::default()).map_err(err)?]).map_err(err)?;
        tx.execute(
            "INSERT INTO connector_message_runs VALUES(?1,?2,?2,NULL,0) ON CONFLICT DO NOTHING",
            params![canvas, root],
        )
        .map_err(err)?;
        let graph = graph_on(&tx, canvas, root)?;
        tx.commit().map_err(err)?;
        Ok(graph)
    }

    pub fn select_connector_message(
        &self,
        canvas: &str,
        root: &str,
        node: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn();
        let graph = graph_on(&conn, canvas, root)?;
        if node.is_some_and(|id| !graph.nodes.iter().any(|n| n.id == id)) {
            return Err("消息节点不存在于当前画布".into());
        }
        conn.execute("UPDATE connector_message_graphs SET selected_node_id=?3 WHERE canvas_id=?1 AND root_conversation_id=?2",params![canvas,root,node]).map_err(err)?;
        Ok(())
    }

    pub fn save_connector_message_layout(
        &self,
        canvas: &str,
        root: &str,
        layout: &ConnectorLayout,
    ) -> Result<(), String> {
        if !layout.viewport.x.is_finite()
            || !layout.viewport.y.is_finite()
            || !layout.viewport.zoom.is_finite()
            || !(0.25..=2.).contains(&layout.viewport.zoom)
            || layout
                .nodes
                .iter()
                .any(|node| !node.x.is_finite() || !node.y.is_finite())
        {
            return Err("消息画布坐标或缩放无效".into());
        }
        let changed=self.conn().execute("UPDATE connector_message_graphs SET layout_json=?3 WHERE canvas_id=?1 AND root_conversation_id=?2",params![canvas,root,serde_json::to_string(layout).map_err(err)?]).map_err(err)?;
        if changed == 0 {
            return Err("消息画布不存在".into());
        }
        Ok(())
    }

    /// Allocate a new ordinary conversation containing exactly the selected ancestral prefix.
    /// The chat API subsequently appends the new question and runs the existing scoped runtime.
    pub fn fork_connector_message(
        &self,
        canvas: &str,
        root: &str,
        parent: Option<&str>,
    ) -> Result<String, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(err)?;
        let graph = graph_on(&tx, canvas, root)?;
        let history = match parent {
            Some(id) => {
                let node = graph
                    .nodes
                    .iter()
                    .find(|n| n.id == id)
                    .ok_or("父消息不存在于当前画布")?;
                if node.role != "assistant" {
                    return Err("只能从助手回答继续提问".into());
                }
                let mut history = messages(&tx, &node.conversation_id)?;
                let index = history
                    .iter()
                    .position(|m| m.id == id)
                    .ok_or("父消息已不存在")?;
                history.truncate(index + 1);
                history
            }
            None => {
                if !graph.nodes.is_empty() {
                    return Err("请选择一个助手回答作为分支起点".into());
                }
                vec![]
            }
        };
        let agent: String = tx
            .query_row(
                "SELECT agent_id FROM connector_canvases WHERE id=?1",
                [canvas],
                |row| row.get(0),
            )
            .map_err(err)?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        tx.execute(
            "INSERT INTO conversations(id,title,created_at,updated_at) VALUES(?1,'消息分支',?2,?2)",
            params![id, now],
        )
        .map_err(err)?;
        for (index, message) in history.iter().enumerate() {
            let copied_id = uuid::Uuid::new_v4().to_string();
            tx.execute("INSERT INTO messages(id,conversation_id,role,content,tool_calls,tool_call_id,tool_name,tool_result,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![copied_id,id,message.role,message.content,message.tool_calls,message.tool_call_id,message.tool_name,message.tool_result,index as i64]).map_err(err)?;
            super::conversation_presentation::copy_message_attachments(
                &tx,
                &message.id,
                &copied_id,
            )?;
            for prefix in ["chat_reasoning", "chat_turn_reasoning"] {
                tx.execute(
                    "INSERT INTO settings(key,value) SELECT ?1,value FROM settings WHERE key=?2",
                    params![
                        format!("{prefix}:{copied_id}"),
                        format!("{prefix}:{}", message.id)
                    ],
                )
                .map_err(err)?;
            }
        }
        bind_on(&tx, &id, &agent)?;
        tx.execute(
            "INSERT INTO connector_message_runs VALUES(?1,?2,?3,?4,?5)",
            params![canvas, root, id, parent, history.len() as i64],
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(id)
    }
}

fn bind_on(conn: &rusqlite::Connection, conversation: &str, agent: &str) -> Result<(), String> {
    conn.execute("INSERT INTO connector_bindings(id,agent_id,kind,source_id,target_id,service_kind,model_source) VALUES(?1,?2,'conversation_agent',?3,?4,'','')",params![uuid::Uuid::new_v4().to_string(),agent,format!("conversation:{conversation}"),format!("agent:{agent}")]).map_err(err)?;
    Ok(())
}
