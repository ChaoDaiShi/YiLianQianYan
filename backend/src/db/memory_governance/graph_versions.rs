//! Graph history projects actual changes from the existing governed revisions.
use super::{Database, MemoryVersion, MigrationOwner, ProductMigrationSpec};
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;

const HISTORY: ProductMigrationSpec = ProductMigrationSpec::new(
    1091,
    "1091_memory_graph_versions",
    MigrationOwner::V1TaskWorld,
    include_str!("graph_versions.sql"),
);

const READ_HISTORY: &str = "
SELECT v.memory_id,v.revision,v.content,v.category,v.source,v.source_ref,v.scope,v.status,v.reason,
 v.recorded_at,v.valid_from,v.valid_until,x.relationship_json,
 COALESCE(x.relationship_known,0),COALESCE(x.baseline,v.reason='legacy import; real-world validity unknown'),
 c.id,origin.id,
 (x.conversation_id IS NOT NULL OR x.message_id IS NOT NULL OR removed.memory_id IS NOT NULL)
   AND (c.id IS NULL OR (x.message_id IS NOT NULL AND
     (origin.id IS NULL OR removed_activity.message_id IS NOT NULL))),
 x.graph_id,x.report_id,COALESCE(x.captured_with_revision,0)
FROM memory_versions v
LEFT JOIN memory_version_details x ON x.memory_id=v.memory_id AND x.revision=v.revision
LEFT JOIN removed_memory_sources removed ON removed.memory_id=v.memory_id AND removed.conversation_id=v.source_ref
LEFT JOIN conversations c ON c.id=COALESCE(x.conversation_id,v.source_ref)
LEFT JOIN messages origin ON origin.id=x.message_id AND origin.conversation_id=c.id
LEFT JOIN memory_candidate_removed_activity removed_activity ON removed_activity.message_id=origin.id
WHERE v.memory_id IN (SELECT value FROM json_each(?1))
ORDER BY v.memory_id, v.revision";

fn same_state(older: &MemoryVersion, newer: &MemoryVersion) -> bool {
    older.content == newer.content
        && older.category == newer.category
        && older.source == newer.source
        && older.source_ref == newer.source_ref
        && older.scope == newer.scope
        && older.status == newer.status
        && older.valid_from == newer.valid_from
        && older.valid_until == newer.valid_until
        && (!older.relationship_known
            || !newer.relationship_known
            || older.relationship == newer.relationship)
}

impl Database {
    pub(in crate::db) fn initialize_memory_graph_versions(&self) -> rusqlite::Result<()> {
        self.apply_product_migration(&HISTORY)
    }

    pub fn memory_history(&self, id: &str) -> Result<Vec<MemoryVersion>, String> {
        let conn = self.conn();
        Ok(Self::memory_histories_on(&conn, &[id.to_owned()])?
            .remove(id)
            .unwrap_or_default())
    }

