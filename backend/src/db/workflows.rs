// ============================================================
// Workflow persistence — CRUD operations for workflows table
// ============================================================

use super::Database;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub nodes: Vec<String>,
    pub tags: Vec<String>,
    pub system_prompt_extra: String,
    pub is_builtin: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

fn parse_json_field(opt: Option<String>) -> Vec<String> {
    opt.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

impl Database {
    pub fn list_workflows(&self) -> Result<Vec<Workflow>, rusqlite::Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, nodes, tags, system_prompt_extra, is_builtin, created_at, updated_at FROM workflows ORDER BY is_builtin DESC, created_at ASC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Workflow {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                nodes: parse_json_field(row.get(3)?),
                tags: parse_json_field(row.get(4)?),
                system_prompt_extra: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                is_builtin: row.get::<_, i32>(6)? != 0,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_workflow(&self, id: &str) -> Result<Option<Workflow>, rusqlite::Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, nodes, tags, system_prompt_extra, is_builtin, created_at, updated_at FROM workflows WHERE id = ?1"
        )?;
        let mut rows = stmt.query_map(rusqlite::params![id], |row| {
            Ok(Workflow {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                nodes: parse_json_field(row.get(3)?),
                tags: parse_json_field(row.get(4)?),
                system_prompt_extra: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                is_builtin: row.get::<_, i32>(6)? != 0,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn create_workflow(&self, wf: &Workflow) -> Result<(), rusqlite::Error> {
        let conn = self.conn();
        let nodes_json = serde_json::to_string(&wf.nodes).unwrap_or_default();
        let tags_json = serde_json::to_string(&wf.tags).unwrap_or_default();
        conn.execute(
            "INSERT INTO workflows (id, name, description, nodes, tags, system_prompt_extra, is_builtin, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            rusqlite::params![
                wf.id, wf.name, wf.description,
                nodes_json, tags_json, wf.system_prompt_extra,
                wf.is_builtin as i32, wf.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn update_workflow(&self, id: &str, wf: &Workflow) -> Result<(), rusqlite::Error> {
        let conn = self.conn();
        let nodes_json = serde_json::to_string(&wf.nodes).unwrap_or_default();
        let tags_json = serde_json::to_string(&wf.tags).unwrap_or_default();
        conn.execute(
            "UPDATE workflows SET name=?2, description=?3, nodes=?4, tags=?5, system_prompt_extra=?6, updated_at=?7 WHERE id=?1",
            rusqlite::params![
                id, wf.name, wf.description,
                nodes_json, tags_json, wf.system_prompt_extra,
                wf.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_workflow(&self, id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM workflows WHERE id = ?1 AND is_builtin = 0",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    pub fn get_active_workflow_id(&self) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.conn();
        let result = conn.query_row(
            "SELECT value FROM settings WHERE key = 'active_workflow_id'",
            [],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_active_workflow_id(&self, id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('active_workflow_id', ?1)",
            rusqlite::params![id],
        )?;
        Ok(())
    }
}
