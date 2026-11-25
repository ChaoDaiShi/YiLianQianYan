use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1088,
    "1088_conversation_lanes",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE conversation_lanes (
        conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
        parent_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
        kind TEXT NOT NULL CHECK(kind IN ('chat','work'))
    );
    CREATE INDEX conversation_lanes_parent ON conversation_lanes(parent_id);",
);

#[derive(Serialize)]
pub struct ConversationLane {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub run_status: String,
}

impl Database {
    pub(crate) fn initialize_conversation_lanes(&self) -> rusqlite::Result<()> {
        self.apply_product_migration(&SCHEMA)
    }

    pub(crate) fn conversation_lane(&self, id: &str) -> rusqlite::Result<Option<(String, String)>> {
        self.conn()
            .query_row(
                "SELECT parent_id,kind FROM conversation_lanes WHERE conversation_id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
    }

    pub(crate) fn conversation_lanes(
        &self,
        parent: &str,
    ) -> rusqlite::Result<Vec<ConversationLane>> {
        let conn = self.conn();
        let mut query = conn.prepare("SELECT c.id,c.title,l.kind,c.run_status FROM conversation_lanes l JOIN conversations c ON c.id=l.conversation_id
            WHERE l.parent_id=?1 ORDER BY CASE WHEN c.run_status IN ('running','waiting_approval','waiting_input') THEN 0 ELSE 1 END, c.updated_at DESC,c.rowid DESC")?;
        let rows = query.query_map([parent], |row| {
            Ok(ConversationLane {
                id: row.get(0)?,
                title: row.get(1)?,
                kind: row.get(2)?,
                run_status: row.get(3)?,
            })
        })?;
        rows.collect()
    }

    pub(crate) fn create_conversation_lane(
        &self,
        parent: &str,
        title: &str,
        kind: &str,
    ) -> rusqlite::Result<ConversationLane> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1)",
            [parent],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        tx.execute(
            "INSERT INTO conversations(id,title,created_at,updated_at) VALUES(?1,?2,?3,?3)",
            params![id, title, now],
        )?;
        tx.execute(
            "INSERT INTO conversation_lanes VALUES(?1,?2,?3)",
            params![id, parent, kind],
        )?;
        tx.execute("INSERT INTO settings(key,value) SELECT 'conversation_workspace:'||?1,value FROM settings WHERE key='conversation_workspace:'||?2", params![id,parent])?;
        tx.commit()?;
        Ok(ConversationLane {
            id,
            title: title.into(),
            kind: kind.into(),
            run_status: "idle".into(),
        })
    }

    pub(crate) fn conversation_lane_context(
        &self,
        id: &str,
    ) -> rusqlite::Result<Option<(String, Vec<(String, String)>)>> {
        let Some((parent, _)) = self.conversation_lane(id)? else {
            return Ok(None);
        };
        let conn = self.conn();
        let mut query = conn.prepare("SELECT role,substr(content,1,500) FROM messages
            WHERE conversation_id=?1 AND role IN ('user','assistant') AND tool_calls IS NULL AND length(trim(content))>0
            AND (SELECT value FROM settings WHERE key='conversation_workspace:'||?1) IS (SELECT value FROM settings WHERE key='conversation_workspace:'||?2)
            AND NOT EXISTS (SELECT 1 FROM conversation_turns t WHERE t.conversation_id=?1 AND t.finished_at IS NULL AND messages.created_at>=t.started_at)
            ORDER BY created_at DESC,rowid DESC LIMIT 4")?;
        let entries = query
            .query_map([&parent, id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .rev()
            .filter(|(_, text)| !crate::safety::contains_sensitive_content(text))
            .collect();
        Ok(Some((parent, entries)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_runs_are_local_and_do_not_become_standalone_cloud_conversations() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let root = db.create_conversation("主对话").unwrap();
        let lane = db
            .create_conversation_lane(&root.id, "并行运行", "work")
            .unwrap();
        for (id, text) in [(&root.id, "主消息"), (&lane.id, "运行消息")] {
            db.persist_chat_message(
                id,
                &crate::integrations::llm::types::ChatMessage::user(text),
                1,
            )
            .unwrap();
        }
        assert_eq!(db.list_conversations().unwrap().len(), 1);
        let mut conn = db.conn();
        let tx = conn.transaction().unwrap();
        use crate::db::cloud::capture::{collect_sync_sources, CaptureReport, SyncPolicy};
        let sources =
            collect_sync_sources(&tx, &SyncPolicy::default(), &mut CaptureReport::default())
                .unwrap();
        assert!(sources.iter().any(|entry| entry.entity_id == root.id));
        assert!(
            !sources
                .iter()
                .any(|entry| entry.entity_id == lane.id
                    || entry.payload["conversation_id"] == lane.id)
        );
    }
}
