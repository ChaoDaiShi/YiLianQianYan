// ============================================================
// Approval — PendingApproval model + in-memory ApprovalStore.
//
// Stores high-risk tool calls that were paused awaiting user
// decision. The original tool call (tool_name + arguments) is
// preserved verbatim so an approved action executes exactly
// the call the user was shown.
// ============================================================

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::tools::trait_def::RiskLevel;

/// Lifecycle of a pending approval.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Cancelled,
    Expired,
}

impl std::fmt::Display for ApprovalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Rejected => "rejected",
            ApprovalStatus::Cancelled => "cancelled",
            ApprovalStatus::Expired => "expired",
        };
        write!(f, "{}", s)
    }
}

/// A paused high-risk tool call awaiting user confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub approval_id: String,
    pub conversation_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub risk_level: RiskLevel,
    pub reason: String,
    pub status: ApprovalStatus,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// The security subject that initiated this approval request.
    pub subject_id: String,
    /// Optional workflow binding (set when the approval pauses a workflow run).
    pub execution_id: Option<String>,
    pub workflow_run_id: Option<String>,
    pub workflow_node_id: Option<String>,
}

impl PendingApproval {
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }
}

/// Errors returned when an approval cannot be processed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalError {
    NotFound,
    AlreadyProcessed,
    Expired,
    Cancelled,
    ConversationMismatch,
}

impl std::fmt::Display for ApprovalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ApprovalError::NotFound => "审批不存在",
            ApprovalError::AlreadyProcessed => "审批已被处理，不能重复操作",
            ApprovalError::Expired => "审批已过期",
            ApprovalError::Cancelled => "审批已取消",
            ApprovalError::ConversationMismatch => "审批不属于当前会话",
        };
        write!(f, "{}", s)
    }
}

/// Default approval lifetime (20 minutes, within the 15–30 min range).
const DEFAULT_TTL_SECS: i64 = 20 * 60;

/// Thread-safe in-memory store of pending approvals.
#[derive(Default)]
pub struct ApprovalStore {
    approvals: RwLock<HashMap<String, PendingApproval>>,
}

