// ============================================================
// Conversation CRUD operations
// ============================================================

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::Database;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<MessageRow>,
    pub execution_history: Vec<ConversationExecutionRecord>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationExecutionRecord {
    pub conversation_id: String,
    pub tool_call_id: String,
    pub approval_id: Option<String>,
    pub name: String,
    pub args: serde_json::Value,
    pub risk_level: String,
    pub reason: Option<String>,
    pub approval_status: String,
    pub execution_status: String,
    pub verification_status: String,
    pub verification_reason: Option<String>,
    pub result: Option<String>,
    pub sequence: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub tool_calls: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_result: Option<String>,
    pub created_at: i64,
}

impl Database {
    pub fn create_conversation(&self, title: &str) -> Result<ConversationSummary, String> {
        let conn = self.conn();
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, title, now, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(ConversationSummary {
            id,
            title: title.to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    pub fn list_conversations(&self) -> Result<Vec<ConversationSummary>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, title, created_at, updated_at
                 FROM conversations
                 WHERE EXISTS (
                     SELECT 1 FROM messages
                     WHERE messages.conversation_id = conversations.id
                       AND (
                           length(trim(messages.content)) > 0
                           OR messages.tool_calls IS NOT NULL
                           OR messages.tool_call_id IS NOT NULL
                       )
                 )
                 ORDER BY updated_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ConversationSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn get_conversation(&self, id: &str) -> Result<Conversation, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT id, title, created_at, updated_at FROM conversations WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        let (conv_id, title, created_at, updated_at) = stmt
            .query_row(params![id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| format!("Conversation not found: {}", e))?;
        drop(stmt);

        let messages = self.get_messages_internal(&conn, &conv_id)?;
        let execution_history = self.get_execution_history_internal(&conn, &conv_id)?;
        Ok(Conversation {
            id: conv_id,
            title,
            messages,
            execution_history,
            created_at,
            updated_at,
        })
    }

    pub fn delete_conversation(&self, id: &str) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM messages WHERE conversation_id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_conversation_title(&self, id: &str, title: &str) -> Result<(), String> {
        let conn = self.conn();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── Messages ──

    pub fn add_message(&self, msg: &MessageRow) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content, tool_calls, tool_call_id, tool_name, tool_result, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![msg.id, msg.conversation_id, msg.role, msg.content,
                    msg.tool_calls, msg.tool_call_id, msg.tool_name, msg.tool_result, msg.created_at],
        ).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now, msg.conversation_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn get_messages_internal(
        &self,
        conn: &std::sync::MutexGuard<'_, rusqlite::Connection>,
        conv_id: &str,
    ) -> Result<Vec<MessageRow>, String> {
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, role, content, tool_calls, tool_call_id, tool_name, tool_result, created_at
             FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC"
        ).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![conv_id], |row| {
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
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn get_execution_history(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<ConversationExecutionRecord>, String> {
        let conn = self.conn();
        self.get_execution_history_internal(&conn, conversation_id)
    }

    fn get_execution_history_internal(
        &self,
        conn: &std::sync::MutexGuard<'_, rusqlite::Connection>,
        conversation_id: &str,
    ) -> Result<Vec<ConversationExecutionRecord>, String> {
        let mut stmt = conn
            .prepare(
                "SELECT conversation_id, tool_call_id, approval_id, name, args_json,
                        risk_level, reason, approval_status, execution_status,
                        verification_status, verification_reason, result, sequence,
                        started_at, finished_at
                 FROM conversation_execution_records
                 WHERE conversation_id = ?1
                 ORDER BY sequence ASC, tool_call_id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![conversation_id], |row| {
                let args_json: String = row.get(4)?;
                Ok(ConversationExecutionRecord {
                    conversation_id: row.get(0)?,
                    tool_call_id: row.get(1)?,
                    approval_id: row.get(2)?,
                    name: row.get(3)?,
                    args: serde_json::from_str(&args_json).unwrap_or(serde_json::Value::Null),
                    risk_level: row.get(5)?,
                    reason: row.get(6)?,
                    approval_status: row.get(7)?,
                    execution_status: row.get(8)?,
                    verification_status: row.get(9)?,
                    verification_reason: row.get(10)?,
                    result: row.get(11)?,
                    sequence: row.get(12)?,
                    started_at: row.get(13)?,
                    finished_at: row.get(14)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn apply_execution_event(
        &self,
        event: &crate::agent::engine::AgentEvent,
    ) -> Result<(), String> {
        let Some(tool_call_id) = event.tool_call_id.as_deref() else {
            return Ok(());
        };

        let mut record = self
            .get_execution_history(&event.conversation_id)?
            .into_iter()
            .find(|item| item.tool_call_id == tool_call_id)
            .unwrap_or_else(|| ConversationExecutionRecord {
                conversation_id: event.conversation_id.clone(),
                tool_call_id: tool_call_id.to_string(),
                approval_id: None,
                name: event
                    .tool_name
                    .clone()
                    .unwrap_or_else(|| "unknown_tool".to_string()),
                args: event.args.clone().unwrap_or(serde_json::json!({})),
                risk_level: event
                    .risk_level
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                reason: None,
                approval_status: "not_required".to_string(),
                execution_status: "queued".to_string(),
                verification_status: "not_requested".to_string(),
                verification_reason: None,
                result: None,
                sequence: 0,
                started_at: None,
                finished_at: None,
            });

        if record.sequence == 0 {
            record.sequence = self
                .get_execution_history(&event.conversation_id)?
                .iter()
                .map(|item| item.sequence)
                .max()
                .unwrap_or(0)
                + 1;
        }
        if let Some(name) = &event.tool_name {
            record.name = name.clone();
        }
        if let Some(args) = &event.args {
            record.args = args.clone();
        }
        if let Some(risk_level) = &event.risk_level {
            record.risk_level = risk_level.clone();
        }
        if let Some(approval_id) = &event.approval_id {
            record.approval_id = Some(approval_id.clone());
        }
        let now = chrono::Utc::now().timestamp_millis();
        match event.event_type.as_str() {
            "tool_start" => {
                record.execution_status = "running".to_string();
                record.started_at.get_or_insert(now);
            }
            "approval_required" => {
                record.approval_status = "pending".to_string();
                record.execution_status = "awaiting_approval".to_string();
                record.reason = event.reason.clone();
            }
            "approval_resolved" => {
                let status = event.status.as_deref().unwrap_or("cancelled");
                record.approval_status = status.to_string();
                record.execution_status = match status {
                    "approved" => "queued",
                    "rejected" => "rejected",
                    "cancelled" | "expired" => "cancelled",
                    _ => "queued",
                }
                .to_string();
            }
            "tool_end" => {
                record.execution_status = if event.status.as_deref() == Some("success") {
                    "succeeded"
                } else {
                    "failed"
                }
                .to_string();
                record.result = event.result.clone();
                record.finished_at = Some(now);
            }
            "verification" => {
                record.verification_status = if event.verification_success == Some(true) {
                    "passed"
                } else {
                    "failed"
                }
                .to_string();
                record.verification_reason = event.verification_reason.clone();
            }
            _ => return Ok(()),
        }

        let conn = self.conn();
        conn.execute(
            "INSERT INTO conversation_execution_records
                (conversation_id, tool_call_id, approval_id, name, args_json, risk_level,
                 reason, approval_status, execution_status, verification_status,
                 verification_reason, result, sequence, started_at, finished_at,
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)
             ON CONFLICT(conversation_id, tool_call_id) DO UPDATE SET
                 approval_id=excluded.approval_id, name=excluded.name, args_json=excluded.args_json,
                 risk_level=excluded.risk_level, reason=excluded.reason,
                 approval_status=excluded.approval_status, execution_status=excluded.execution_status,
                 verification_status=excluded.verification_status,
                 verification_reason=excluded.verification_reason, result=excluded.result,
                 sequence=excluded.sequence, started_at=excluded.started_at,
                 finished_at=excluded.finished_at, updated_at=excluded.updated_at",
            params![
                record.conversation_id,
                record.tool_call_id,
                record.approval_id,
                record.name,
                serde_json::to_string(&record.args).unwrap_or_else(|_| "{}".to_string()),
                record.risk_level,
                record.reason,
                record.approval_status,
                record.execution_status,
                record.verification_status,
                record.verification_reason,
                record.result,
                record.sequence,
                record.started_at,
                record.finished_at,
                now,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
