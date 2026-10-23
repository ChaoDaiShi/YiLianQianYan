use super::migrations::{MigrationOwner, ProductMigrationSpec};
use super::{Database, MessageRow};
use rusqlite::params;
use serde::Serialize;

#[derive(Serialize)]
pub struct ConversationTurnRecord {
    pub id: String,
    pub conversation_id: String,
    pub user_message_id: String,
    pub status: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}
impl Database {
    pub fn conversation_workspace(&self, id: &str) -> Result<Option<String>, String> {
        use rusqlite::OptionalExtension;
        self.conn()
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                [format!("conversation_workspace:{id}")],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }
    pub fn set_conversation_workspace(
        &self,
        id: &str,
        workspace: Option<&str>,
    ) -> Result<(), String> {
        self.get_conversation(id)?;
        if let Some(workspace) = workspace {
            let parsed =
                crate::workspace::WorkspaceId::new(workspace).map_err(|e| e.to_string())?;
            let row = self.get_workspace(&parsed)?.ok_or("工作空间不存在")?;
            if row.status != crate::workspace::WorkspaceStatus::Active {
                return Err("工作空间已归档".into());
            }
            self.conn().execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("conversation_workspace:{id}"),workspace]).map_err(|e|e.to_string())?;
        } else {
            self.conn()
                .execute(
                    "DELETE FROM settings WHERE key=?1",
                    [format!("conversation_workspace:{id}")],
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn load_conversation_compaction(
        &self,
        id: &str,
    ) -> Result<Option<crate::agent::context_compaction::CompactedContext>, String> {
        use rusqlite::OptionalExtension;
        let raw: Option<String> = self
            .conn()
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                [format!("conversation_compaction:{id}")],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        raw.map(|v| serde_json::from_str(&v).map_err(|e| e.to_string()))
            .transpose()
    }
    pub fn save_conversation_compaction(
        &self,
        id: &str,
        checkpoint: &Option<crate::agent::context_compaction::CompactedContext>,
    ) -> Result<(), String> {
        if let Some(point) = checkpoint {
            let json = serde_json::to_string(point).map_err(|e| e.to_string())?;
            self.conn().execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("conversation_compaction:{id}"),json]).map_err(|e|e.to_string())?;
        }
        Ok(())
    }
    pub fn initialize_conversation_turns(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&ProductMigrationSpec::new(1050,"1050_conversation_turns",MigrationOwner::V1TaskWorld,
            "CREATE TABLE conversation_turns(id TEXT PRIMARY KEY,conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,user_message_id TEXT NOT NULL,status TEXT NOT NULL,started_at INTEGER NOT NULL,finished_at INTEGER); CREATE INDEX conversation_turns_conversation ON conversation_turns(conversation_id,started_at);"))
    }
    pub fn accept_conversation_turn(&self, message: &MessageRow) -> Result<String, String> {
        self.accept_conversation_turn_with_resources(message, &[])
    }
    pub fn accept_conversation_turn_with_resources(
        &self,
        message: &MessageRow,
        resources: &[String],
    ) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO messages(id,conversation_id,role,content,tool_calls,tool_call_id,tool_name,tool_result,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![message.id,message.conversation_id,message.role,message.content,message.tool_calls,message.tool_call_id,message.tool_name,message.tool_result,message.created_at]).map_err(|e|e.to_string())?;
        if resources.len() > crate::modules::resource::MAX_INPUT_FILES {
            return Err("一次最多附加8个文件".into());
        }
        for (position, resource) in resources.iter().enumerate() {
            let bound:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM resource_bindings WHERE resource_id=?1 AND target_kind='conversation' AND target_id=?2 AND owner='local-user')",params![resource,message.conversation_id],|r|r.get(0)).map_err(|e|e.to_string())?;
            if !bound {
                return Err("消息附件尚未绑定到当前会话".into());
            }
            tx.execute(
                "INSERT INTO message_attachments(message_id,resource_id,position) VALUES(?1,?2,?3)",
                params![message.id, resource, position as i64],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.execute(
            "INSERT INTO conversation_turns VALUES(?1,?2,?3,'running',?4,NULL)",
            params![id, message.conversation_id, message.id, message.created_at],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("UPDATE conversations SET run_status='running',run_error=NULL,run_started_at=?2,run_finished_at=NULL,updated_at=?2 WHERE id=?1",params![message.conversation_id,message.created_at]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(id)
    }
    pub fn finish_conversation_turn(&self, conversation: &str, status: &str) -> Result<(), String> {
        let finished = if matches!(status, "running" | "waiting_approval" | "waiting_input") {
            None
        } else {
            Some(chrono::Utc::now().timestamp_millis())
        };
        self.conn().execute("UPDATE conversation_turns SET status=?2,finished_at=?3 WHERE id=(SELECT id FROM conversation_turns WHERE conversation_id=?1 AND finished_at IS NULL ORDER BY started_at DESC,rowid DESC LIMIT 1)",params![conversation,status,finished]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn conversation_turns(
        &self,
        conversation: &str,
    ) -> Result<Vec<ConversationTurnRecord>, String> {
        let conn = self.conn();
        let mut statement=conn.prepare("SELECT id,conversation_id,user_message_id,status,started_at,finished_at FROM conversation_turns WHERE conversation_id=?1 ORDER BY started_at DESC,rowid DESC LIMIT 100").map_err(|e|e.to_string())?;
        let rows = statement
            .query_map([conversation], |r| {
                Ok(ConversationTurnRecord {
                    id: r.get(0)?,
                    conversation_id: r.get(1)?,
                    user_message_id: r.get(2)?,
                    status: r.get(3)?,
                    started_at: r.get(4)?,
                    finished_at: r.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
}
