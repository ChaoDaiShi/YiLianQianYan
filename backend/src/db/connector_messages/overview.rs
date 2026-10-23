//! Read-only projection of ordinary conversation history, including persisted fork ancestry.
use super::*;

const OVERVIEW_LAYOUT_KEY: &str = "message_canvas.overview.layout";
const BRANCH_SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1024,
    "1024_message_canvas_branches",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE message_canvas_branches (
        conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
        parent_node_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
        prefix_count INTEGER NOT NULL);
     CREATE TRIGGER message_canvas_branch_conversation_delete AFTER DELETE ON conversations BEGIN
        DELETE FROM message_canvas_branches WHERE conversation_id=OLD.id;
     END;
     CREATE TRIGGER message_canvas_branch_message_delete AFTER DELETE ON messages BEGIN
        DELETE FROM message_canvas_branches WHERE parent_node_id=OLD.id;
     END;
     CREATE TRIGGER message_canvas_preserve_canvas_branches BEFORE DELETE ON connector_canvases BEGIN
        INSERT OR IGNORE INTO message_canvas_branches(conversation_id,parent_node_id,prefix_count)
        SELECT conversation_id,parent_node_id,prefix_count FROM connector_message_runs
        WHERE canvas_id=OLD.id AND conversation_id<>root_conversation_id AND parent_node_id IS NOT NULL;
     END;",
);

