// ============================================================
// MCP Server persistence — CRUD operations for mcp_servers table
// ============================================================

use super::Database;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    pub env: Option<serde_json::Value>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

fn parse_opt_json<T: for<'a> serde::Deserialize<'a>>(opt: Option<String>) -> Option<T> {
    opt.and_then(|s| serde_json::from_str(&s).ok())
}

impl Database {
    pub fn list_mcp_servers(&self) -> Result<Vec<McpServer>, rusqlite::Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, transport, command, args, url, env, enabled, created_at, updated_at FROM mcp_servers ORDER BY created_at"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(McpServer {
                id: row.get(0)?,
                name: row.get(1)?,
                transport: row.get(2)?,
                command: row.get(3)?,
                args: parse_opt_json(row.get(4)?),
                url: row.get(5)?,
                env: parse_opt_json(row.get(6)?),
                enabled: row.get::<_, i32>(7)? != 0,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_mcp_server(&self, id: &str) -> Result<Option<McpServer>, rusqlite::Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, transport, command, args, url, env, enabled, created_at, updated_at FROM mcp_servers WHERE id = ?1"
        )?;
        let mut rows = stmt.query_map(rusqlite::params![id], |row| {
            Ok(McpServer {
                id: row.get(0)?,
                name: row.get(1)?,
                transport: row.get(2)?,
                command: row.get(3)?,
                args: parse_opt_json(row.get(4)?),
                url: row.get(5)?,
                env: parse_opt_json(row.get(6)?),
                enabled: row.get::<_, i32>(7)? != 0,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn create_mcp_server(&self, server: &McpServer) -> Result<(), rusqlite::Error> {
        let conn = self.conn();
        let args_json = server
            .args
            .as_ref()
            .map(|a| serde_json::to_string(a).unwrap_or_default());
        let env_json = server
            .env
            .as_ref()
            .map(|e| serde_json::to_string(e).unwrap_or_default());
        conn.execute(
            "INSERT INTO mcp_servers (id, name, transport, command, args, url, env, enabled, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            rusqlite::params![
                server.id, server.name, server.transport,
                server.command, args_json, server.url, env_json,
                server.enabled as i32, server.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn update_mcp_server(&self, id: &str, server: &McpServer) -> Result<(), rusqlite::Error> {
        let conn = self.conn();
        let args_json = server
            .args
            .as_ref()
            .map(|a| serde_json::to_string(a).unwrap_or_default());
        let env_json = server
            .env
            .as_ref()
            .map(|e| serde_json::to_string(e).unwrap_or_default());
        conn.execute(
            "UPDATE mcp_servers SET name=?2, transport=?3, command=?4, args=?5, url=?6, env=?7, enabled=?8, updated_at=?9 WHERE id=?1",
            rusqlite::params![
                id, server.name, server.transport,
                server.command, args_json, server.url, env_json,
                server.enabled as i32, server.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_mcp_server(&self, id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM mcp_servers WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    pub fn toggle_mcp_server(&self, id: &str) -> Result<Option<McpServer>, rusqlite::Error> {
        let conn = self.conn();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE mcp_servers SET enabled = 1 - enabled, updated_at = ?2 WHERE id = ?1",
            rusqlite::params![id, now],
        )?;
        drop(conn);
        self.get_mcp_server(id)
    }
}
