use crate::db::Database;
use crate::shared::event::{EventHub, YiEvent};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

pub const DEFAULT_MAX_RESOURCE_BYTES: usize = 25 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub source: String,
    pub name: String,
    pub mime_type: String,
    pub size: i64,
    pub hash: String,
    pub storage_path: String,
    #[serde(default)]
    pub metadata: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Error)]
pub enum ResourceError {
    #[error("resource is empty")]
    Empty,
    #[error("resource exceeds the {max_bytes} byte limit")]
    TooLarge { max_bytes: usize },
    #[error("invalid resource name: {0}")]
    InvalidName(String),
    #[error("invalid resource metadata")]
    InvalidMetadata,
    #[error("resource storage failed: {0}")]
    Storage(String),
    #[error("resource persistence failed: {0}")]
    Persistence(String),
}

#[derive(Clone)]
pub struct ResourceService {
    db: Database,
    storage_root: PathBuf,
    event_hub: EventHub,
    max_bytes: usize,
    ingest_lock: std::sync::Arc<parking_lot::Mutex<()>>,
}

impl ResourceService {
    pub fn new(db: Database, storage_root: PathBuf, event_hub: EventHub) -> Self {
        Self {
            db,
            storage_root,
            event_hub,
            max_bytes: DEFAULT_MAX_RESOURCE_BYTES,
            ingest_lock: std::sync::Arc::new(parking_lot::Mutex::new(())),
        }
    }

    pub fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = max_bytes.max(1);
        self
    }

    pub fn ingest(
        &self,
        name: &str,
        mime_type: &str,
        bytes: &[u8],
        metadata: Value,
    ) -> Result<Resource, ResourceError> {
        let _guard = self.ingest_lock.lock();
        if bytes.is_empty() {
            return Err(ResourceError::Empty);
        }
        if bytes.len() > self.max_bytes {
            return Err(ResourceError::TooLarge {
                max_bytes: self.max_bytes,
            });
        }
        if !metadata.is_object() {
            return Err(ResourceError::InvalidMetadata);
        }
        let name = normalize_name(name)?;
        let mime_type = normalize_mime_type(mime_type);
        let id = format!("res-{}", Uuid::new_v4());
        let hash = format!("{:x}", Sha256::digest(bytes));
        let now = chrono::Utc::now().timestamp_millis();

        std::fs::create_dir_all(&self.storage_root)
            .map_err(|error| ResourceError::Storage(error.to_string()))?;
        let temporary = self.storage_root.join(format!(".{id}.tmp"));
        let final_path = self.storage_root.join(format!("sha256-{hash}"));
        let write_result = (|| -> Result<(), std::io::Error> {
            if final_path.exists() {
                return verify_blob(&final_path, bytes.len(), &hash);
            }
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            if let Err(error) = std::fs::rename(&temporary, &final_path) {
                if !final_path.exists() {
                    return Err(error);
                }
                verify_blob(&final_path, bytes.len(), &hash)?;
                std::fs::remove_file(&temporary)?;
            }
            Ok(())
        })();
        if let Err(error) = write_result {
            let _ = std::fs::remove_file(&temporary);
            return Err(ResourceError::Storage(error.to_string()));
        }

        let resource = Resource {
            id: id.clone(),
            source: "upload".to_string(),
            name,
            mime_type,
            size: bytes.len() as i64,
            hash,
            storage_path: final_path.to_string_lossy().to_string(),
            metadata,
            created_at: now,
            updated_at: now,
        };
        if let Err(error) = self.db.insert_resource(&resource) {
            // A content-addressed blob can already belong to another upload.
            // Leave any unreferenced blob recoverable rather than delete it.
            return Err(ResourceError::Persistence(error));
        }

        let _ = self.event_hub.publish(YiEvent::new(
            "resource.created",
            "resource-core",
            json!({
                "id": resource.id,
                "name": resource.name,
                "mime_type": resource.mime_type,
                "size": resource.size,
                "hash": resource.hash,
            }),
        ));
        Ok(resource)
    }

    pub fn get(&self, id: &str) -> Result<Option<Resource>, ResourceError> {
        self.db.get_resource(id).map_err(ResourceError::Persistence)
    }

    pub fn list(&self, limit: usize, offset: usize) -> Result<Vec<Resource>, ResourceError> {
        self.db
            .list_resources(limit, offset)
            .map_err(ResourceError::Persistence)
    }

    pub fn read_bytes(&self, id: &str) -> Result<Vec<u8>, ResourceError> {
        let resource = self
            .get(id)?
            .ok_or_else(|| ResourceError::Storage("resource is missing".into()))?;
        let root = self
            .storage_root
            .canonicalize()
            .map_err(|_| ResourceError::Storage("managed storage is unavailable".into()))?;
        let target = std::path::Path::new(&resource.storage_path)
            .canonicalize()
            .map_err(|_| ResourceError::Storage("resource file is unavailable".into()))?;
        if target == root
            || !target.starts_with(&root)
            || resource.size <= 0
            || resource.size as u64 > self.max_bytes as u64
        {
            return Err(ResourceError::Storage(
                "resource containment or size check failed".into(),
            ));
        }
        let file = std::fs::File::open(&target)
            .map_err(|_| ResourceError::Storage("resource file is unavailable".into()))?;
        let metadata = file
            .metadata()
            .map_err(|_| ResourceError::Storage("resource metadata unavailable".into()))?;
        if !metadata.is_file() || metadata.len() != resource.size as u64 {
            return Err(ResourceError::Storage("resource size mismatch".into()));
        }
        let mut bytes = Vec::new();
        file.take(self.max_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ResourceError::Storage("resource read failed".into()))?;
        if bytes.len() != resource.size as usize
            || format!("{:x}", Sha256::digest(&bytes)) != resource.hash
        {
            return Err(ResourceError::Storage(
                "resource integrity check failed".into(),
            ));
        }
        Ok(bytes)
    }
}

