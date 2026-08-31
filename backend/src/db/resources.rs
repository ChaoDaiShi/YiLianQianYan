use super::Database;
use crate::shared::resource::Resource;
use rusqlite::{params, OptionalExtension};

impl Database {
    pub fn insert_resource(&self, resource: &Resource) -> Result<(), String> {
        let metadata =
            serde_json::to_string(&resource.metadata).map_err(|error| error.to_string())?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO resources (
                id, source, name, mime_type, size, hash, storage_path,
                metadata_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                resource.id,
                resource.source,
                resource.name,
                resource.mime_type,
                resource.size,
                resource.hash,
                resource.storage_path,
                metadata,
                resource.created_at,
                resource.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_resource(&self, id: &str) -> Result<Option<Resource>, String> {
        let conn = self.conn();
        conn.query_row(
            "SELECT id, source, name, mime_type, size, hash, storage_path,
                    metadata_json, created_at, updated_at
             FROM resources WHERE id=?1",
            [id],
            row_to_resource,
        )
        .optional()
        .map_err(|error| error.to_string())
    }

    pub fn list_resources(&self, limit: usize, offset: usize) -> Result<Vec<Resource>, String> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT id, source, name, mime_type, size, hash, storage_path,
                        metadata_json, created_at, updated_at
                 FROM resources ORDER BY created_at DESC, id DESC LIMIT ?1 OFFSET ?2",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(
                params![limit.min(200) as i64, offset as i64],
                row_to_resource,
            )
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }
}

fn row_to_resource(row: &rusqlite::Row<'_>) -> rusqlite::Result<Resource> {
    let metadata: String = row.get(7)?;
    let metadata = serde_json::from_str(&metadata).unwrap_or_else(|_| serde_json::json!({}));
    Ok(Resource {
        id: row.get(0)?,
        source: row.get(1)?,
        name: row.get(2)?,
        mime_type: row.get(3)?,
        size: row.get(4)?,
        hash: row.get(5)?,
        storage_path: row.get(6)?,
        metadata,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resource_metadata_roundtrips() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let resource = Resource {
            id: "res-db".to_string(),
            source: "upload".to_string(),
            name: "notes.txt".to_string(),
            mime_type: "text/plain".to_string(),
            size: 5,
            hash: "hash".to_string(),
            storage_path: "managed/res-db".to_string(),
            metadata: json!({"safe": true}),
            created_at: 1,
            updated_at: 1,
        };

        db.insert_resource(&resource).unwrap();

        assert_eq!(db.get_resource("res-db").unwrap(), Some(resource.clone()));
        assert_eq!(db.list_resources(10, 0).unwrap(), vec![resource]);
    }
}
