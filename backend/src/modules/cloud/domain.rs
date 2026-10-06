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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOperation {
    Upsert,
    Delete,
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
