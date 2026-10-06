use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityStatus {
    Guest,
    Authenticated,
    OfflineAuthenticated,
    Closing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentitySnapshot {
    pub local_identity_id: String,
    pub device_id: String,
    pub account_id: Option<String>,
    pub email: Option<String>,
    pub status: IdentityStatus,
    pub local_core_available: bool,
    pub cloud_enabled: bool,
}

impl IdentitySnapshot {
    pub fn guest(local_identity_id: impl Into<String>, device_id: impl Into<String>) -> Self {
        Self {
            local_identity_id: local_identity_id.into(),
            device_id: device_id.into(),
            account_id: None,
            email: None,
            status: IdentityStatus::Guest,
            local_core_available: true,
            cloud_enabled: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AuthenticatedSessionInput {
    pub account_id: String,
    pub email: String,
    pub session_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOperation {
    Upsert,
    Delete,
}

impl SyncOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Upsert => "upsert",
            Self::Delete => "delete",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "upsert" => Some(Self::Upsert),
            "delete" => Some(Self::Delete),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewSyncChange {
    pub entity_type: String,
    pub entity_id: String,
    pub base_revision: i64,
    pub revision: i64,
    pub operation: SyncOperation,
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncOutboxItem {
    pub change_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub base_revision: i64,
    pub revision: i64,
    pub operation: SyncOperation,
    pub payload: Option<serde_json::Value>,
    pub device_id: String,
    pub state: String,
    pub attempts: i64,
    pub next_retry_at: Option<i64>,
    pub last_error_code: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudSyncStatus {
    pub pending_count: i64,
    pub conflict_count: i64,
    pub error_count: i64,
    pub cursor: Option<String>,
    pub last_success_at: Option<i64>,
    pub last_error_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionDecision {
    Apply { next_revision: i64 },
    Conflict { authoritative_revision: i64 },
    TombstoneWins { authoritative_revision: i64 },
}

pub fn apply_revision(
    base_revision: i64,
    authoritative_revision: i64,
    operation: SyncOperation,
    authoritative_deleted: bool,
) -> RevisionDecision {
    if authoritative_deleted && operation == SyncOperation::Upsert {
        return RevisionDecision::TombstoneWins {
            authoritative_revision,
        };
    }
    if base_revision != authoritative_revision {
        return RevisionDecision::Conflict {
            authoritative_revision,
        };
    }
    RevisionDecision::Apply {
        next_revision: authoritative_revision + 1,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountDeletionState {
    Active,
    Requested,
    Deleting,
    Failed,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    Request,
    SessionsRevoked,
    Fail,
    Retry,
    Complete,
}

impl AccountDeletionState {
    pub fn transition(self, event: LifecycleEvent) -> Result<Self, &'static str> {
        match (self, event) {
            (Self::Active, LifecycleEvent::Request) => Ok(Self::Requested),
            (Self::Requested, LifecycleEvent::SessionsRevoked) => Ok(Self::Deleting),
            (Self::Deleting, LifecycleEvent::Fail) => Ok(Self::Failed),
            (Self::Failed, LifecycleEvent::Retry) => Ok(Self::Deleting),
            (Self::Deleting, LifecycleEvent::Complete) => Ok(Self::Deleted),
            _ => Err("invalid account deletion transition"),
        }
    }
}
