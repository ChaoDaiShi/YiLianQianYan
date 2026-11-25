use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WatchSource {
    File {
        path: String,
    },
    Http {
        url: String,
        #[serde(default)]
        send_preview: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionStatus {
    Waiting,
    Checking,
    Deferred,
    Unchanged,
    ActionQueued,
    Proposed,
    Conflict,
    Blocked,
    Unknown,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionAction {
    Defer,
    Stop,
    Act,
    Propose,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AttentionDecision {
    pub action: AttentionAction,
    pub reason: String,
    pub recheck_in_secs: Option<u64>,
    pub proposal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct FactReference {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AttentionProposal {
    pub goal: String,
    pub observation_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AttentionState {
    pub status: AttentionStatus,
    pub reason: String,
    pub decision: Option<AttentionAction>,
    pub checks_used: u32,
    pub retries_used: u32,
    pub last_checked_at: Option<i64>,
    pub observation_id: Option<String>,
    pub last_observed_at: Option<i64>,
    pub proposal: Option<AttentionProposal>,
    #[serde(default)]
    pub facts: Vec<FactReference>,
    /// Kept with the scheduler checkpoint across process exits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pending_check: Option<String>,
    #[serde(default)]
    pub(crate) interval_secs: u64,
    #[serde(default)]
    pub(crate) fingerprint: Option<String>,
    #[serde(default)]
    pub(crate) source_at: Option<i64>,
    #[serde(default)]
    pub(crate) decision_pending: bool,
    #[serde(default)]
    pub(crate) decision_key: Option<String>,
}

impl AttentionState {
    pub(crate) fn waiting() -> Self {
        Self {
            status: AttentionStatus::Waiting,
            reason: "等待首次采集".into(),
            decision: None,
            checks_used: 0,
            retries_used: 0,
            last_checked_at: None,
            observation_id: None,
            last_observed_at: None,
            proposal: None,
            facts: vec![],
            pending_check: None,
            interval_secs: 0,
            fingerprint: None,
            source_at: None,
            decision_pending: false,
            decision_key: None,
        }
    }

    pub(crate) fn stop(&mut self, reason: impl Into<String>) {
        self.status = AttentionStatus::Stopped;
        self.reason = reason.into();
        self.pending_check = None;
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationStatus {
    Captured,
    Conflict,
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceObservation {
    pub id: String,
    pub automation_id: String,
    pub automation_revision: u64,
    pub workspace_id: String,
    pub source_kind: String,
    pub source: String,
    pub authority: String,
    pub source_at: Option<i64>,
    pub observed_at: i64,
    pub fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<FactReference>,
    pub status: ObservationStatus,
    pub previous_observation_id: Option<String>,
}
