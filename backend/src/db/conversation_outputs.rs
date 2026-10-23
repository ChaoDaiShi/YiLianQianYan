use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(1051, "1051_conversation_outputs", MigrationOwner::V1TaskWorld,
    "CREATE TABLE conversation_outputs(id TEXT PRIMARY KEY,conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,tool_call_id TEXT NOT NULL,kind TEXT NOT NULL,title TEXT NOT NULL,source_tool TEXT NOT NULL,profile_id TEXT,source_url TEXT,status TEXT NOT NULL,resource_id TEXT REFERENCES resources(id),error TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,source_message_id TEXT,block_index INTEGER,version INTEGER NOT NULL DEFAULT 1,previous_output_id TEXT REFERENCES conversation_outputs(id),UNIQUE(conversation_id,tool_call_id));
     CREATE INDEX conversation_outputs_conversation ON conversation_outputs(conversation_id,created_at);");

#[derive(Clone, Debug, Serialize)]
pub struct ConversationOutput {
    pub id: String,
    pub conversation_id: String,
    pub kind: String,
    pub title: String,
    pub source_tool: String,
    pub profile_id: Option<String>,
    pub status: String,
    pub resource_id: Option<String>,
    pub preview_url: Option<String>,
    pub download_url: Option<String>,
    pub can_download: bool,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub source_message_id: Option<String>,
    pub block_index: Option<usize>,
    pub version: usize,
    pub previous_output_id: Option<String>,
    // Signed CDN URLs are persisted for the authorized download, not exposed in
    // list APIs or copied into chat model context and ordinary debug logging.
    #[serde(skip)]
    pub(crate) source_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn code_versions_reference_real_messages_resources_and_same_conversation() {
        let root = std::env::temp_dir().join(format!("code-output-{}", uuid::Uuid::new_v4()));
        let db = Database::new(&root.join("test.db")).unwrap();
        db.initialize_conversation_outputs().unwrap();
        let conversation = db.create_conversation("代码").unwrap();
        let other = db.create_conversation("另一个对话").unwrap();
        for (id, owner) in [
            ("message-1", &conversation.id),
            ("message-2", &conversation.id),
            ("message-3", &other.id),
        ] {
            db.add_message(&crate::db::MessageRow {
                id: id.into(),
                conversation_id: owner.clone(),
                role: "assistant".into(),
                content: "```rust\nfn main() {}\n```".into(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_result: None,
                created_at: 1,
            })
            .unwrap();
        }
        let service = crate::shared::resource::ResourceService::new(
            db.clone_connection(),
            root.join("resources"),
            crate::shared::event::EventHub::new(4),
        );
        let resource = service
            .ingest(
                "code.rs",
                "text/plain",
                b"fn main() {}\n",
                serde_json::json!({"source":"assistant-code-block"}),
            )
            .unwrap();
        let first = db
            .register_conversation_code_output(
                &conversation.id,
                "message-1",
                0,
                "rust",
                &resource.id,
                None,
            )
            .unwrap();
        let second = db
            .register_conversation_code_output(
                &conversation.id,
                "message-2",
                0,
                "rust",
                &resource.id,
                Some(&first.id),
            )
            .unwrap();
        assert_eq!(first.version, 1);
        assert_eq!(second.version, 2);
        assert_eq!(second.previous_output_id, Some(first.id.clone()));
        assert_eq!(second.source_message_id.as_deref(), Some("message-2"));
        assert_eq!(
            service
                .read_bytes(second.resource_id.as_deref().unwrap())
                .unwrap(),
            b"fn main() {}\n"
        );
        assert!(db
            .register_conversation_code_output(
                &other.id,
                "message-3",
                0,
                "rust",
                &resource.id,
                Some(&first.id)
            )
            .is_err());
        assert!(db
            .register_conversation_code_output(
                &conversation.id,
                "message-3",
                0,
                "rust",
                &resource.id,
                None
            )
            .is_err());
    }
}

fn row_output(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationOutput> {
    let resource_id: Option<String> = row.get(8)?;
    let source_url: Option<String> = row.get(6)?;
    Ok(ConversationOutput {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        kind: row.get(2)?,
        title: row.get(3)?,
        source_tool: row.get(4)?,
        profile_id: row.get(5)?,
        can_download: source_url.is_some(),
        source_url,
        status: row.get(7)?,
        preview_url: resource_id
            .as_ref()
            .map(|id| format!("/api/resources/{id}/preview")),
        download_url: resource_id
            .as_ref()
            .map(|id| format!("/api/resources/{id}/content")),
        resource_id,
        error: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        source_message_id: row.get(12)?,
        block_index: row.get(13)?,
        version: row.get(14)?,
        previous_output_id: row.get(15)?,
    })
}
const SELECT: &str = "SELECT id,conversation_id,kind,title,source_tool,profile_id,source_url,status,resource_id,error,created_at,updated_at,source_message_id,block_index,version,previous_output_id FROM conversation_outputs";

impl Database {
    pub fn initialize_conversation_outputs(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&SCHEMA)
    }

    pub fn conversation_outputs(
        &self,
        conversation: &str,
    ) -> Result<Vec<ConversationOutput>, String> {
        self.get_conversation(conversation)?;
        let conn = self.conn();
        let mut statement = conn
            .prepare(&format!(
                "{SELECT} WHERE conversation_id=?1 ORDER BY created_at,id"
            ))
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([conversation], row_output)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    }

    pub fn conversation_output(
        &self,
        conversation: &str,
        id: &str,
    ) -> Result<Option<ConversationOutput>, String> {
        self.conn()
            .query_row(
                &format!("{SELECT} WHERE conversation_id=?1 AND id=?2"),
                params![conversation, id],
                row_output,
            )
            .optional()
            .map_err(|e| e.to_string())
    }

    pub fn conversation_output_for_call(
        &self,
        conversation: &str,
        call: &str,
    ) -> Result<Option<ConversationOutput>, String> {
        self.conn()
            .query_row(
                &format!("{SELECT} WHERE conversation_id=?1 AND tool_call_id=?2"),
                params![conversation, call],
                row_output,
            )
            .optional()
            .map_err(|e| e.to_string())
    }

    pub fn create_conversation_image_output(
        &self,
        conversation: &str,
        call: &str,
        title: &str,
        profile: Option<&str>,
        source_url: Option<&str>,
    ) -> Result<ConversationOutput, String> {
        self.get_conversation(conversation)?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        self.conn().execute("INSERT INTO conversation_outputs(id,conversation_id,tool_call_id,kind,title,source_tool,profile_id,source_url,status,created_at,updated_at) VALUES(?1,?2,?3,'image',?4,'image_generation',?5,?6,'awaiting_download',?7,?7)",
            params![id,conversation,call,title,profile,source_url,now]).map_err(|e| e.to_string())?;
        self.conversation_output(conversation, &id)?
            .ok_or("成果登记记录不可用".into())
    }

    pub fn complete_conversation_output(
        &self,
        conversation: &str,
        id: &str,
        resource: &str,
    ) -> Result<ConversationOutput, String> {
        let changed = self.conn().execute("UPDATE conversation_outputs SET status='ready',resource_id=?3,error=NULL,updated_at=?4 WHERE conversation_id=?1 AND id=?2 AND status!='ready' AND EXISTS(SELECT 1 FROM resources WHERE id=?3)",
            params![conversation,id,resource,chrono::Utc::now().timestamp_millis()]).map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err("成果不存在、已登记或资源缺失".into());
        }
        self.conversation_output(conversation, id)?
            .ok_or("成果记录不可用".into())
    }

    pub fn fail_conversation_output(
        &self,
        conversation: &str,
        id: &str,
        error: &str,
    ) -> Result<(), String> {
        self.conn().execute("UPDATE conversation_outputs SET status='download_failed',error=?3,updated_at=?4 WHERE conversation_id=?1 AND id=?2 AND status!='ready'",
            params![conversation,id,error,chrono::Utc::now().timestamp_millis()]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn register_conversation_code_output(
        &self,
        conversation: &str,
        message: &str,
        block_index: usize,
        title: &str,
        resource: &str,
        previous: Option<&str>,
    ) -> Result<ConversationOutput, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let message_exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM messages WHERE id=?1 AND conversation_id=?2 AND role='assistant')",params![message,conversation],|r|r.get(0)).map_err(|e|e.to_string())?;
        if !message_exists {
            return Err("代码来源消息不存在于当前对话".into());
        }
        let version = if let Some(previous) = previous {
            let version: usize = tx.query_row("SELECT version FROM conversation_outputs WHERE id=?1 AND conversation_id=?2 AND kind='code' AND status='ready'", params![previous,conversation],|r|r.get(0)).map_err(|_|"所选前一版本不是当前对话中的代码成果")?;
            version + 1
        } else {
            1
        };
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        tx.execute("INSERT INTO conversation_outputs(id,conversation_id,tool_call_id,kind,title,source_tool,status,resource_id,created_at,updated_at,source_message_id,block_index,version,previous_output_id) VALUES(?1,?2,?3,'code',?4,'assistant_message','ready',?5,?6,?6,?7,?8,?9,?10)",
            params![id,conversation,format!("message:{message}:block:{block_index}"),title,resource,now,message,block_index,version,previous]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);
        self.conversation_output(conversation, &id)?
            .ok_or("代码成果记录不可用".into())
    }
}
