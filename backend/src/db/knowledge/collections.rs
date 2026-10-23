use super::{Database, KnowledgeDocument};
use crate::db::migrations::{MigrationOwner, ProductMigrationSpec};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub(super) const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1057,
    "1057_knowledge_folder_collections",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE knowledge_collections(id TEXT PRIMARY KEY,name TEXT NOT NULL UNIQUE,created_at INTEGER NOT NULL);
     CREATE TABLE knowledge_collection_documents(collection_id TEXT NOT NULL REFERENCES knowledge_collections(id) ON DELETE CASCADE,document_id TEXT NOT NULL REFERENCES knowledge_documents(id) ON DELETE CASCADE,relative_path TEXT NOT NULL,created_at INTEGER NOT NULL,PRIMARY KEY(collection_id,relative_path));
     CREATE INDEX knowledge_membership_document ON knowledge_collection_documents(document_id);
     CREATE TRIGGER knowledge_membership_deleted AFTER DELETE ON knowledge_documents BEGIN DELETE FROM knowledge_collection_documents WHERE document_id=OLD.id; END;",
);

#[derive(Debug, Serialize)]
pub struct KnowledgeCollection {
    pub id: String,
    pub name: String,
    pub document_count: i64,
    pub path_count: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeMembership {
    pub collection_id: String,
    pub collection_name: String,
    pub document_id: String,
    pub relative_path: String,
}

#[derive(Debug, Deserialize)]
pub struct KnowledgeImportLocation {
    pub collection_id: String,
    pub relative_path: String,
}

pub(super) fn attach_on(
    conn: &Connection,
    document: &str,
    location: &KnowledgeImportLocation,
) -> Result<(), String> {
    let path = location.relative_path.replace('\\', "/");
    if path.is_empty()
        || path.len() > 4096
        || path.starts_with('/')
        || path
            .chars()
            .any(|character| character.is_control() || character == ':')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("文件夹相对路径无效".into());
    }
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_collections WHERE id=?1)",
            [&location.collection_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !exists {
        return Err("知识分类不存在".into());
    }
    conn.execute(
        "INSERT INTO knowledge_collection_documents(collection_id,document_id,relative_path,created_at) VALUES(?1,?2,?3,?4)
         ON CONFLICT(collection_id,relative_path) DO UPDATE SET document_id=excluded.document_id,created_at=excluded.created_at",
        params![location.collection_id, document, path, chrono::Utc::now().timestamp_millis()],
    ).map_err(|error| error.to_string())?;
    Ok(())
}

impl Database {
    pub fn knowledge_collections(&self) -> Result<Vec<KnowledgeCollection>, String> {
        let conn = self.conn();
        let mut statement = conn.prepare("SELECT c.id,c.name,COUNT(DISTINCT m.document_id),COUNT(m.relative_path),c.created_at FROM knowledge_collections c LEFT JOIN knowledge_collection_documents m ON m.collection_id=c.id GROUP BY c.id ORDER BY c.name COLLATE NOCASE,c.id").map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok(KnowledgeCollection {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    document_count: row.get(2)?,
                    path_count: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub fn ensure_knowledge_collection(&self, name: &str) -> Result<KnowledgeCollection, String> {
        let name = name.trim();
        if name.is_empty()
            || name.chars().count() > 255
            || name
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\'))
        {
            return Err("文件夹分类名称无效".into());
        }
        let conn = self.conn();
        conn.execute("INSERT INTO knowledge_collections(id,name,created_at) VALUES(?1,?2,?3) ON CONFLICT(name) DO NOTHING", params![uuid::Uuid::new_v4().to_string(),name,chrono::Utc::now().timestamp_millis()]).map_err(|error| error.to_string())?;
        drop(conn);
        self.knowledge_collections()?
            .into_iter()
            .find(|collection| collection.name == name)
            .ok_or_else(|| "分类记录不可用".into())
    }

    pub fn knowledge_memberships(
        &self,
        document: Option<&str>,
    ) -> Result<Vec<KnowledgeMembership>, String> {
        let conn = self.conn();
        let mut statement = conn.prepare("SELECT m.collection_id,c.name,m.document_id,m.relative_path FROM knowledge_collection_documents m JOIN knowledge_collections c ON c.id=m.collection_id WHERE (?1 IS NULL OR m.document_id=?1) ORDER BY c.name,m.relative_path").map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([document], |row| {
                Ok(KnowledgeMembership {
                    collection_id: row.get(0)?,
                    collection_name: row.get(1)?,
                    document_id: row.get(2)?,
                    relative_path: row.get(3)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub fn find_knowledge_document(&self, id: &str) -> Result<Option<KnowledgeDocument>, String> {
        self.conn().query_row("SELECT d.id,d.resource_id,d.name,d.mime_type,d.hash,d.size,d.char_count,(SELECT COUNT(*) FROM knowledge_document_chunks WHERE document_id=d.id),d.created_at FROM knowledge_documents d WHERE d.id=?1",[id],|row|Ok(KnowledgeDocument {id:row.get(0)?,resource_id:row.get(1)?,name:row.get(2)?,mime_type:row.get(3)?,hash:row.get(4)?,size:row.get(5)?,char_count:row.get(6)?,chunk_count:row.get(7)?,created_at:row.get(8)?})).optional().map_err(|error|error.to_string())
    }
}
