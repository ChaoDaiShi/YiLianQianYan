//! Resolve graph scopes and effective version heads before building any projections.
use rusqlite::{params, Connection, OptionalExtension};

use super::storage;
use crate::db::{Memory, MemoryQuery};

#[derive(Default)]
pub(crate) struct GraphScope {
    pub workspace_id: Option<String>,
    pub conversation_id: Option<String>,
}

impl GraphScope {
    pub(super) fn resolve(&self, conn: &Connection) -> Result<Vec<String>, String> {
        let mut scopes = vec!["profile".to_string()];
        let conversation_workspace = if let Some(id) = self.conversation_id.as_deref() {
            let workspace = conn
                .query_row(
                    "SELECT w.id FROM conversations c
                 LEFT JOIN settings s ON s.key='conversation_workspace:'||c.id
                 LEFT JOIN workspaces w ON w.id=s.value AND w.status='active' WHERE c.id=?1",
                    [id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(storage)?
                .ok_or("memory_graph_scope_unavailable")?;
            scopes.push(format!("conversation:{id}"));
            workspace
        } else {
            None
        };
        if let Some(id) = self.workspace_id.as_deref() {
            if self.conversation_id.is_some() && conversation_workspace.as_deref() != Some(id) {
                return Err("memory_graph_scope_unavailable".into());
            }
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id=?1 AND status='active')",
                    [id],
                    |row| row.get(0),
                )
                .map_err(storage)?;
            if !exists {
                return Err("memory_graph_scope_unavailable".into());
            }
            scopes.push(format!("workspace:{id}"));
        } else if let Some(id) = conversation_workspace {
            scopes.push(format!("workspace:{id}"));
        }
        Ok(scopes)
    }
}

pub(super) struct VisibleHead {
    pub memory: Memory,
    pub revision: i64,
}

pub(super) fn read_heads(
    conn: &Connection,
    query: &MemoryQuery,
    scopes: &[String],
    withdrawn: bool,
    now: i64,
) -> Result<Vec<VisibleHead>, String> {
    let scopes = serde_json::to_string(scopes).map_err(|error| error.to_string())?;
    let status = if withdrawn { "withdrawn" } else { "active" };
    let mut stmt = conn.prepare(
        "WITH visible(memory_id, revision) AS (
           SELECT g.memory_id, MAX(v.revision)
           FROM memory_governance g JOIN memory_versions v ON v.memory_id=g.memory_id
           WHERE g.status=?1 AND g.scope IN (SELECT value FROM json_each(?2))
             AND ((?1='withdrawn' AND v.revision=g.revision)
               OR (?1='active' AND v.status='active' AND (g.valid_until IS NULL OR g.valid_until>?3)
                 AND (v.revision=g.revision OR g.valid_from>?3)))
             AND (?1='withdrawn' OR (
                (CASE WHEN v.revision=g.revision THEN g.valid_from ELSE v.valid_from END IS NULL
                 OR CASE WHEN v.revision=g.revision THEN g.valid_from ELSE v.valid_from END<=?3)
                AND (CASE WHEN v.revision=g.revision THEN g.valid_until ELSE v.valid_until END IS NULL
                 OR CASE WHEN v.revision=g.revision THEN g.valid_until ELSE v.valid_until END>?3)
             ))
             AND v.scope IN (SELECT value FROM json_each(?2))
             AND NOT EXISTS (SELECT 1 FROM knowledge_document_chunks c WHERE c.memory_id=g.memory_id)
           GROUP BY g.memory_id
         )
         SELECT m.id,v.content,v.category,v.source,
            CASE WHEN v.revision=g.revision THEN m.source_conversation_id ELSE x.conversation_id END,
            CASE WHEN v.revision=g.revision THEN m.embedding END,
            CASE WHEN v.revision=g.revision THEN m.metadata
                 ELSE json_object('relationship',CASE WHEN json_valid(x.relationship_json) THEN json(x.relationship_json) END,
                   'source_message_id',x.message_id,'source_graph_id',x.graph_id,'source_report_id',x.report_id) END,
            m.created_at,v.recorded_at,v.revision
         FROM visible h JOIN memory_versions v ON v.memory_id=h.memory_id AND v.revision=h.revision
          JOIN memories m ON m.id=h.memory_id JOIN memory_governance g ON g.memory_id=h.memory_id
          LEFT JOIN memory_version_details x ON x.memory_id=v.memory_id AND x.revision=v.revision
         WHERE (?4 IS NULL OR v.category=?4) AND (?5 IS NULL OR v.source=?5)
           AND (?6 IS NULL OR v.content LIKE ?6)
         ORDER BY v.recorded_at DESC,m.id LIMIT ?7 OFFSET ?8",
    ).map_err(storage)?;
    let rows = stmt
        .query_map(
            params![
                status,
                scopes,
                now,
                query.category,
                query.source,
                query.q.as_ref().map(|q| format!("%{q}%")),
                query.limit.unwrap_or(100) as i64,
                query.offset.unwrap_or_default() as i64,
            ],
            |row| {
                Ok(VisibleHead {
                    memory: Memory {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        category: row.get(2)?,
                        source: row.get(3)?,
                        source_conversation_id: row.get(4)?,
                        embedding: row.get(5)?,
                        metadata: row.get(6)?,
                        created_at: row.get(7)?,
                        updated_at: row.get(8)?,
                    },
                    revision: row.get(9)?,
                })
            },
        )
        .map_err(storage)?;
    rows.collect::<rusqlite::Result<_>>().map_err(storage)
}
