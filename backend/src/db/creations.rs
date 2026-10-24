use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

const CREATIONS: ProductMigrationSpec = ProductMigrationSpec::new(
    1066,
    "1066_workspace_creations",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE workspace_creations(id TEXT PRIMARY KEY,workspace_id TEXT NOT NULL REFERENCES workspaces(id),title TEXT NOT NULL,current_version_id TEXT NOT NULL REFERENCES workspace_creation_versions(id) DEFERRABLE INITIALLY DEFERRED,revision INTEGER NOT NULL CHECK(revision>0),created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);
    CREATE INDEX workspace_creations_workspace ON workspace_creations(workspace_id,updated_at,id);
    CREATE TABLE workspace_creation_versions(id TEXT PRIMARY KEY,creation_id TEXT NOT NULL REFERENCES workspace_creations(id),revision INTEGER NOT NULL CHECK(revision>0),title TEXT NOT NULL,resource_id TEXT NOT NULL REFERENCES resources(id),parent_version_id TEXT REFERENCES workspace_creation_versions(id),author_kind TEXT NOT NULL CHECK(author_kind IN ('user','agent')),run_id TEXT,created_at INTEGER NOT NULL,UNIQUE(creation_id,revision));
    CREATE INDEX workspace_creation_versions_creation ON workspace_creation_versions(creation_id,revision);",
);

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceCreation {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub current_version_id: String,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceCreationVersion {
    pub id: String,
    pub creation_id: String,
    pub revision: i64,
    pub title: String,
    pub resource_id: String,
    pub parent_version_id: Option<String>,
    pub author_kind: String,
    pub run_id: Option<String>,
    pub created_at: i64,
    pub current: bool,
}

fn creation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceCreation> {
    Ok(WorkspaceCreation {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        title: row.get(2)?,
        current_version_id: row.get(3)?,
        revision: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

pub fn validate_creation_title(title: &str) -> Result<(), String> {
    if title.trim().is_empty() || title.chars().count() > 200 {
        return Err("文稿名称不能为空，且不能超过200字".into());
    }
    Ok(())
}

fn active_workspace(conn: &rusqlite::Connection, workspace: &str) -> Result<(), String> {
    let active: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id=?1 AND status='active')",
            [workspace],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !active {
        return Err("工作空间不存在或已归档".into());
    }
    Ok(())
}

impl Database {
    pub fn initialize_creations(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&CREATIONS)
    }

    pub fn workspace_creations(
        &self,
        workspace: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<WorkspaceCreation>, String> {
        let conn = self.conn();
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id=?1)",
                [workspace],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !exists {
            return Err("workspace_not_found".into());
        }
        let mut query = conn.prepare("SELECT id,workspace_id,title,current_version_id,revision,created_at,updated_at FROM workspace_creations WHERE workspace_id=?1 ORDER BY updated_at DESC,id DESC LIMIT ?2 OFFSET ?3").map_err(|error| error.to_string())?;
        let rows = query
            .query_map(params![workspace, limit, offset], creation_row)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub fn workspace_creation(
        &self,
        workspace: &str,
        id: &str,
    ) -> Result<WorkspaceCreation, String> {
        self.conn().query_row("SELECT id,workspace_id,title,current_version_id,revision,created_at,updated_at FROM workspace_creations WHERE id=?1 AND workspace_id=?2", params![id,workspace], creation_row)
            .optional().map_err(|error| error.to_string())?.ok_or_else(|| "creation_not_found".into())
    }

    pub fn create_workspace_creation(
        &self,
        workspace: &str,
        title: &str,
        resource: &str,
    ) -> Result<WorkspaceCreation, String> {
        validate_creation_title(title)?;
        let id = uuid::Uuid::new_v4().to_string();
        let version = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        active_workspace(&tx, workspace)?;
        tx.execute("INSERT INTO workspace_creations(id,workspace_id,title,current_version_id,revision,created_at,updated_at) VALUES(?1,?2,?3,?4,1,?5,?5)", params![id,workspace,title.trim(),version,now]).map_err(|error| error.to_string())?;
        tx.execute("INSERT INTO workspace_creation_versions(id,creation_id,revision,title,resource_id,parent_version_id,author_kind,run_id,created_at) VALUES(?1,?2,1,?3,?4,NULL,'user',NULL,?5)", params![version,id,title.trim(),resource,now]).map_err(|error| error.to_string())?;
        tx.commit().map_err(|error| error.to_string())?;
        drop(conn);
        self.workspace_creation(workspace, &id)
    }

    pub fn append_workspace_creation(
        &self,
        workspace: &str,
        id: &str,
        expected_revision: i64,
        title: &str,
        resource: &str,
    ) -> Result<WorkspaceCreation, String> {
        validate_creation_title(title)?;
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        active_workspace(&tx, workspace)?;
        let parent: Option<String> = tx.query_row("SELECT current_version_id FROM workspace_creations WHERE id=?1 AND workspace_id=?2 AND revision=?3",params![id,workspace,expected_revision], |row| row.get(0)).optional().map_err(|error| error.to_string())?;
        let parent = parent.ok_or("creation_revision_conflict")?;
        let revision = expected_revision.checked_add(1).ok_or("文稿版本达到上限")?;
        let version = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        tx.execute("INSERT INTO workspace_creation_versions(id,creation_id,revision,title,resource_id,parent_version_id,author_kind,run_id,created_at) VALUES(?1,?2,?3,?4,?5,?6,'user',NULL,?7)", params![version,id,revision,title.trim(),resource,parent,now]).map_err(|error| error.to_string())?;
        tx.execute("UPDATE workspace_creations SET title=?2,current_version_id=?3,revision=?4,updated_at=?5 WHERE id=?1", params![id,title.trim(),version,revision,now]).map_err(|error| error.to_string())?;
        tx.commit().map_err(|error| error.to_string())?;
        drop(conn);
        self.workspace_creation(workspace, id)
    }

    pub fn workspace_creation_versions(
        &self,
        workspace: &str,
        id: &str,
    ) -> Result<Vec<WorkspaceCreationVersion>, String> {
        let creation = self.workspace_creation(workspace, id)?;
        let conn = self.conn();
        let mut statement = conn.prepare("SELECT id,creation_id,revision,title,resource_id,parent_version_id,author_kind,run_id,created_at FROM workspace_creation_versions WHERE creation_id=?1 ORDER BY revision DESC").map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([id], |row| {
                let version_id: String = row.get(0)?;
                Ok(WorkspaceCreationVersion {
                    current: version_id == creation.current_version_id,
                    id: version_id,
                    creation_id: row.get(1)?,
                    revision: row.get(2)?,
                    title: row.get(3)?,
                    resource_id: row.get(4)?,
                    parent_version_id: row.get(5)?,
                    author_kind: row.get(6)?,
                    run_id: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::super::Database;

    #[test]
    fn workspace_creation_versions_have_resource_foreign_keys() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let count: i64 = db.conn().query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('workspace_creations','workspace_creation_versions')",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(
            count, 2,
            "workspace creations and their immutable resource versions must be migrated"
        );
        let resource_foreign_keys: i64 = db.conn().query_row("SELECT COUNT(*) FROM pragma_foreign_key_list('workspace_creation_versions') WHERE \"table\"='resources'", [], |row| row.get(0)).unwrap();
        assert_eq!(resource_foreign_keys, 1);
    }

    #[test]
    fn edits_append_versions_without_overwriting_old_resources_and_reject_stale_writes() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        db.conn().execute("INSERT INTO workspaces(id,name,status,created_at,updated_at) VALUES('workspace','Workspace','active',1,1)",[]).unwrap();
        for id in ["resource-a", "resource-b"] {
            db.insert_resource(&crate::shared::resource::Resource {
                id: id.into(),
                source: "user".into(),
                name: "note.md".into(),
                mime_type: "text/markdown".into(),
                size: 10,
                hash: id.into(),
                storage_path: id.into(),
                metadata: serde_json::json!({}),
                created_at: 1,
                updated_at: 1,
            })
            .unwrap();
        }
        let created = db
            .create_workspace_creation("workspace", "初稿", "resource-a")
            .unwrap();
        let edited = db
            .append_workspace_creation("workspace", &created.id, 1, "修订稿", "resource-b")
            .unwrap();
        assert_eq!(edited.revision, 2);
        assert_eq!(
            db.append_workspace_creation("workspace", &created.id, 1, "过期编辑", "resource-a")
                .unwrap_err(),
            "creation_revision_conflict"
        );
        let versions = db
            .workspace_creation_versions("workspace", &created.id)
            .unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(
            versions[0].parent_version_id.as_deref(),
            Some(created.current_version_id.as_str())
        );
        assert_eq!(versions[1].resource_id, "resource-a");
        assert_eq!(versions[1].title, "初稿");
        assert!(versions[0].current);
        assert!(!versions[1].current);
        assert_eq!(versions[0].author_kind, "user");
        assert!(db
            .workspace_creation("another-workspace", &created.id)
            .is_err());
        db.conn()
            .execute(
                "UPDATE workspaces SET status='archived' WHERE id='workspace'",
                [],
            )
            .unwrap();
        assert!(db
            .append_workspace_creation("workspace", &created.id, 2, "归档编辑", "resource-a")
            .is_err());
        assert_eq!(
            db.workspace_creation_versions("workspace", &created.id)
                .unwrap()
                .len(),
            2
        );
    }
}
