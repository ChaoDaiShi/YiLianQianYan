// ============================================================
// Conversation CRUD operations
// ============================================================

use rusqlite::{params, TransactionBehavior};
use serde::{Deserialize, Serialize};

use super::Database;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationRunStatus {
    #[default]
    Idle,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl ConversationRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    fn from_stored(value: &str) -> Self {
        match value {
            "idle" => Self::Idle,
            "running" => Self::Running,
            "waiting_approval" => Self::WaitingApproval,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "interrupted" => Self::Interrupted,
            _ => Self::Interrupted,
        }
    }

    fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub run_status: ConversationRunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_finished_at: Option<i64>,
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
            run_status: ConversationRunStatus::Idle,
            run_error: None,
            run_started_at: None,
            run_finished_at: None,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn list_conversations(&self) -> Result<Vec<ConversationSummary>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, title, run_status, run_error, run_started_at, run_finished_at,
                        created_at, updated_at
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
                    run_status: ConversationRunStatus::from_stored(&row.get::<_, String>(2)?),
                    run_error: row.get(3)?,
                    run_started_at: row.get(4)?,
                    run_finished_at: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
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
        let mut conn = self.conn();
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        transaction
            .execute(
                "DELETE FROM messages WHERE conversation_id = ?1",
                params![id],
            )
            .map_err(|e| e.to_string())?;
        transaction
            .execute("DELETE FROM conversations WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())?;
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
        self.add_message_if_conversation_exists(msg)
    }

    /// Insert one message only if its conversation still exists, with the
    /// existence check and insert/update committed as one SQLite transaction.
    /// This closes the anchor-validation/delete race even when SQLite foreign
    /// key enforcement is disabled by an older database connection.
    pub fn add_message_if_conversation_exists(&self, msg: &MessageRow) -> Result<(), String> {
        let mut conn = self.conn();
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1)",
                params![msg.conversation_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            return Err(format!(
                "conversation does not exist: {}",
                msg.conversation_id
            ));
        }
        transaction
            .execute(
            "INSERT INTO messages (id, conversation_id, role, content, tool_calls, tool_call_id, tool_name, tool_result, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![msg.id, msg.conversation_id, msg.role, msg.content,
                    msg.tool_calls, msg.tool_call_id, msg.tool_name, msg.tool_result, msg.created_at],
            )
            .map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().timestamp_millis();
        transaction
            .execute(
                "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
                params![now, msg.conversation_id],
            )
            .map_err(|e| e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())?;
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

    pub fn set_conversation_run_status(
        &self,
        conversation_id: &str,
        status: ConversationRunStatus,
        error: Option<&str>,
    ) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp_millis();
        let terminal = status.is_terminal();
        let conn = self.conn();
        let changed = conn
            .execute(
                "UPDATE conversations
                 SET run_status=?2,
                     run_error=?3,
                     run_started_at=CASE
                         WHEN ?2='running' AND run_status='waiting_approval' THEN run_started_at
                         WHEN ?2='running' THEN ?4
                         ELSE run_started_at
                     END,
                     run_finished_at=CASE WHEN ?5 THEN ?4 ELSE NULL END,
                     updated_at=?4
                 WHERE id=?1",
                params![conversation_id, status.as_str(), error, now, terminal],
            )
            .map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err("conversation not found".to_string());
        }
        Ok(())
    }

    pub fn interrupt_running_conversations(&self) -> Result<usize, String> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "UPDATE conversations
             SET run_status='interrupted',
                 run_error='应用退出前任务尚未完成',
                 run_finished_at=?1,
                 updated_at=?1
             WHERE run_status IN ('running', 'waiting_approval')",
            [now],
        )
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempConversationDatabase {
        path: std::path::PathBuf,
    }

    impl TempConversationDatabase {
        fn new(label: &str) -> (Self, Database) {
            let path = std::env::temp_dir().join(format!(
                "yilian-conversation-{label}-{}.db",
                uuid::Uuid::new_v4()
            ));
            let database = Database::new(&path).expect("temporary conversation database");
            (Self { path }, database)
        }
    }

    impl Drop for TempConversationDatabase {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    #[test]
    fn conversation_summary_persists_run_lifecycle() {
        let (_temp, db) = TempConversationDatabase::new("run-lifecycle");
        let conversation = db.create_conversation("后台任务").unwrap();
        db.add_message(&MessageRow {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: conversation.id.clone(),
            role: "user".to_string(),
            content: "继续运行".to_string(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at: chrono::Utc::now().timestamp_millis(),
        })
        .unwrap();

        db.set_conversation_run_status(&conversation.id, ConversationRunStatus::Running, None)
            .unwrap();

        let listed = db.list_conversations().unwrap();
        assert_eq!(listed[0].run_status, ConversationRunStatus::Running);
        assert!(listed[0].run_started_at.is_some());
        assert!(listed[0].run_finished_at.is_none());
    }

    #[test]
    fn stale_running_conversations_are_interrupted() {
        let (_temp, db) = TempConversationDatabase::new("interrupt-running");
        let conversation = db.create_conversation("后台任务").unwrap();
        db.add_message(&MessageRow {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: conversation.id.clone(),
            role: "user".to_string(),
            content: "继续运行".to_string(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at: chrono::Utc::now().timestamp_millis(),
        })
        .unwrap();
        db.set_conversation_run_status(&conversation.id, ConversationRunStatus::Running, None)
            .unwrap();

        assert_eq!(db.interrupt_running_conversations().unwrap(), 1);
        let listed = db.list_conversations().unwrap();
        assert_eq!(listed[0].run_status, ConversationRunStatus::Interrupted);
        assert!(listed[0].run_finished_at.is_some());
    }
}
