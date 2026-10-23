use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
mod collections;
mod context;
pub use collections::{KnowledgeCollection, KnowledgeImportLocation, KnowledgeMembership};
pub use context::KnowledgeContext;

const SCHEMA:ProductMigrationSpec=ProductMigrationSpec::new(1025,"1025_knowledge_documents",MigrationOwner::V1TaskWorld,
"CREATE TABLE knowledge_documents(id TEXT PRIMARY KEY,resource_id TEXT NOT NULL,name TEXT NOT NULL,mime_type TEXT NOT NULL,hash TEXT NOT NULL UNIQUE,size INTEGER NOT NULL,char_count INTEGER NOT NULL,text_content TEXT NOT NULL,created_at INTEGER NOT NULL);
 CREATE TABLE knowledge_document_chunks(document_id TEXT NOT NULL REFERENCES knowledge_documents(id) ON DELETE CASCADE,memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,chunk_index INTEGER NOT NULL,PRIMARY KEY(document_id,chunk_index));
 CREATE TRIGGER knowledge_memory_deleted AFTER DELETE ON memories BEGIN DELETE FROM knowledge_document_chunks WHERE memory_id=OLD.id; END;");
#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeDocument {
    pub id: String,
    pub resource_id: String,
    pub name: String,
    pub mime_type: String,
    pub hash: String,
    pub size: i64,
    pub char_count: i64,
    pub chunk_count: i64,
    pub created_at: i64,
}
#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeChunk {
    pub memory_id: String,
    pub document_id: String,
    pub document_name: String,
    pub chunk_index: i64,
    pub content: String,
}
/// A canonical document excerpt. Its zero-based character offset traces the
/// content into text_content without claiming the identity of a memory chunk.
#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeSearchHit {
    pub document_id: String,
    pub document_name: String,
    pub content: String,
    pub start_char: usize,
}
pub fn split_document(text: &str) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut part = String::new();
    let mut count = 0;
    for character in text.chars() {
        part.push(character);
        count += 1;
        if count >= 1200 {
            result.push(std::mem::take(&mut part));
            count = 0;
        }
    }
    if !part.is_empty() {
        result.push(part);
    }
    result
}
impl Database {
    pub fn initialize_knowledge_schema(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&SCHEMA)?;
        self.apply_product_migration(&collections::SCHEMA)
    }
    pub fn knowledge_documents(&self) -> Result<Vec<KnowledgeDocument>, String> {
        self.knowledge_documents_for_collection(None)
    }
    pub fn knowledge_documents_for_collection(
        &self,
        collection: Option<&str>,
    ) -> Result<Vec<KnowledgeDocument>, String> {
        let conn = self.conn();
        let mut statement=conn.prepare("SELECT d.id,d.resource_id,d.name,d.mime_type,d.hash,d.size,d.char_count,COUNT(m.id),d.created_at FROM knowledge_documents d LEFT JOIN knowledge_document_chunks c ON c.document_id=d.id LEFT JOIN memories m ON m.id=c.memory_id WHERE (?1 IS NULL OR EXISTS(SELECT 1 FROM knowledge_collection_documents membership WHERE membership.document_id=d.id AND membership.collection_id=?1)) GROUP BY d.id ORDER BY d.created_at DESC,d.id").map_err(|e|e.to_string())?;
        let result = statement
            .query_map([collection], |r| {
                Ok(KnowledgeDocument {
                    id: r.get(0)?,
                    resource_id: r.get(1)?,
                    name: r.get(2)?,
                    mime_type: r.get(3)?,
                    hash: r.get(4)?,
                    size: r.get(5)?,
                    char_count: r.get(6)?,
                    chunk_count: r.get(7)?,
                    created_at: r.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string());
        result
    }
    pub fn import_knowledge(
        &self,
        resource_id: &str,
        name: &str,
        mime: &str,
        hash: &str,
        size: i64,
        text: &str,
    ) -> Result<(KnowledgeDocument, bool), String> {
        self.import_knowledge_in_collection(resource_id, name, mime, hash, size, text, None)
    }
    pub fn import_knowledge_in_collection(
        &self,
        resource_id: &str,
        name: &str,
        mime: &str,
        hash: &str,
        size: i64,
        text: &str,
        location: Option<&KnowledgeImportLocation>,
    ) -> Result<(KnowledgeDocument, bool), String> {
        if text.trim().is_empty() {
            return Err("文档没有可导入的文本".into());
        }
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM knowledge_documents WHERE hash=?1",
                [hash],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let created = existing.is_none();
        let id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if created {
            let now = chrono::Utc::now().timestamp_millis();
            tx.execute("INSERT INTO knowledge_documents(id,resource_id,name,mime_type,hash,size,char_count,text_content,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,resource_id,name,mime,hash,size,text.chars().count() as i64,text,now]).map_err(|e|e.to_string())?;
            for (index, content) in split_document(text).into_iter().enumerate() {
                let memory = uuid::Uuid::new_v4().to_string();
                let metadata=serde_json::json!({"knowledge_document_id":id,"resource_id":resource_id,"document_name":name,"chunk_index":index}).to_string();
                tx.execute("INSERT INTO memories(id,content,category,source,metadata,created_at,updated_at) VALUES(?1,?2,'knowledge','document',?3,?4,?4)",params![memory,content,metadata,now]).map_err(|e|e.to_string())?;
                tx.execute("INSERT INTO knowledge_document_chunks(document_id,memory_id,chunk_index) VALUES(?1,?2,?3)",params![id,memory,index as i64]).map_err(|e|e.to_string())?;
            }
        }
        if let Some(location) = location {
            collections::attach_on(&tx, &id, location)?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);
        let document = self
            .find_knowledge_document(&id)?
            .ok_or("知识文档记录不可用")?;
        Ok((document, created))
    }
    pub fn knowledge_text(&self, id: &str) -> Result<Option<String>, String> {
        self.conn()
            .query_row(
                "SELECT text_content FROM knowledge_documents WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }
    pub fn knowledge_chunk_document(&self, id: &str) -> Result<Option<String>, String> {
        self.conn()
            .query_row(
                "SELECT document_id FROM knowledge_document_chunks WHERE memory_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }
    pub fn knowledge_chunks(
        &self,
        document: Option<&str>,
        query: Option<&str>,
    ) -> Result<Vec<KnowledgeChunk>, String> {
        let conn = self.conn();
        let query = query.filter(|query| !query.trim().is_empty());
        let mut statement=conn.prepare("SELECT m.id,d.id,d.name,c.chunk_index,m.content FROM knowledge_document_chunks c JOIN memories m ON m.id=c.memory_id JOIN knowledge_documents d ON d.id=c.document_id WHERE (?1 IS NULL OR d.id=?1) AND (?2 IS NULL OR instr(lower(m.content),lower(?2))>0 OR instr(lower(d.name),lower(?2))>0) ORDER BY d.created_at DESC,d.id,c.chunk_index LIMIT 2000").map_err(|e|e.to_string())?;
        let result = statement
            .query_map(params![document, query], |r| {
                Ok(KnowledgeChunk {
                    memory_id: r.get(0)?,
                    document_id: r.get(1)?,
                    document_name: r.get(2)?,
                    chunk_index: r.get(3)?,
                    content: r.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string());
        result
    }
    pub fn search_knowledge(&self, query: &str) -> Result<Vec<KnowledgeSearchHit>, String> {
        self.search_knowledge_in_collection(query, None)
    }
    pub fn search_knowledge_in_collection(
        &self,
        query: &str,
        collection: Option<&str>,
    ) -> Result<Vec<KnowledgeSearchHit>, String> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn();
        let mut statement=conn.prepare("SELECT id,name,text_content,instr(lower(text_content),lower(?1)) FROM knowledge_documents WHERE (instr(lower(text_content),lower(?1))>0 OR instr(lower(name),lower(?1))>0) AND (?2 IS NULL OR EXISTS(SELECT 1 FROM knowledge_collection_documents m WHERE m.document_id=knowledge_documents.id AND m.collection_id=?2)) ORDER BY created_at DESC,id LIMIT 200").map_err(|e|e.to_string())?;
        let rows = statement
            .query_map(params![query, collection], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, usize>(3)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        let mut hits = Vec::new();
        for row in rows {
            let (document_id, document_name, text, position) =
                row.map_err(|error| error.to_string())?;
            // SQLite instr is one-based and counts Unicode characters. A name
            // match with no body match uses the true beginning of the document.
            let start_char = position.saturating_sub(1).saturating_sub(120);
            let content = text
                .chars()
                .skip(start_char)
                .take(500 + query.chars().count())
                .collect();
            hits.push(KnowledgeSearchHit {
                document_id,
                document_name,
                content,
                start_char,
            });
        }
        Ok(hits)
    }
    pub fn delete_knowledge_document(&self, id: &str) -> Result<bool, String> {
        let identity = self
            .get_or_create_cloud_identity()
            .map_err(|e| e.to_string())?;
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let ids = {
            let mut stmt = tx
                .prepare("SELECT memory_id FROM knowledge_document_chunks WHERE document_id=?1")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            rows
        };
        for memory in ids {
            super::connectors::remove_connector_entity_on(&tx, "knowledge", &memory)
                .map_err(|e| e.to_string())?;
            super::connectors::remove_connector_entity_on(&tx, "memory", &memory)
                .map_err(|e| e.to_string())?;
            crate::db::cloud::capture::record_local_deletion_tx(
                &tx,
                "memory",
                &memory,
                &identity.device_id,
            )
            .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM memories WHERE id=?1", [memory])
                .map_err(|e| e.to_string())?;
        }
        let removed = tx
            .execute("DELETE FROM knowledge_documents WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(removed > 0)
    }
}

#[cfg(test)]
mod collection_tests;
#[cfg(test)]
mod tests;
