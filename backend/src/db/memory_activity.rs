use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database, MessageRow,
};
use rusqlite::{params, Connection};
use serde::Serialize;

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1084,
    "1084_memory_activity",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE memory_activity (
        id TEXT PRIMARY KEY, kind TEXT NOT NULL, recorded_at INTEGER NOT NULL,
        content TEXT, conversation_id TEXT, message_id TEXT
    );
    CREATE INDEX memory_activity_recent ON memory_activity(recorded_at DESC, id DESC);",
);
const BACKFILL_KEY: &str = "memory_activity_backfilled";
pub(super) const AUTOMATION_INPUTS: &str = "automation_inputs AS (
    SELECT json_extract(payload,'$.assistant_turn_id') AS message_id,
           json_extract(payload,'$.conversation_id') AS conversation_id
    FROM automation_occurrences WHERE json_extract(payload,'$.assistant_turn_id') IS NOT NULL
    UNION SELECT message_id,conversation_id FROM memory_activity WHERE kind='automation_message'
)";

pub(super) fn is_automation_input_on(
    conn: &Connection,
    id: &str,
    conversation: &str,
) -> rusqlite::Result<bool> {
    conn.query_row(
        &format!("WITH {AUTOMATION_INPUTS} SELECT EXISTS(SELECT 1 FROM automation_inputs WHERE message_id=?1 AND conversation_id=?2)"),
        params![id, conversation], |row| row.get(0),
    )
}
const REMOVED_SOURCES: ProductMigrationSpec = ProductMigrationSpec::new(
    1086,
    "1086_removed_memory_sources",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE removed_memory_sources (
        memory_id TEXT PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
        conversation_id TEXT NOT NULL
    );
    CREATE TRIGGER remember_removed_memory_source BEFORE DELETE ON conversations BEGIN
        INSERT OR REPLACE INTO removed_memory_sources(memory_id,conversation_id)
        SELECT id,OLD.id FROM memories WHERE source_conversation_id=OLD.id;
    END;
    CREATE TRIGGER clear_removed_memory_source AFTER DELETE ON memories BEGIN
        DELETE FROM removed_memory_sources WHERE memory_id=OLD.id;
    END;
    INSERT INTO removed_memory_sources(memory_id,conversation_id)
    SELECT m.id,v.source_ref FROM memories m
    JOIN memory_versions v ON v.memory_id=m.id AND v.revision=1
    LEFT JOIN conversations c ON c.id=v.source_ref
    WHERE m.source_conversation_id IS NULL AND v.source_ref IS NOT NULL AND c.id IS NULL
        AND (CASE WHEN json_valid(m.metadata) THEN json_extract(m.metadata,'$.source_task_id') END)
            IS NOT v.source_ref;",
);

#[derive(Serialize)]
pub struct MemoryActivity {
    pub id: String,
    pub kind: String,
    pub recorded_at: i64,
    pub content: Option<String>,
    pub conversation_id: Option<String>,
    pub source_removed: bool,
    pub memory_id: Option<String>,
    pub removable: bool,
    pub origin: Option<String>,
    pub memory_status: Option<String>,
}

fn chat_excerpt(text: &str) -> String {
    if crate::safety::contains_sensitive_content(text) {
        "此条消息含敏感内容，已隐藏正文。".into()
    } else {
        let mut chars = text.trim().chars();
        let mut excerpt: String = chars.by_ref().take(2_000).collect();
        if chars.next().is_some() {
            excerpt.push('…');
        }
        excerpt
    }
}

fn insert_chat_activity(
    conn: &Connection,
    message_id: &str,
    conversation_id: &str,
    text: &str,
    recorded_at: i64,
    automated: bool,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO memory_activity VALUES(?1,?6,?2,?3,?4,?5)",
        params![
            format!("message:{message_id}"),
            recorded_at,
            chat_excerpt(text),
            conversation_id,
            message_id,
            if automated {
                "automation_message"
            } else {
                "chat_message"
            },
        ],
    )?;
    Ok(())
}

pub(super) fn record_chat_activity(
    conn: &Connection,
    message: &MessageRow,
) -> rusqlite::Result<()> {
    if message.role == "user" {
        insert_chat_activity(
            conn,
            &message.id,
            &message.conversation_id,
            &message.content,
            message.created_at,
            crate::modules::automation::personal::dispatching_automatic_input(),
        )?;
    }
    Ok(())
}

