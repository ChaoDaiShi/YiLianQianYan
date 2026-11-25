//! Immutable ProofLoop goal binding for a chat conversation.
use rusqlite::{params, OptionalExtension, TransactionBehavior};

use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};

const CONVERSATION_GOAL_BINDINGS: ProductMigrationSpec = ProductMigrationSpec::new(
    1019,
    "1019_conversation_goal_bindings",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE conversation_goal_bindings (
        conversation_id TEXT PRIMARY KEY NOT NULL,
        goal_id TEXT NOT NULL,
        goal_version INTEGER NOT NULL CHECK(goal_version > 0),
        created_at INTEGER NOT NULL,
        FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
    );
    CREATE INDEX conversation_goal_bindings_goal
        ON conversation_goal_bindings(goal_id, goal_version);
    CREATE TRIGGER conversation_goal_bindings_delete
        AFTER DELETE ON conversations
        BEGIN
            DELETE FROM conversation_goal_bindings WHERE conversation_id=OLD.id;
        END;",
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationGoalBinding {
    pub conversation_id: String,
    pub goal_id: String,
    pub goal_version: u32,
}

impl Database {
    pub fn initialize_conversation_goal_bindings(&self) -> Result<(), String> {
        self.apply_product_migration(&CONVERSATION_GOAL_BINDINGS)
            .map_err(|error| error.to_string())
    }

    pub fn load_conversation_goal_binding(
        &self,
        conversation_id: &str,
    ) -> Result<Option<ConversationGoalBinding>, String> {
        self.initialize_conversation_goal_bindings()?;
        self.conn()
            .query_row(
                "SELECT conversation_id, goal_id, goal_version
                 FROM conversation_goal_bindings WHERE conversation_id=?1",
                [conversation_id],
                |row| {
                    Ok(ConversationGoalBinding {
                        conversation_id: row.get(0)?,
                        goal_id: row.get(1)?,
                        goal_version: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(|error| error.to_string())
    }

    /// Insert the conversation's first binding, or confirm that an existing
    /// binding is identical. Returns `false` when the conversation is already
    /// bound to another goal version.
    pub fn bind_conversation_goal(
        &self,
        binding: &ConversationGoalBinding,
    ) -> Result<bool, String> {
        self.bind_conversation_goal_with_workspace(binding, None)
    }

    /// Save the existing immutable goal binding and its workspace together.
    pub fn bind_conversation_goal_with_workspace(
        &self,
        binding: &ConversationGoalBinding,
        workspace_id: Option<&str>,
    ) -> Result<bool, String> {
        self.initialize_conversation_goal_bindings()?;
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let inserted = tx
            .execute(
                "INSERT INTO conversation_goal_bindings
             (conversation_id, goal_id, goal_version, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(conversation_id) DO NOTHING",
                params![
                    binding.conversation_id,
                    binding.goal_id,
                    binding.goal_version,
                    chrono::Utc::now().timestamp_millis()
                ],
            )
            .map_err(|error| error.to_string())?;
        let existing: (String, u32) = tx
            .query_row(
                "SELECT goal_id, goal_version FROM conversation_goal_bindings
                 WHERE conversation_id=?1",
                [&binding.conversation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| error.to_string())?;
        if existing.0 != binding.goal_id || existing.1 != binding.goal_version {
            return Ok(false);
        }
        let mut workspace_changed = false;
        if let Some(workspace_id) = workspace_id {
            let key = format!("conversation_workspace:{}", binding.conversation_id);
            let current: Option<String> = tx
                .query_row("SELECT value FROM settings WHERE key=?1", [&key], |row| {
                    row.get(0)
                })
                .optional()
                .map_err(|error| error.to_string())?;
            if current
                .as_deref()
                .is_some_and(|current| current != workspace_id)
            {
                return Err("conversation_goal_workspace_conflict".into());
            }
            workspace_changed = current.as_deref() != Some(workspace_id);
            tx.execute(
                "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, workspace_id],
            ).map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())?;
        drop(conn);
        if inserted > 0 || workspace_changed {
            self.publish_goal_conversation_change(
                &binding.conversation_id,
                "conversation_bound",
                None,
            );
        }
        Ok(true)
    }

    pub fn goal_conversation_ids(
        &self,
        goal_id: &str,
        goal_version: u32,
    ) -> Result<Vec<String>, String> {
        self.initialize_conversation_goal_bindings()?;
        let conn = self.conn();
        let mut query = conn.prepare(
            "SELECT conversation_id FROM conversation_goal_bindings WHERE goal_id=?1 AND goal_version=?2 ORDER BY created_at,conversation_id",
        ).map_err(|error| error.to_string())?;
        let ids = query
            .query_map(params![goal_id, goal_version], |row| row.get(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<String>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(ids)
    }

    pub(crate) fn graph_goal_conversation_ids(
        &self,
        graph_id: &str,
    ) -> Result<Vec<String>, String> {
        self.initialize_conversation_goal_bindings()?;
        self.initialize_proofloop_schema()
            .map_err(|error| error.to_string())?;
        let conn = self.conn();
        let mut query = conn.prepare("SELECT DISTINCT b.conversation_id FROM conversation_goal_bindings b JOIN proofloop_drafts d ON d.goal_id=b.goal_id AND d.goal_version=b.goal_version WHERE d.graph_id=?1 AND d.status='confirmed' ORDER BY b.conversation_id").map_err(|error| error.to_string())?;
        let ids = query
            .query_map([graph_id], |row| row.get(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<String>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(ids)
    }

    pub(crate) fn active_goal_conversation_graph_ids(&self) -> Result<Vec<String>, String> {
        self.initialize_conversation_goal_bindings()?;
        self.initialize_proofloop_schema()
            .map_err(|error| error.to_string())?;
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT DISTINCT d.graph_id
                 FROM proofloop_drafts d
                 JOIN conversation_goal_bindings b
                   ON b.goal_id = d.goal_id AND b.goal_version = d.goal_version
                 JOIN conversations c ON c.id = b.conversation_id
                 WHERE d.status = 'confirmed'
                   AND c.run_status IN ('running', 'waiting_approval', 'waiting_input')",
            )
            .map_err(|error| error.to_string())?;
        let ids = statement
            .query_map([], |row| row.get(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_goal_binding_is_immutable_and_repeatable() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let conversation = db.create_conversation("goal chat").unwrap();
        let binding = ConversationGoalBinding {
            conversation_id: conversation.id.clone(),
            goal_id: "goal-1".to_string(),
            goal_version: 3,
        };
        assert!(db.bind_conversation_goal(&binding).unwrap());
        assert!(db.bind_conversation_goal(&binding).unwrap());
        assert!(!db
            .bind_conversation_goal(&ConversationGoalBinding {
                goal_id: "goal-2".to_string(),
                goal_version: 1,
                ..binding.clone()
            })
            .unwrap());
        assert_eq!(
            db.load_conversation_goal_binding(&conversation.id)
                .unwrap()
                .unwrap(),
            ConversationGoalBinding {
                conversation_id: conversation.id,
                goal_id: "goal-1".to_string(),
                goal_version: 3,
            }
        );
    }
}
