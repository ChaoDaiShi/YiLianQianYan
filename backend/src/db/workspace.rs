// ============================================================
// Workspace persistence — workspaces table.
// ============================================================

use super::Database;
use crate::workspace::{Workspace, WorkspaceFieldError, WorkspaceId, WorkspaceStatus};
use rusqlite::OptionalExtension;

const DEFAULT_WORKSPACE_KEY: &str = "default_workspace_id";
const INITIAL_WORKSPACE_KEY: &str = "initial_workspace_id";

pub(super) fn default_workspace_on(conn: &rusqlite::Connection) -> Result<Option<String>, String> {
    active_workspace_setting(conn, DEFAULT_WORKSPACE_KEY)
}

fn active_workspace_setting(
    conn: &rusqlite::Connection,
    key: &str,
) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT w.id FROM settings s JOIN workspaces w ON w.id=s.value
         WHERE s.key=?1 AND w.status='active'",
        [key],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn select_default_on(conn: &rusqlite::Connection, id: &WorkspaceId) -> Result<bool, String> {
    conn.execute(
        "INSERT INTO settings(key,value)
         SELECT ?1,id FROM workspaces WHERE id=?2 AND status='active'
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![DEFAULT_WORKSPACE_KEY, id.as_str()],
    )
    .map(|changed| changed == 1)
    .map_err(|error| error.to_string())
}

fn reset_default_on(conn: &rusqlite::Connection, root: &str) -> Result<String, String> {
    let id = if let Some(id) = active_workspace_setting(conn, INITIAL_WORKSPACE_KEY)? {
        WorkspaceId::new(id).map_err(|error| error.to_string())?
    } else {
        let path = std::path::absolute(root).map_err(|error| error.to_string())?;
        let workspace = Workspace::new(
            WorkspaceId::generate(),
            "默认工作空间".into(),
            String::new(),
            Some(path.to_string_lossy().into_owned()),
            chrono::Utc::now().timestamp_millis(),
        )
        .map_err(|error| error.to_string())?;
        insert_workspace(conn, &workspace)?;
        conn.execute(
            "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            rusqlite::params![INITIAL_WORKSPACE_KEY, workspace.id.as_str()],
        )
        .map_err(|error| error.to_string())?;
        workspace.id
    };
    select_default_on(conn, &id)?;
    Ok(id.to_string())
}

impl Database {
    pub fn default_workspace_id(&self) -> Result<Option<String>, String> {
        default_workspace_on(&self.conn())
    }

    pub fn set_default_workspace(&self, id: &WorkspaceId) -> Result<bool, String> {
        select_default_on(&self.conn(), id)
    }

    pub(crate) fn ensure_default_workspace(&self, root: &str) -> Result<String, String> {
        self.resolve_default_workspace(root, false)
    }

    pub(crate) fn reset_default_workspace(&self, root: &str) -> Result<String, String> {
        self.resolve_default_workspace(root, true)
    }

    fn resolve_default_workspace(&self, root: &str, reset: bool) -> Result<String, String> {
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let id = match default_workspace_on(&tx)?.filter(|_| !reset) {
            Some(id) => id,
            None => reset_default_on(&tx, root)?,
        };
        tx.commit().map_err(|error| error.to_string())?;
        Ok(id)
    }

    pub fn create_workspace(&self, workspace: &Workspace) -> Result<(), String> {
        insert_workspace(&self.conn(), workspace)
    }

    pub fn get_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>, String> {
        Ok(self
            .get_workspace_for_history(id)?
            .filter(|workspace| workspace.status != WorkspaceStatus::Deleted))
    }

    /// Retain the original workspace and path for existing task and artifact records.
    pub(crate) fn get_workspace_for_history(
        &self,
        id: &WorkspaceId,
    ) -> Result<Option<Workspace>, String> {
        self.conn()
            .query_row(
                "SELECT id, name, description, root_path, status, created_at, updated_at, project_json
                 FROM workspaces WHERE id=?1",
                [id.as_str()],
                map_workspace_row,
            )
            .optional()
            .map_err(|error| error.to_string())
    }

    pub fn list_workspaces(&self) -> Result<Vec<Workspace>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, root_path, status, created_at, updated_at, project_json
                 FROM workspaces WHERE status!='deleted' ORDER BY updated_at DESC, id DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], map_workspace_row)
            .map_err(|error| error.to_string())?;
        rows.map(|row| row.map_err(|error| error.to_string()))
            .collect()
    }

    pub fn update_workspace(&self, workspace: &Workspace) -> Result<(), String> {
        self.update_workspace_with_project(workspace, false)
    }

    pub fn update_workspace_with_project(
        &self,
        workspace: &Workspace,
        update_project: bool,
    ) -> Result<(), String> {
        let conn = self.conn();
        let project = update_project
            .then(|| serde_json::to_string(&workspace.project))
            .transpose()
            .map_err(|error| error.to_string())?;
        conn.execute(
            "UPDATE workspaces SET name=?2, description=?3, root_path=?4, updated_at=?5,
                 project_json=COALESCE(?6, project_json)
             WHERE id=?1 AND status!='deleted'",
            rusqlite::params![
                workspace.id.as_str(),
                workspace.name,
                workspace.description,
                workspace.root_path,
                workspace.updated_at,
                project,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Archive a workspace. Historical tasks/executions/artifacts are retained.
    pub fn archive_workspace(
        &self,
        id: &WorkspaceId,
        now: i64,
        default_root: Option<&str>,
    ) -> Result<(), String> {
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        tx.execute(
            "UPDATE workspaces SET status='archived', updated_at=?2 WHERE id=?1 AND status!='deleted'",
            rusqlite::params![id.as_str(), now],
        )
        .map_err(|error| error.to_string())?;
        if let Some(root) = default_root {
            if default_workspace_on(&tx)?.is_none() {
                reset_default_on(&tx, root)?;
            }
        }
        tx.commit().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn restore_workspace(&self, id: &WorkspaceId, now: i64) -> Result<bool, String> {
        self.conn()
            .execute(
                "UPDATE workspaces SET status='active', updated_at=?2 WHERE id=?1 AND status='archived'",
                rusqlite::params![id.as_str(), now],
            )
            .map(|changed| changed == 1)
            .map_err(|error| error.to_string())
    }

    /// Remove the registration without removing rows linked to its original ID.
    pub fn remove_archived_workspace(&self, id: &WorkspaceId, now: i64) -> Result<bool, String> {
        self.conn()
            .execute(
                "UPDATE workspaces SET status='deleted', updated_at=?2 WHERE id=?1 AND status='archived'",
                rusqlite::params![id.as_str(), now],
            )
            .map(|changed| changed == 1)
            .map_err(|error| error.to_string())
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

fn insert_workspace(conn: &rusqlite::Connection, workspace: &Workspace) -> Result<(), String> {
    let project = serde_json::to_string(&workspace.project).map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO workspaces (id, name, description, root_path, status, created_at, updated_at, project_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            workspace.id.as_str(), workspace.name, workspace.description, workspace.root_path,
            workspace.status.to_string(), workspace.created_at, workspace.updated_at, project,
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn map_workspace_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    let status: String = row.get(4)?;
    let status = match status.as_str() {
        "active" => WorkspaceStatus::Active,
        "archived" => WorkspaceStatus::Archived,
        "deleted" => WorkspaceStatus::Deleted,
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
        project: serde_json::from_str(&row.get::<_, String>(7)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}
