// ============================================================
// Workspace persistence — workspaces table.
// ============================================================

use super::Database;
use crate::workspace::{Workspace, WorkspaceFieldError, WorkspaceId, WorkspaceStatus};

impl Database {
    pub fn create_workspace(&self, workspace: &Workspace) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO workspaces (id, name, description, root_path, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                workspace.id.as_str(),
                workspace.name,
                workspace.description,
                workspace.root_path,
                workspace.status.to_string(),
                workspace.created_at,
                workspace.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, root_path, status, created_at, updated_at
                 FROM workspaces WHERE id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(rusqlite::params![id.as_str()], map_workspace_row)
            .map_err(|error| error.to_string())?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|error| error.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn list_workspaces(&self) -> Result<Vec<Workspace>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, root_path, status, created_at, updated_at
                 FROM workspaces ORDER BY updated_at DESC, id DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], map_workspace_row)
            .map_err(|error| error.to_string())?;
        rows.map(|row| row.map_err(|error| error.to_string()))
            .collect()
    }

    pub fn update_workspace(&self, workspace: &Workspace) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "UPDATE workspaces SET name=?2, description=?3, root_path=?4, status=?5, updated_at=?6
             WHERE id=?1",
            rusqlite::params![
                workspace.id.as_str(),
                workspace.name,
                workspace.description,
                workspace.root_path,
                workspace.status.to_string(),
                workspace.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Archive a workspace. Historical tasks/executions/artifacts are retained.
    pub fn archive_workspace(&self, id: &WorkspaceId, now: i64) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "UPDATE workspaces SET status='archived', updated_at=?2 WHERE id=?1",
            rusqlite::params![id.as_str(), now],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Count active tasks in a workspace (for the desktop card).
    pub fn count_active_tasks(&self, workspace_id: &WorkspaceId) -> Result<usize, String> {
        let conn = self.conn();
        conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE workspace_id = ?1 AND status IN
                ('draft','ready','running','waiting_approval','waiting_user','blocked')",
            [workspace_id.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count as usize)
        .map_err(|error| error.to_string())
    }
}

fn map_workspace_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    let status: String = row.get(4)?;
    let status = match status.as_str() {
        "active" => WorkspaceStatus::Active,
        "archived" => WorkspaceStatus::Archived,
        _other => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(WorkspaceFieldError::InvalidId),
            ))
        }
    };
    Ok(Workspace {
        id: WorkspaceId::new(row.get::<_, String>(0)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        name: row.get(1)?,
        description: row.get(2)?,
        root_path: row.get(3)?,
        status,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}