#[derive(Debug, Clone, Serialize)]
pub struct MessageCanvasBranch {
    pub parent_node_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageCanvasConversation {
    pub id: String,
    pub title: String,
    pub status: String,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub nodes: Vec<ConnectorMessageNode>,
    pub branch: Option<MessageCanvasBranch>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageCanvasOverview {
    pub conversations: Vec<MessageCanvasConversation>,
    pub layout: ConnectorLayout,
}

/// Convert the former frame-origin/relative-child layout into message-only absolute coordinates.
/// Once the current canvas saves its positions, the persisted layout uses this canonical format.
fn message_only_layout(
    mut layout: ConnectorLayout,
    conversations: &[MessageCanvasConversation],
) -> ConnectorLayout {
    let points: std::collections::HashMap<_, _> = layout
        .nodes
        .iter()
        .map(|point| (point.id.as_str(), point))
        .collect();
    if !conversations
        .iter()
        .any(|conversation| points.contains_key(conversation.id.as_str()))
    {
        return layout;
    }
    let mut nodes = vec![];
    for conversation in conversations {
        for (index, message) in conversation.nodes.iter().enumerate() {
            let saved = points.get(message.id.as_str());
            if let Some(origin) = points.get(conversation.id.as_str()) {
                nodes.push(super::super::connectors::ConnectorPosition {
                    id: message.id.clone(),
                    x: origin.x + saved.map_or(24., |point| point.x),
                    y: origin.y + saved.map_or(48. + index as f64 * 148., |point| point.y),
                });
            } else if let Some(saved) = saved {
                nodes.push((**saved).clone());
            }
        }
    }
    layout.nodes = nodes;
    layout
}

impl Database {
    pub(super) fn initialize_message_canvas_schema(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&BRANCH_SCHEMA)
    }

    /// Opening the canvas does not create graphs, conversations, or agent bindings.
    pub fn message_canvas_overview(&self) -> Result<MessageCanvasOverview, String> {
        let conn = self.conn();
        let mut query = conn.prepare("SELECT id,title,run_status,run_error,created_at,updated_at FROM conversations ORDER BY created_at,id").map_err(err)?;
        let mut conversations = query
            .query_map([], |row| {
                Ok(MessageCanvasConversation {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    status: row.get(2)?,
                    error: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    nodes: vec![],
                    branch: None,
                })
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        for conversation in &mut conversations {
            let relationship: Option<(Option<String>, usize)> = conn.query_row(
                "SELECT parent_node_id,prefix_count FROM message_canvas_branches WHERE conversation_id=?1
                 UNION ALL SELECT parent_node_id,prefix_count FROM connector_message_runs
                    WHERE conversation_id=?1 AND conversation_id<>root_conversation_id
                 LIMIT 1", [&conversation.id], |row| Ok((row.get(0)?, row.get(1)?)),
            ).optional().map_err(err)?;
            let (mut parent, prefix) = relationship.unwrap_or((None, 0));
            conversation.branch = parent
                .clone()
                .map(|parent_node_id| MessageCanvasBranch { parent_node_id });
            let visible_messages: Vec<_> = messages(&conn, &conversation.id)?
                .into_iter()
                .skip(prefix)
                .filter(visible)
                .collect();
            let last = visible_messages.last().map(|message| message.id.clone());
            for message in visible_messages {
                let current = Some(&message.id) == last.as_ref() && message.role != "user";
                let reasoning_key = if message.role == "user" {
                    format!("chat_turn_reasoning:{}", message.id)
                } else {
                    format!("chat_reasoning:{}", message.id)
                };
                let reasoning: Option<String> = conn
                    .query_row(
                        "SELECT value FROM settings WHERE key=?1",
                        [reasoning_key],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(err)?;
                conversation.nodes.push(ConnectorMessageNode {
                    id: message.id.clone(),
                    parent_id: parent,
                    role: message.role,
                    content: message.content,
                    conversation_id: conversation.id.clone(),
                    created_at: message.created_at,
                    status: if current {
                        conversation.status.clone()
                    } else {
                        "completed".into()
                    },
                    error: if current {
                        conversation.error.clone()
                    } else {
                        None
                    },
                    reasoning,
                });
                parent = Some(message.id);
            }
        }
        let saved: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                [OVERVIEW_LAYOUT_KEY],
                |row| row.get(0),
            )
            .optional()
            .map_err(err)?;
        let layout = match saved {
            Some(json) => serde_json::from_str(&json).map_err(err)?,
            None => ConnectorLayout::default(),
        };
        let layout = message_only_layout(layout, &conversations);
        Ok(MessageCanvasOverview {
            conversations,
            layout,
        })
    }

    pub fn save_message_canvas_layout(&self, layout: &ConnectorLayout) -> Result<(), String> {
        if !layout.viewport.x.is_finite()
            || !layout.viewport.y.is_finite()
            || !layout.viewport.zoom.is_finite()
            || !(0.1..=2.).contains(&layout.viewport.zoom)
            || layout
                .nodes
                .iter()
                .any(|node| !node.x.is_finite() || !node.y.is_finite())
        {
            return Err("消息画布坐标或缩放无效".into());
        }
        self.conn().execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![OVERVIEW_LAYOUT_KEY, serde_json::to_string(layout).map_err(err)?]).map_err(err)?;
        Ok(())
    }

    /// Explicitly branch from an existing answer, retaining its full protocol prefix.
    /// The source's agent binding is inherited only when one already exists.
    pub fn fork_message_canvas_conversation(
        &self,
        conversation: &str,
        parent: &str,
    ) -> Result<String, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(err)?;
        let title: String = tx
            .query_row(
                "SELECT title FROM conversations WHERE id=?1",
                [conversation],
                |row| row.get(0),
            )
            .map_err(err)?;
        let mut history = messages(&tx, conversation)?;
        let index = history
            .iter()
            .position(|message| {
                message.id == parent && message.role == "assistant" && visible(message)
            })
            .ok_or("请选择当前会话中的助手回答作为分支起点")?;
        history.truncate(index + 1);
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        tx.execute(
            "INSERT INTO conversations(id,title,created_at,updated_at) VALUES(?1,?2,?3,?3)",
            params![id, format!("{title} · 分支"), now],
        )
        .map_err(err)?;
        for (index, message) in history.iter().enumerate() {
            let copied_id = uuid::Uuid::new_v4().to_string();
            tx.execute("INSERT INTO messages(id,conversation_id,role,content,tool_calls,tool_call_id,tool_name,tool_result,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![copied_id,id,message.role,message.content,message.tool_calls,message.tool_call_id,message.tool_name,message.tool_result,index as i64]).map_err(err)?;
            crate::db::conversation_presentation::copy_message_attachments(
                &tx,
                &message.id,
                &copied_id,
            )?;
            tx.execute(
                "INSERT INTO settings(key,value) SELECT ?1,value FROM settings WHERE key=?2",
                params![
                    format!("chat_reasoning:{copied_id}"),
                    format!("chat_reasoning:{}", message.id)
                ],
            )
            .map_err(err)?;
            tx.execute(
                "INSERT INTO settings(key,value) SELECT ?1,value FROM settings WHERE key=?2",
                params![
                    format!("chat_turn_reasoning:{copied_id}"),
                    format!("chat_turn_reasoning:{}", message.id)
                ],
            )
            .map_err(err)?;
        }
        tx.execute(
            "INSERT INTO settings(key,value) SELECT ?1,value FROM settings WHERE key=?2",
            params![
                format!("chat_options:{id}"),
                format!("chat_options:{conversation}")
            ],
        )
        .map_err(err)?;
        tx.execute(
            "INSERT INTO settings(key,value) SELECT ?1,value FROM settings WHERE key=?2",
            params![
                format!("conversation_mode:{id}"),
                format!("conversation_mode:{conversation}")
            ],
        )
        .map_err(err)?;
        let agent: Option<String> = tx.query_row("SELECT agent_id FROM connector_bindings WHERE kind='conversation_agent' AND source_id=?1", [format!("conversation:{conversation}")], |row| row.get(0)).optional().map_err(err)?;
        if let Some(agent) = agent {
            bind_on(&tx, &id, &agent)?;
        }
        tx.execute("INSERT INTO message_canvas_branches(conversation_id,parent_node_id,prefix_count) VALUES(?1,?2,?3)", params![id,parent,history.len() as i64]).map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(id)
    }
}