impl ApprovalStore {
    pub fn new() -> Self {
        Self {
            approvals: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new pending approval. Returns the created approval.
    pub fn create(
        &self,
        conversation_id: String,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        risk_level: RiskLevel,
        reason: String,
        subject_id: String,
    ) -> PendingApproval {
        let now = Utc::now();
        let approval = PendingApproval {
            approval_id: uuid::Uuid::new_v4().to_string(),
            conversation_id,
            tool_call_id,
            tool_name,
            arguments,
            risk_level,
            reason,
            status: ApprovalStatus::Pending,
            created_at: now,
            expires_at: now + chrono::Duration::seconds(DEFAULT_TTL_SECS),
            subject_id,
            execution_id: None,
            workflow_run_id: None,
            workflow_node_id: None,
        };
        self.approvals
            .write()
            .insert(approval.approval_id.clone(), approval.clone());
        approval
    }

    /// Create a pending approval bound to a specific workflow run + node.
    ///
    /// The workflow binding lets a later resume locate the exact run and node
    /// the approval paused — and reject resumption against a different run.
    pub fn create_workflow(
        &self,
        execution_id: String,
        workflow_run_id: String,
        workflow_node_id: String,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        risk_level: RiskLevel,
        reason: String,
        subject_id: String,
    ) -> PendingApproval {
        let now = Utc::now();
        let approval = PendingApproval {
            approval_id: uuid::Uuid::new_v4().to_string(),
            conversation_id: execution_id.clone(),
            tool_call_id,
            tool_name,
            arguments,
            risk_level,
            reason,
            status: ApprovalStatus::Pending,
            created_at: now,
            expires_at: now + chrono::Duration::seconds(DEFAULT_TTL_SECS),
            subject_id,
            execution_id: Some(execution_id),
            workflow_run_id: Some(workflow_run_id),
            workflow_node_id: Some(workflow_node_id),
        };
        self.approvals
            .write()
            .insert(approval.approval_id.clone(), approval.clone());
        approval
    }

    /// Atomically reuse the active approval for a conversation or create one.
    /// Returns the approval and whether this call created it.
    pub fn create_or_get_pending(
        &self,
        conversation_id: String,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        risk_level: RiskLevel,
        reason: String,
        subject_id: String,
    ) -> (PendingApproval, bool) {
        let now = Utc::now();
        let mut approvals = self.approvals.write();

        if let Some(existing) = approvals
            .values()
            .find(|approval| {
                approval.conversation_id == conversation_id
                    && approval.status == ApprovalStatus::Pending
            })
            .cloned()
        {
            return (existing, false);
        }

        let approval = PendingApproval {
            approval_id: uuid::Uuid::new_v4().to_string(),
            conversation_id,
            tool_call_id,
            tool_name,
            arguments,
            risk_level,
            reason,
            status: ApprovalStatus::Pending,
            created_at: now,
            expires_at: now + chrono::Duration::seconds(DEFAULT_TTL_SECS),
            subject_id,
            execution_id: None,
            workflow_run_id: None,
            workflow_node_id: None,
        };
        approvals.insert(approval.approval_id.clone(), approval.clone());
        (approval, true)
    }

    /// Fetch an approval by id.
    pub fn get(&self, approval_id: &str) -> Option<PendingApproval> {
        self.approvals.read().get(approval_id).cloned()
    }

    /// Fetch the active pending approval for a conversation, if any.
    pub fn pending_for(&self, conversation_id: &str) -> Option<PendingApproval> {
        self.approvals
            .read()
            .values()
            .find(|a| a.conversation_id == conversation_id && a.status == ApprovalStatus::Pending)
            .cloned()
    }

    /// List all approvals (any status), newest last.
    pub fn list(&self) -> Vec<PendingApproval> {
        self.approvals.read().values().cloned().collect()
    }

    /// List only pending approvals.
    pub fn list_pending(&self) -> Vec<PendingApproval> {
        self.list()
            .into_iter()
            .filter(|a| a.status == ApprovalStatus::Pending)
            .collect()
    }

    /// Atomically mark a pending approval as Approved.
    ///
    /// Returns the approval with its original tool-call payload. The
    /// state transition happens inside the write lock so two concurrent
    /// approves cannot both execute the tool.
    pub fn consume_for_approval(
        &self,
        approval_id: &str,
        conversation_id: &str,
    ) -> Result<PendingApproval, ApprovalError> {
        self.transition(approval_id, conversation_id, ApprovalStatus::Approved)
    }

    /// Atomically mark a pending approval as Rejected.
    pub fn consume_for_rejection(
        &self,
        approval_id: &str,
        conversation_id: &str,
    ) -> Result<PendingApproval, ApprovalError> {
        self.transition(approval_id, conversation_id, ApprovalStatus::Rejected)
    }

    /// Atomically mark a pending approval as Cancelled.
    pub fn cancel(
        &self,
        approval_id: &str,
        conversation_id: &str,
    ) -> Result<PendingApproval, ApprovalError> {
        self.transition(approval_id, conversation_id, ApprovalStatus::Cancelled)
    }

    fn transition(
        &self,
        approval_id: &str,
        conversation_id: &str,
        target: ApprovalStatus,
    ) -> Result<PendingApproval, ApprovalError> {
        let now = Utc::now();
        let mut map = self.approvals.write();

        let approval = map.get_mut(approval_id).ok_or(ApprovalError::NotFound)?;

        if approval.conversation_id != conversation_id {
            return Err(ApprovalError::ConversationMismatch);
        }
        if approval.status != ApprovalStatus::Pending {
            return Err(ApprovalError::AlreadyProcessed);
        }
        if approval.is_expired(now) {
            approval.status = ApprovalStatus::Expired;
            return Err(ApprovalError::Expired);
        }

        approval.status = target;
        Ok(approval.clone())
    }

    /// Remove expired approvals (already marked Expired, or Pending past expiry).
    /// Returns how many were removed.
    pub fn purge_expired(&self) -> usize {
        let now = Utc::now();
        let mut map = self.approvals.write();
        let mut removed = 0usize;
        map.retain(|_, a| {
            let gone = a.status == ApprovalStatus::Expired
                || (a.status == ApprovalStatus::Pending && a.is_expired(now));
            if gone {
                removed += 1;
                false
            } else {
                true
            }
        });
        removed
    }

    /// Insert a pre-built approval (test helper for crafting expiry etc.).
    #[cfg(test)]
    pub(crate) fn insert_for_test(&self, approval: PendingApproval) {
        self.approvals
            .write()
            .insert(approval.approval_id.clone(), approval);
    }
}
