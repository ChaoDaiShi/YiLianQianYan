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
    pub created_at: i64,
    pub updated_at: i64,
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
        ).map_err(|e| e.to_string())?;
        Ok(ConversationSummary { id, title: title.to_string(), created_at: now, updated_at: now })
    }

    pub fn list_conversations(&self) -> Result<Vec<ConversationSummary>, String> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, title, created_at, updated_at FROM conversations ORDER BY updated_at DESC"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            Ok(ConversationSummary {
                id: row.get(0)?, title: row.get(1)?,
                created_at: row.get(2)?, updated_at: row.get(3)?,
            })
        }).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn get_conversation(&self, id: &str) -> Result<Conversation, String> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, title, created_at, updated_at FROM conversations WHERE id = ?1"
        ).map_err(|e| e.to_string())?;
        let (conv_id, title, created_at, updated_at) = stmt.query_row(params![id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?, row.get::<_, i64>(3)?))
        }).map_err(|e| format!("Conversation not found: {}", e))?;
        drop(stmt);

        let messages = self.get_messages_internal(&conn, &conv_id)?;
        Ok(Conversation { id: conv_id, title, messages, created_at, updated_at })
    }

    pub fn delete_conversation(&self, id: &str) -> Result<(), String> {
        let conn = self.conn();
        conn.execute("DELETE FROM messages WHERE conversation_id = ?1", params![id])
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
        ).map_err(|e| e.to_string())?;
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
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn get_messages_internal(&self, conn: &std::sync::MutexGuard<'_, rusqlite::Connection>, conv_id: &str) -> Result<Vec<MessageRow>, String> {
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, role, content, tool_calls, tool_call_id, tool_name, tool_result, created_at
             FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![conv_id], |row| {
            Ok(MessageRow {
                id: row.get(0)?, conversation_id: row.get(1)?, role: row.get(2)?,
                content: row.get(3)?, tool_calls: row.get(4)?, tool_call_id: row.get(5)?,
                tool_name: row.get(6)?, tool_result: row.get(7)?, created_at: row.get(8)?,
            })
        }).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }
}
