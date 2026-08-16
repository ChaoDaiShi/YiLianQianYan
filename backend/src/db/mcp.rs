// ============================================================
// MCP Server persistence — CRUD operations for mcp_servers table
// ============================================================

use super::Database;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::secret::SecretRef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    /// For `streamable_http`: Header name → environment-variable *name*.
    /// For `stdio`: MUST be empty — literal values are forbidden (see guard).
    pub env: Option<serde_json::Value>,
    /// stdio env secret refs (env name → SecretRef). Values live in SecretStore.
    #[serde(default)]
    pub env_secret_refs: BTreeMap<String, SecretRef>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

fn parse_opt_json<T: for<'a> serde::Deserialize<'a>>(opt: Option<String>) -> Option<T> {
    opt.and_then(|s| serde_json::from_str(&s).ok())
}

/// Defense-in-depth: refuse to persist plaintext env values for stdio servers.
/// All stdio env (secret or not) must go through `env_secret_refs`.
fn ensure_no_stdio_plaintext_env(server: &McpServer) -> Result<(), String> {
    if server.transport == "stdio" && server.env.is_some() {
        return Err(
            "SecretPersistenceViolation: stdio MCP env must use env_secret_refs, not plaintext values"
                .to_string(),
        );
    }
    Ok(())
}

impl Database {
    pub fn list_mcp_servers(&self) -> Result<Vec<McpServer>, rusqlite::Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, transport, command, args, url, env, env_secret_refs, enabled, created_at, updated_at FROM mcp_servers ORDER BY created_at"
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
                env_secret_refs: parse_opt_json(row.get(7)?).unwrap_or_default(),
                enabled: row.get::<_, i32>(8)? != 0,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_mcp_server(&self, id: &str) -> Result<Option<McpServer>, rusqlite::Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, transport, command, args, url, env, env_secret_refs, enabled, created_at, updated_at FROM mcp_servers WHERE id = ?1"
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
                env_secret_refs: parse_opt_json(row.get(7)?).unwrap_or_default(),
                enabled: row.get::<_, i32>(8)? != 0,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn create_mcp_server(&self, server: &McpServer) -> Result<(), String> {
        ensure_no_stdio_plaintext_env(server)?;
        let conn = self.conn();
        let args_json = server
            .args
            .as_ref()
            .map(|a| serde_json::to_string(a).unwrap_or_default());
        let env_json = server
            .env
            .as_ref()
            .map(|e| serde_json::to_string(e).unwrap_or_default());
        let env_secret_refs_json = if server.env_secret_refs.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&server.env_secret_refs).unwrap_or_default())
        };
        conn.execute(
            "INSERT INTO mcp_servers (id, name, transport, command, args, url, env, env_secret_refs, enabled, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
            rusqlite::params![
                server.id, server.name, server.transport,
                server.command, args_json, server.url, env_json,
                env_secret_refs_json,
                server.enabled as i32, server.created_at,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_mcp_server(&self, id: &str, server: &McpServer) -> Result<(), String> {
        ensure_no_stdio_plaintext_env(server)?;
        let conn = self.conn();
        let args_json = server
            .args
            .as_ref()
            .map(|a| serde_json::to_string(a).unwrap_or_default());
        let env_json = server
            .env
            .as_ref()
            .map(|e| serde_json::to_string(e).unwrap_or_default());
        let env_secret_refs_json = if server.env_secret_refs.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&server.env_secret_refs).unwrap_or_default())
        };
        conn.execute(
            "UPDATE mcp_servers SET name=?2, transport=?3, command=?4, args=?5, url=?6, env=?7, env_secret_refs=?8, enabled=?9, updated_at=?10 WHERE id=?1",
            rusqlite::params![
                id, server.name, server.transport,
                server.command, args_json, server.url, env_json,
                env_secret_refs_json,
                server.enabled as i32, server.updated_at,
            ],
        )
        .map_err(|e| e.to_string())?;
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