fn verify_blob(
    path: &std::path::Path,
    expected_size: usize,
    expected_hash: &str,
) -> std::io::Result<()> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() != expected_size as u64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "stored resource size mismatch",
        ));
    }
    let mut bytes = Vec::new();
    file.take(expected_size as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() != expected_size || format!("{:x}", Sha256::digest(&bytes)) != expected_hash {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "stored resource hash mismatch",
        ));
    }
    Ok(())
}

fn normalize_name(value: &str) -> Result<String, ResourceError> {
    let name = value.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.chars().count() > 255
        || name.chars().any(char::is_control)
    {
        return Err(ResourceError::InvalidName(
            "name must be a safe basename".to_string(),
        ));
    }
    Ok(name.to_string())
}

fn normalize_mime_type(value: &str) -> String {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || value.chars().any(|character| character.is_control())
    {
        "application/octet-stream".to_string()
    } else {
        value.to_ascii_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::shared::event::EventHub;
    use serde_json::json;

    fn temp_paths(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("yilian-resource-{label}-{}", uuid::Uuid::new_v4()));
        let db = root.join("test.db");
        (root, db)
    }

    #[tokio::test]
    async fn ingest_writes_managed_bytes_persists_metadata_and_emits_event() {
        let (root, db_path) = temp_paths("ingest");
        let db = Database::new(&db_path).unwrap();
        let hub = EventHub::new(4);
        let mut events = hub.subscribe();
        let service = ResourceService::new(db.clone(), root.join("files"), hub);

        let resource = service
            .ingest(
                "C:\\Users\\person\\notes.txt",
                "text/plain",
                b"hello",
                json!({"origin": "picker"}),
            )
            .unwrap();

        assert_eq!(resource.name, "notes.txt");
        assert_eq!(
            resource.hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert!(!resource.storage_path.contains("person"));
        assert!(!resource.storage_path.contains("notes.txt"));
        assert_eq!(
            std::path::Path::new(&resource.storage_path).parent(),
            Some(root.join("files").as_path()),
        );
        assert_eq!(std::fs::read(&resource.storage_path).unwrap(), b"hello");
        assert_eq!(
            db.get_resource(&resource.id).unwrap(),
            Some(resource.clone())
        );
        let event = events.recv().await.unwrap();
        assert_eq!(event.event_type, "resource.created");
        assert_eq!(event.payload["id"], resource.id);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn oversized_ingest_fails_before_writing_or_persisting() {
        let (root, db_path) = temp_paths("limit");
        let db = Database::new(&db_path).unwrap();
        let service = ResourceService::new(db.clone(), root.join("files"), EventHub::new(1))
            .with_max_bytes(4);

        let error = service
            .ingest("large.bin", "application/octet-stream", b"12345", json!({}))
            .unwrap_err();

        assert!(matches!(error, ResourceError::TooLarge { .. }));
        assert!(db.list_resources(10, 0).unwrap().is_empty());
        assert!(!root.join("files").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn identical_resource_bytes_share_storage_without_merging_identities() {
        let (root, db_path) = temp_paths("dedup");
        let db = Database::new(&db_path).unwrap();
        let service = ResourceService::new(db.clone(), root.join("files"), EventHub::new(4));
        let first = service
            .ingest("first.txt", "text/plain", b"shared bytes", json!({}))
            .unwrap();
        let second = service
            .ingest("second.txt", "text/plain", b"shared bytes", json!({}))
            .unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(first.storage_path, second.storage_path);
        assert_eq!(db.list_resources(10, 0).unwrap().len(), 2);
        assert_eq!(std::fs::read_dir(root.join("files")).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn controlled_read_requires_managed_containment_and_matching_bytes() {
        let (root, db_path) = temp_paths("controlled-read");
        let db = Database::new(&db_path).unwrap();
        let service = ResourceService::new(db.clone(), root.join("files"), EventHub::new(2));
        let resource = service
            .ingest("safe.txt", "text/plain", b"safe", json!({}))
            .unwrap();
        assert_eq!(service.read_bytes(&resource.id).unwrap(), b"safe");
        let outside = root.join("outside.txt");
        std::fs::write(&outside, b"safe").unwrap();
        let mut escaped = resource.clone();
        escaped.id = "outside-record".into();
        escaped.storage_path = outside.to_string_lossy().into();
        db.insert_resource(&escaped).unwrap();
        assert!(service.read_bytes(&escaped.id).is_err());
        std::fs::write(&resource.storage_path, b"edit").unwrap();
        assert!(service.read_bytes(&resource.id).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn resource_contract_ignores_unknown_additive_fields() {
        let value = json!({
            "id": "res-1", "source": "upload", "name": "a.txt",
            "mime_type": "text/plain", "size": 1, "hash": "h",
            "storage_path": "managed/res-1", "metadata": {},
            "created_at": 1, "updated_at": 1, "future": true
        });
        let resource: Resource = serde_json::from_value(value).unwrap();
        assert_eq!(resource.id, "res-1");
    }
}
