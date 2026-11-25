use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1122,
    "1122_rich_component_state",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE message_component_states (
        message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
        component_id TEXT NOT NULL,
        state_version INTEGER NOT NULL CHECK(state_version > 0),
        state_json TEXT NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY(message_id, component_id)
    );",
);

#[derive(Serialize)]
pub(crate) struct ComponentState {
    pub state_version: i64,
    pub state: Value,
}

impl Database {
    pub(crate) fn initialize_rich_components(&self) -> rusqlite::Result<()> {
        self.apply_product_migration(&SCHEMA)
    }

    pub(crate) fn rich_message_source(
        &self,
        conversation: &str,
        message: &str,
    ) -> rusqlite::Result<Option<String>> {
        self.conn().query_row(
            "SELECT content FROM messages WHERE id=?1 AND conversation_id=?2 AND role='assistant'",
            params![message, conversation], |row| row.get(0),
        ).optional()
    }

    pub(crate) fn conversation_owns_rich_resource(
        &self,
        conversation: &str,
        resource: &str,
    ) -> rusqlite::Result<bool> {
        self.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM conversation_outputs WHERE conversation_id=?1 AND resource_id=?2 AND status='ready')
              OR EXISTS(SELECT 1 FROM message_attachments a JOIN messages m ON m.id=a.message_id
                WHERE m.conversation_id=?1 AND a.resource_id=?2)
              OR EXISTS(SELECT 1 FROM resource_bindings
                WHERE target_kind='conversation' AND target_id=?1 AND resource_id=?2 AND owner='local-user')",
            params![conversation, resource], |row| row.get(0),
        )
    }

    pub(crate) fn component_states(
        &self,
        message: &str,
    ) -> rusqlite::Result<BTreeMap<String, ComponentState>> {
        let conn = self.conn();
        let mut statement = conn.prepare("SELECT component_id,state_version,state_json FROM message_component_states WHERE message_id=?1")?;
        let rows = statement.query_map([message], |row| {
            let content: String = row.get(2)?;
            let state = serde_json::from_str(&content).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok((
                row.get(0)?,
                ComponentState {
                    state_version: row.get(1)?,
                    state,
                },
            ))
        })?;
        rows.collect()
    }

    pub(crate) fn save_component_state(
        &self,
        message: &str,
        component: &str,
        version: i64,
        state: Value,
    ) -> rusqlite::Result<Option<ComponentState>> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<i64> = tx.query_row(
            "SELECT state_version FROM message_component_states WHERE message_id=?1 AND component_id=?2",
            params![message, component], |row| row.get(0),
        ).optional()?;
        if version != current.unwrap_or(0) {
            return Ok(None);
        }
        let state_version = version + 1;
        tx.execute(
            "INSERT INTO message_component_states(message_id,component_id,state_version,state_json,updated_at)
             VALUES(?1,?2,?3,?4,?5) ON CONFLICT(message_id,component_id) DO UPDATE SET
             state_version=excluded.state_version,state_json=excluded.state_json,updated_at=excluded.updated_at",
            params![message, component, state_version, state.to_string(), chrono::Utc::now().timestamp_millis()],
        )?;
        tx.commit()?;
        Ok(Some(ComponentState {
            state_version,
            state,
        }))
    }
}