pub(super) fn record_memory_removal(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO memory_activity(id,kind,recorded_at) VALUES(?1,'memory_deleted',?2)",
        params![format!("deletion:{}", uuid::Uuid::new_v4()), now],
    )?;
    Ok(())
}

impl Database {
    pub(super) fn initialize_memory_activity(&self) -> rusqlite::Result<()> {
        self.apply_product_migration(&SCHEMA)?;
        self.apply_product_migration(&REMOVED_SOURCES)?;
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let imported: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?1)",
            [BACKFILL_KEY],
            |r| r.get(0),
        )?;
        if !imported {
            // Branch copies use ordinal timestamps and are not new user actions.
            {
                let mut stmt = tx.prepare(
                    "SELECT m.id,m.conversation_id,m.content,m.created_at
                    FROM messages m JOIN conversations c ON c.id=m.conversation_id
                    WHERE m.role='user' AND m.created_at>=c.created_at",
                )?;
                let mut rows = stmt.query([])?;
                while let Some(row) = rows.next()? {
                    let (id, conversation, content): (String, String, String) =
                        (row.get(0)?, row.get(1)?, row.get(2)?);
                    insert_chat_activity(&tx, &id, &conversation, &content, row.get(3)?, false)?;
                }
            }
            tx.execute(
                "INSERT INTO settings(key,value) VALUES(?1,'1')",
                [BACKFILL_KEY],
            )?;
        }
        tx.commit()
    }

    #[cfg(test)]
    fn memory_activity(
        &self,
        recent: bool,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<MemoryActivity>> {
        self.search_memory_activity(recent, None, limit, offset)
    }

    pub fn search_memory_activity(
        &self,
        recent: bool,
        query: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<MemoryActivity>> {
        let conn = self.conn();
        let recent_since = (chrono::Utc::now() - chrono::Duration::days(30)).timestamp_millis();
        let mut stmt = conn.prepare(
            &format!("WITH {AUTOMATION_INPUTS}
             SELECT 'message:'||msg.id,CASE WHEN recent_auto.message_id IS NOT NULL THEN 'automation_message' ELSE 'chat_message' END,msg.created_at,msg.content,c.id,0,NULL,0,
                    CASE WHEN recent_auto.message_id IS NOT NULL THEN 'automation_message' ELSE 'user_message' END,NULL
             FROM messages msg JOIN conversations c ON c.id=msg.conversation_id
             LEFT JOIN automation_inputs recent_auto ON recent_auto.message_id=msg.id AND recent_auto.conversation_id=c.id
             WHERE ?1 AND msg.role='user' AND msg.created_at>=c.created_at AND msg.created_at>=?5
             AND (?4 IS NULL OR instr(lower(msg.content),lower(?4))>0)
             UNION ALL
             SELECT a.id,CASE WHEN journal_auto.message_id IS NOT NULL THEN 'automation_message' ELSE a.kind END,a.recorded_at,a.content,
                    CASE WHEN msg.id IS NOT NULL THEN c.id END,
                    a.kind IN ('chat_message','automation_message') AND (msg.id IS NULL OR c.id IS NULL),NULL,1,
                    CASE WHEN journal_auto.message_id IS NOT NULL THEN 'automation_message' WHEN a.kind='chat_message' THEN 'user_message' END,NULL
             FROM memory_activity a
             LEFT JOIN messages msg ON msg.id=a.message_id AND msg.conversation_id=a.conversation_id
             LEFT JOIN conversations c ON c.id=a.conversation_id
             LEFT JOIN automation_inputs journal_auto ON journal_auto.message_id=a.message_id AND journal_auto.conversation_id=a.conversation_id
             WHERE NOT ?1
             AND (?4 IS NULL OR instr(lower(coalesce(a.content,'')),lower(?4))>0)
             UNION ALL
             SELECT 'version:'||v.memory_id||':'||v.revision,
                    CASE WHEN v.status='withdrawn' THEN 'memory_withdrawn' WHEN v.revision=1 THEN 'memory_created' ELSE 'memory_edited' END,
                    v.recorded_at,v.content,CASE WHEN source.message_id IS NULL OR origin.id IS NOT NULL THEN c.id END,
                    (removed.memory_id IS NOT NULL AND (v.source_ref IS NULL OR v.source_ref=removed.conversation_id))
                        OR (m.source_conversation_id IS NOT NULL AND coalesce(v.source_ref,m.source_conversation_id)=m.source_conversation_id AND c.id IS NULL)
                        OR (source.message_id IS NOT NULL AND origin.id IS NULL),
                    v.memory_id,0,v.source,v.status
             FROM memory_versions v JOIN memories m ON m.id=v.memory_id
             LEFT JOIN removed_memory_sources removed ON removed.memory_id=v.memory_id
             LEFT JOIN conversations c ON c.id=coalesce(v.source_ref,m.source_conversation_id)
             LEFT JOIN (
                 SELECT id,CASE WHEN json_valid(metadata) THEN json_extract(metadata,'$.source_message_id') END AS message_id FROM memories
             ) source ON source.id=m.id AND c.id=m.source_conversation_id
             LEFT JOIN messages origin ON origin.id=source.message_id AND origin.conversation_id=c.id
             LEFT JOIN memory_versions previous ON previous.memory_id=v.memory_id AND previous.revision=v.revision-1
             WHERE NOT ?1 AND NOT EXISTS(SELECT 1 FROM knowledge_document_chunks k WHERE k.memory_id=v.memory_id)
             AND (previous.memory_id IS NULL OR v.content IS NOT previous.content OR v.category IS NOT previous.category
                  OR v.source IS NOT previous.source OR v.scope IS NOT previous.scope OR v.status IS NOT previous.status
                  OR (v.reason<>'' AND v.reason IS NOT previous.reason) OR v.source_ref IS NOT previous.source_ref
                  OR v.valid_from IS NOT previous.valid_from OR v.valid_until IS NOT previous.valid_until)
             AND (?4 IS NULL OR instr(lower(v.content),lower(?4))>0)
             ORDER BY 3 DESC,1 DESC LIMIT ?2 OFFSET ?3"),
        )?;
        let rows = stmt.query_map(
            params![recent, limit as i64, offset as i64, query, recent_since],
            |r| {
                let content: Option<String> = r.get(3)?;
                Ok(MemoryActivity {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    recorded_at: r.get(2)?,
                    content: if recent {
                        content.as_deref().map(chat_excerpt)
                    } else {
                        content
                    },
                    conversation_id: r.get(4)?,
                    source_removed: r.get(5)?,
                    memory_id: r.get(6)?,
                    removable: r.get(7)?,
                    origin: r.get(8)?,
                    memory_status: r.get(9)?,
                })
            },
        )?;
        rows.collect()
    }

    pub fn remove_memory_activity(&self, id: &str) -> rusqlite::Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM memory_activity WHERE id=?1)",
            [id],
            |row| row.get(0),
        )?;
        if exists {
            self.record_memory_purge(super::memory_purge_journal::PurgeKind::Activity, &[id])?;
            tx.execute("DELETE FROM memory_activity WHERE id=?1", [id])?;
        }
        tx.commit()?;
        Ok(exists)
    }

    #[cfg(test)]
    fn memory_source_removed(&self, id: &str) -> rusqlite::Result<bool> {
        let mut memory = self
            .find_memory(id)
            .map_err(rusqlite::Error::InvalidParameterName)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        Ok(self.memory_source_status(std::slice::from_mut(&mut memory))?[0])
    }

    pub fn memory_source_status(
        &self,
        records: &mut [super::Memory],
    ) -> rusqlite::Result<Vec<bool>> {
        let conn = self.conn();
        Self::memory_source_status_on(&conn, records)
    }

    pub(super) fn memory_source_status_on(
        conn: &rusqlite::Connection,
        records: &mut [super::Memory],
    ) -> rusqlite::Result<Vec<bool>> {
        let mut status = conn.prepare(
            "WITH source(message_id) AS (SELECT CASE WHEN json_valid(?3) THEN json_extract(?3,'$.source_message_id') END)
             SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?2)
                    AND (message_id IS NULL OR EXISTS(SELECT 1 FROM messages WHERE id=message_id AND conversation_id=?2)),
                    EXISTS(SELECT 1 FROM removed_memory_sources WHERE memory_id=?1) FROM source",
        )?;
        records
            .iter_mut()
            .map(|memory| {
                if let Some(metadata) = memory
                    .metadata
                    .as_deref()
                    .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
                {
                    if let Some((graph, report)) = metadata
                        .get("source_graph_id")
                        .and_then(serde_json::Value::as_str)
                        .zip(
                            metadata
                                .get("source_report_id")
                                .and_then(serde_json::Value::as_str),
                        )
                    {
                        return super::memory_candidates::live_report_exists(conn, graph, report)
                            .map(|available| !available);
                    }
                }
                let (available, recorded): (bool, bool) = status.query_row(
                    params![memory.id, memory.source_conversation_id, memory.metadata],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                let removed = !available && (recorded || memory.source_conversation_id.is_some());
                if removed {
                    memory.source_conversation_id = None;
                }
                Ok(removed)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::CreateMemoryRequest;

    fn message(conversation_id: String, content: &str) -> MessageRow {
        MessageRow {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id,
            role: "user".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at: chrono::Utc::now().timestamp_millis(),
        }
    }

    #[test]
    fn chat_projection_follows_source_lifetime_and_journal_requires_explicit_removal() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let chat = db.create_conversation("问好").unwrap();
        let greeting = message(chat.id.clone(), "你好");
        db.accept_conversation_turn_with_resources(&greeting, &[])
            .unwrap();
        let recent = db.memory_activity(true, 20, 0).unwrap();
        assert_eq!(recent[0].content.as_deref(), Some("你好"));
        assert_eq!(recent[0].conversation_id.as_deref(), Some(chat.id.as_str()));
        db.delete_conversation(&chat.id).unwrap();
        assert!(db.memory_activity(true, 20, 0).unwrap().is_empty());
        let journal = db.memory_activity(false, 20, 0).unwrap();
        assert_eq!(journal[0].content.as_deref(), Some("你好"));
        assert!(journal[0].source_removed);
        assert!(journal[0].conversation_id.is_none());
        assert!(db.remove_memory_activity(&journal[0].id).unwrap());
        assert!(db.memory_activity(false, 20, 0).unwrap().is_empty());
    }

    #[test]
    fn deleting_chat_preserves_memory_and_marks_its_original_conversation_removed() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let chat = db.create_conversation("来源").unwrap();
        let memory = db
            .create_memory(&CreateMemoryRequest {
                content: "用户喜欢茶".into(),
                category: "preference".into(),
                source: "manual".into(),
                source_conversation_id: Some(chat.id.clone()),
                metadata: None,
            })
            .unwrap();
        assert!(!db.memory_source_removed(&memory.id).unwrap());
        db.delete_conversation(&chat.id).unwrap();
        assert_eq!(
            db.find_memory(&memory.id).unwrap().unwrap().content,
            "用户喜欢茶"
        );
        assert!(db.memory_source_removed(&memory.id).unwrap());
        let events = db.memory_activity(false, 20, 0).unwrap();
        let created = events
            .iter()
            .find(|event| event.kind == "memory_created")
            .unwrap();
        assert!(created.source_removed);
        assert!(created.conversation_id.is_none());
    }

    #[test]
    fn removing_source_message_hides_links_even_while_the_chat_exists() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let chat = db.create_conversation("来源").unwrap();
        let source = message(chat.id.clone(), "项目使用坐标系 4490。");
        db.add_message(&source).unwrap();
        db.add_message(&message(chat.id.clone(), "另一条消息"))
            .unwrap();
        let memory = db
            .create_memory(&CreateMemoryRequest {
                content: "项目使用坐标系 4490".into(),
                category: "fact".into(),
                source: "manual".into(),
                source_conversation_id: Some(chat.id.clone()),
                metadata: Some(serde_json::json!({"source_message_id": source.id}).to_string()),
            })
            .unwrap();
        assert!(!db.memory_source_removed(&memory.id).unwrap());
        db.mutate_conversation_message(&chat.id, &source.id, false)
            .unwrap();
        assert!(db.memory_source_removed(&memory.id).unwrap());
        let log = db.memory_activity(false, 20, 0).unwrap();
        let created = log
            .iter()
            .find(|event| event.memory_id.as_deref() == Some(&memory.id))
            .unwrap();
        assert!(created.source_removed);
        assert!(created.conversation_id.is_none());
        assert_eq!(db.get_conversation(&chat.id).unwrap().messages.len(), 1);
        assert_eq!(
            db.find_memory(&memory.id).unwrap().unwrap().content,
            "项目使用坐标系 4490"
        );
    }

    #[test]
    fn permanent_memory_removal_only_retains_the_action_and_redacts_sensitive_messages() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let chat = db.create_conversation("测试").unwrap();
        db.add_message(&message(chat.id.clone(), "api_key = placeholder"))
            .unwrap();
        let memory = db
            .create_memory(&CreateMemoryRequest {
                content: "使用深色主题".into(),
                category: "preference".into(),
                source: "manual".into(),
                source_conversation_id: Some(chat.id),
                metadata: None,
            })
            .unwrap();
        assert!(db
            .memory_activity(false, 20, 0)
            .unwrap()
            .iter()
            .any(|event| event.kind == "memory_created"));
        db.delete_memory(&memory.id).unwrap();
        assert!(db.memory_history(&memory.id).unwrap().is_empty());
        let journal = db.memory_activity(false, 20, 0).unwrap();
        assert_eq!(journal.len(), 2);
        assert!(journal
            .iter()
            .any(|event| event.kind == "memory_deleted" && event.content.is_none()));
        assert!(journal
            .iter()
            .any(|event| event.content.as_deref() == Some("此条消息含敏感内容，已隐藏正文。")));
    }

    #[test]
    fn permanent_memory_removal_erases_sync_payloads_and_rebases_deletion_on_replay() {
        use crate::modules::cloud::domain::SyncOperation;
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let id = "synced-memory";
        let payload = serde_json::json!({"schema_version":1,"content":"private body","category":"note","source":"manual"});
        db.apply_remote_sync_change("memory", id, 1, SyncOperation::Upsert, Some(&payload), 1)
            .unwrap();
        db.update_memory(
            id,
            &crate::db::UpdateMemoryRequest {
                content: Some("local edit".into()),
                category: None,
                metadata: None,
            },
        )
        .unwrap();
        db.capture_sync_candidates().unwrap();
        assert!(db
            .apply_remote_sync_change("memory", id, 2, SyncOperation::Upsert, Some(&payload), 2)
            .unwrap());
        db.delete_memory(id).unwrap();
        db.apply_remote_sync_change("memory", id, 3, SyncOperation::Upsert, Some(&payload), 3)
            .unwrap();

        assert!(db.find_memory(id).unwrap().is_none());
        assert!(db.memory_history(id).unwrap().is_empty());
        let conn = db.conn();
        let copies: i64 = conn.query_row("SELECT
            (SELECT COUNT(*) FROM cloud_sync_outbox WHERE entity_type='memory' AND entity_id=?1 AND payload_json IS NOT NULL)
            + (SELECT COUNT(*) FROM cloud_sync_conflicts WHERE entity_type='memory' AND entity_id=?1)
            + (SELECT COUNT(*) FROM cloud_synced_entities WHERE entity_type='memory' AND entity_id=?1 AND payload_json IS NOT NULL)", [id], |row| row.get(0)).unwrap();
        assert_eq!(copies, 0);
        let deletion: (String, i64) = conn.query_row("SELECT operation,base_revision FROM cloud_sync_outbox WHERE entity_type='memory' AND entity_id=?1", [id], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_eq!(deletion, ("delete".into(), 3));
    }

    #[test]
    fn historical_backfill_runs_once_and_does_not_restore_a_purged_entry() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let chat = db.create_conversation("旧对话").unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        {
            let conn = db.conn();
            conn.execute("DELETE FROM settings WHERE key=?1", [BACKFILL_KEY])
                .unwrap();
            conn.execute("INSERT INTO messages(id,conversation_id,role,content,created_at) VALUES('historical',?1,'user','旧问候',?2)", params![chat.id, now]).unwrap();
            conn.execute("INSERT INTO messages(id,conversation_id,role,content,created_at) VALUES('branch-copy',?1,'user','复制的消息',1)", [&chat.id]).unwrap();
        }
        db.initialize_memory_activity().unwrap();
        let entries = db.memory_activity(false, 20, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content.as_deref(), Some("旧问候"));
        assert!(db.remove_memory_activity(&entries[0].id).unwrap());
        db.initialize_memory_activity().unwrap();
        assert!(db.memory_activity(false, 20, 0).unwrap().is_empty());
        let recent = db.memory_activity(true, 20, 0).unwrap();
        assert_eq!(recent[0].content.as_deref(), Some("旧问候"));
    }
}