    pub fn memory_histories(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Vec<MemoryVersion>>, String> {
        let conn = self.conn();
        Self::memory_histories_on(&conn, ids)
    }

    pub(in crate::db) fn memory_histories_on(
        conn: &Connection,
        ids: &[String],
    ) -> Result<HashMap<String, Vec<MemoryVersion>>, String> {
        let mut result: HashMap<String, Vec<MemoryVersion>> = HashMap::new();
        if ids.is_empty() {
            return Ok(result);
        }
        let ids = serde_json::to_string(ids).map_err(|error| error.to_string())?;
        let mut statement = conn
            .prepare(READ_HISTORY)
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([ids], |row| {
                let id: String = row.get(0)?;
                let revision: i64 = row.get(1)?;
                let mut source_conversation_id = row.get(15)?;
                let mut source_message_id = row.get(16)?;
                let mut source_removed: bool = row.get(17)?;
                let graph: Option<String> = row.get(18)?;
                let report: Option<String> = row.get(19)?;
                if let Some((graph, report)) = graph.as_deref().zip(report.as_deref()) {
                    source_removed |=
                        !crate::db::memory_candidates::live_report_exists(conn, graph, report)?;
                }
                if source_removed {
                    source_conversation_id = None;
                    source_message_id = None;
                }
                Ok((
                    id.clone(),
                    row.get::<_, bool>(20)?,
                    MemoryVersion {
                        version_id: format!("{id}:{revision}"),
                        layer: 0,
                        baseline: row.get(14)?,
                        relationship_known: row.get(13)?,
                        relationship: row
                            .get::<_, Option<String>>(12)?
                            .and_then(|json| serde_json::from_str(&json).ok()),
                        source_conversation_id,
                        source_message_id,
                        source_removed,
                        revision,
                        content: row.get(2)?,
                        category: row.get(3)?,
                        source: row.get(4)?,
                        source_ref: row.get(5)?,
                        scope: row.get(6)?,
                        status: row.get(7)?,
                        reason: row.get(8)?,
                        recorded_at: row.get(9)?,
                        valid_from: row.get(10)?,
                        valid_until: row.get(11)?,
                    },
                ))
            })
            .map_err(|error| error.to_string())?;
        for row in rows {
            let (id, captured_with_revision, mut version) =
                row.map_err(|error| error.to_string())?;
            let history = result.entry(id).or_default();
            if let Some(previous) = history.last_mut() {
                if !captured_with_revision && same_state(previous, &version) {
                    // Legacy reason-only corrections can advance the optimistic
                    // revision without a proven memory state change.
                    version.layer = previous.layer;
                    version.baseline |= previous.baseline;
                    *previous = version;
                    continue;
                }
            }
            version.layer = history.len();
            history.push(version);
        }
        for history in result.values_mut() {
            history.reverse();
        }
        Ok(result)
    }

    pub(in crate::db) fn mark_remote_memory_snapshot_on(
        conn: &Connection,
        id: &str,
        previous_revision: Option<i64>,
        metadata_provided: bool,
    ) -> rusqlite::Result<()> {
        let existing: Option<i64> = conn
            .query_row(
                "SELECT revision FROM memory_governance WHERE memory_id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(revision) = existing.filter(|revision| Some(*revision) != previous_revision) {
            conn.execute(
                "UPDATE memory_version_details SET baseline=CASE WHEN ?4 THEN 1 ELSE baseline END,
                 relationship_known=CASE WHEN ?3 OR relationship_json IS NOT NULL THEN 1 ELSE 0 END
                 WHERE memory_id=?1 AND revision=?2",
                rusqlite::params![id, revision, metadata_provided, previous_revision.is_none()],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_keeps_verified_text_history_and_does_not_invent_older_relationships() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE memories(id TEXT PRIMARY KEY, content TEXT, category TEXT, source TEXT,
                metadata TEXT, source_conversation_id TEXT, created_at INTEGER, updated_at INTEGER, embedding TEXT);
             CREATE TABLE conversations(id TEXT PRIMARY KEY);
             CREATE TABLE messages(id TEXT PRIMARY KEY, conversation_id TEXT);
             CREATE TABLE memory_candidate_removed_activity(message_id TEXT PRIMARY KEY);
             CREATE TABLE removed_memory_sources(memory_id TEXT PRIMARY KEY, conversation_id TEXT);
             INSERT INTO memories VALUES('legacy','来源不明的历史基线','note','manual',NULL,NULL,1,1,NULL);",
        ).unwrap();
        conn.execute_batch(super::super::SCHEMA.sql).unwrap();
        conn.execute_batch(
            "INSERT INTO memories VALUES('v1','起点 · 依赖 · 终点','fact','manual',
                '{\"relationship\":{\"kind\":\"project\",\"directed\":true}}',NULL,10,10,NULL);
             UPDATE memories SET content='起点 · 关联 · 终点',updated_at=20 WHERE id='v1';
             UPDATE memory_governance SET revision=3,reason='重复确认' WHERE memory_id='v1';
             INSERT INTO memory_versions VALUES('v1',3,'起点 · 关联 · 终点','fact','manual',NULL,'profile','active','重复确认',30,NULL,NULL);",
        ).unwrap();
        conn.execute_batch(HISTORY.sql).unwrap();
        let histories =
            Database::memory_histories_on(&conn, &["v1".into(), "legacy".into()]).unwrap();
        assert_eq!(histories["v1"].len(), 2);
        assert_eq!(histories["v1"][0].version_id, "v1:3");
        assert_eq!(histories["v1"][0].layer, 1);
        assert!(histories["v1"][0].relationship_known);
        assert_eq!(histories["v1"][1].version_id, "v1:1");
        assert!(!histories["v1"][1].relationship_known);
        assert!(histories["v1"][1].relationship.is_none());
        assert!(histories["legacy"][0].baseline);
        assert_eq!(histories["legacy"][0].layer, 0);
        conn.execute_batch(
            "UPDATE memories SET content='再更新的有效内容',updated_at=40 WHERE id='v1'",
        )
        .unwrap();
        let next = Database::memory_histories_on(&conn, &["v1".into()]).unwrap();
        assert_eq!(next["v1"][0].version_id, "v1:4");
        assert_eq!(next["v1"][0].layer, 2);
        assert_eq!(next["v1"][1].version_id, "v1:3");
    }
}
