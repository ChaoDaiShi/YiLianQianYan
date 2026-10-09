use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const SYNC_PROTOCOL_VERSION: u32 = 2;
pub const SYNC_PROTOCOL_HEADER: &str = "X-Yilian-Sync-Protocol";
pub const SYNC_EPOCH_HEADER: &str = "X-Yilian-Sync-Epoch";
pub const CONTRACT_REVISION: &str = "v1.1-r2";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScopeKey {
    pub service_origin: String, // 规范化 API 根 URL，包含路径前缀
    pub account_id: String,     // 登录响应中认证后的 account.id
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScopeLease {
    pub scope_id: String,   // 规范化 ScopeKey 的 SHA-256
    pub generation: String, // 每次激活生成的 UUID；用于拒绝旧请求回写
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EntityKey {
    pub entity_type: String,
    pub entity_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RemoteRecord {
    pub change_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub revision: i64,
    pub operation: super::domain::SyncOperation,
    pub payload: Option<Value>,
    pub sequence: String,
    pub device_id: String,
    pub changed_at: i64,
}

impl RemoteRecord {
    pub fn validate(&self) -> Result<(), SyncError> {
        if self.revision <= 0 || self.revision > 9007199254740991 {
            return Err(SyncError::InvalidRevision(format!(
                "revision out of range: {}",
                self.revision
            )));
        }
        let seq_num = self.sequence.parse::<i64>().map_err(|_| {
            SyncError::InvalidPage(format!("invalid non-numeric sequence: {}", self.sequence))
        })?;
        if seq_num < 0 {
            return Err(SyncError::InvalidPage(format!(
                "negative sequence: {}",
                self.sequence
            )));
        }
        match self.operation {
            super::domain::SyncOperation::Delete => {
                if self.payload.is_some() {
                    return Err(SyncError::PayloadInvalid(
                        "delete operation must have null payload".into(),
                    ));
                }
            }
            super::domain::SyncOperation::Upsert => {
                let Some(ref payload) = self.payload else {
                    return Err(SyncError::PayloadInvalid(
                        "upsert operation must have object payload".into(),
                    ));
                };
                if !payload.is_object() {
                    return Err(SyncError::PayloadInvalid(
                        "upsert payload must be an object".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullPage {
    pub changes: Vec<RemoteRecord>,
    pub next_cursor: String,
    pub has_more: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushStatus {
    Applied,
    Duplicate,
    Conflict,
    Rejected,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PushResult {
    pub change_id: String,
    pub status: PushStatus,
    pub authoritative_revision: Option<i64>,
    pub authoritative: Option<RemoteRecord>,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PushAck {
    pub results: Vec<PushResult>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplyDisposition {
    Applied,
    Duplicate,
    Stale,
    ConflictStored { conflict_id: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingChange {
    pub change_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub base_revision: i64,
    pub revision: i64,
    pub operation: super::domain::SyncOperation,
    pub payload: Option<Value>,
    pub device_id: String,
    pub changed_at: i64,
    pub local_generation: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AckReport {
    pub applied: usize,
    pub duplicate: usize,
    pub conflicts: usize,
    pub rejected: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionDecision {
    Apply,
    Stale,
    Duplicate,
    ContentMismatch,
}

pub fn decide_remote_version(
    stored_revision: Option<i64>,
    stored_digest: Option<&str>,
    incoming_revision: i64,
    incoming_digest: &str,
) -> VersionDecision {
    match stored_revision {
        None => VersionDecision::Apply,
        Some(rev) if incoming_revision < rev => VersionDecision::Stale,
        Some(rev) if incoming_revision > rev => VersionDecision::Apply,
        Some(_) if stored_digest == Some(incoming_digest) => VersionDecision::Duplicate,
        Some(_) => VersionDecision::ContentMismatch,
    }
}

pub fn compute_canonical_digest(
    entity_type: &str,
    entity_id: &str,
    revision: i64,
    operation: super::domain::SyncOperation,
    payload: Option<&Value>,
) -> String {
    use sha2::{Digest, Sha256};
    let normalized_payload = match payload {
        Some(Value::Null) | None => Value::Null,
        Some(val) => canonicalize_json_value(val),
    };
    let canonical_payload_str =
        serde_json::to_string(&normalized_payload).unwrap_or_else(|_| "null".to_string());
    let raw = format!(
        "{}:{}:{}:{}:{}",
        entity_type,
        entity_id,
        revision,
        operation.as_str(),
        canonical_payload_str
    );
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn canonicalize_json_value(val: &Value) -> Value {
    match val {
        Value::Object(map) => {
            let mut sorted = std::collections::BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k.clone(), canonicalize_json_value(v));
            }
            let mut new_map = serde_json::Map::new();
            for (k, v) in sorted {
                new_map.insert(k, v);
            }
            Value::Object(new_map)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(canonicalize_json_value).collect()),
        other => other.clone(),
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyPageReport {
    pub applied: usize,
    pub duplicate: usize,
    pub stale: usize,
    pub conflicts: usize,
    pub download_cursor: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncCapabilities {
    pub sync_protocol: u32,
    pub contract_revision: String,
    pub sync_epoch: String,
    pub entity_schemas: BTreeMap<String, u32>,
    pub max_push_changes: usize,
    pub max_pull_changes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Scope changed")]
    ScopeChanged,
    #[error("Checkpoint changed")]
    CheckpointChanged,
    #[error("Invalid page: {0}")]
    InvalidPage(String),
    #[error("Payload invalid: {0}")]
    PayloadInvalid(String),
    #[error("Unsupported schema: {0}")]
    UnsupportedSchema(String),
    #[error("Revision content mismatch: {0}")]
    RevisionContentMismatch(String),
    #[error("Invalid revision: {0}")]
    InvalidRevision(String),
    #[error("Restore requires new entity: {0}")]
    RestoreRequiresNewEntity(String),
    #[error("Transport error: {0}")]
    Transport(String),
    #[error("Authentication required")]
    AuthenticationRequired,
    #[error("Protocol upgrade required")]
    ProtocolUpgradeRequired,
    #[error("Cursor reset required")]
    CursorResetRequired,
    #[error("Scope error: {0}")]
    Scope(#[from] ScopeError),
    #[error("Unavailable: {0}")]
    Unavailable(String),
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ScopeError {
    #[error("Invalid origin: {0}")]
    InvalidOrigin(String),
    #[error("Invalid account: {0}")]
    InvalidAccount(String),
    #[error("Scope mismatch: {0}")]
    ScopeMismatch(String),
    #[error("Quiesce timeout")]
    QuiesceTimeout,
    #[error("Migration failed: {0}")]
    MigrationFailed(String),
    #[error("Secret store failed: {0}")]
    SecretStoreFailed(String),
    #[error("IO error: {0}")]
    Io(String),
}

pub fn canonical_scope(api_root: &str, account_id: &str) -> Result<(ScopeKey, String), ScopeError> {
    let trimmed_account = account_id.trim();
    if trimmed_account.is_empty() || trimmed_account.contains(char::is_whitespace) {
        return Err(ScopeError::InvalidAccount(
            "invalid or empty account id".into(),
        ));
    }

    let parsed = url::Url::parse(api_root.trim())
        .map_err(|e| ScopeError::InvalidOrigin(format!("failed to parse origin URL: {e}")))?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(ScopeError::InvalidOrigin(
            "scheme must be http or https".into(),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ScopeError::InvalidOrigin(
            "userinfo not allowed in origin".into(),
        ));
    }
    if parsed.query().is_some() {
        return Err(ScopeError::InvalidOrigin(
            "query string not allowed in origin".into(),
        ));
    }
    if parsed.fragment().is_some() {
        return Err(ScopeError::InvalidOrigin(
            "fragment not allowed in origin".into(),
        ));
    }
    let Some(host_str) = parsed.host_str() else {
        return Err(ScopeError::InvalidOrigin("host missing in origin".into()));
    };
    let lower_host = host_str.to_ascii_lowercase();

    let port_part = match (parsed.scheme(), parsed.port()) {
        ("http", Some(80)) | ("https", Some(443)) | (_, None) => String::new(),
        (_, Some(p)) => format!(":{p}"),
    };

    let raw_path = parsed.path();
    let normalized_path = if raw_path.is_empty() || raw_path == "/" {
        String::new()
    } else {
        raw_path.trim_end_matches('/').to_string()
    };

    let canonical_origin = format!(
        "{}://{}{}{}",
        parsed.scheme(),
        lower_host,
        port_part,
        normalized_path
    );

    let key = ScopeKey {
        service_origin: canonical_origin,
        account_id: trimmed_account.to_string(),
    };

    let serialized = serde_json::to_string(&[&key.service_origin, &key.account_id])
        .map_err(|e| ScopeError::InvalidOrigin(e.to_string()))?;
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    let scope_id = format!("{:x}", hasher.finalize());

    Ok((key, scope_id))
}

pub use crate::db::cloud::apply::{
    apply_pull_page, apply_selected_remote_tx, reconcile_skipped_history, ReconciliationReport,
};
pub use crate::db::cloud::scoped_db::ScopedDb;

pub fn validate_sync_payload(
    entity_type: &str,
    operation: super::domain::SyncOperation,
    payload: Option<&Value>,
) -> Result<(), SyncError> {
    match operation {
        super::domain::SyncOperation::Delete => {
            if payload.is_some() {
                return Err(SyncError::PayloadInvalid(
                    "delete operation cannot contain payload".into(),
                ));
            }
        }
        super::domain::SyncOperation::Upsert => {
            let Some(payload) = payload else {
                return Err(SyncError::PayloadInvalid(
                    "upsert operation must contain payload".into(),
                ));
            };
            if !payload.is_object() {
                return Err(SyncError::PayloadInvalid(
                    "upsert payload must be an object".into(),
                ));
            }
        }
    }
    let known_entities = [
        "conversation",
        "message",
        "memory",
        "agent_profile",
        "workflow_metadata",
        "artifact_metadata",
        "task_graph",
        "canvas_view",
        "settings",
        "capability_usage",
    ];
    if !known_entities.contains(&entity_type) {
        return Err(SyncError::UnsupportedSchema(format!(
            "unknown entity type: {entity_type}"
        )));
    }
    Ok(())
}
