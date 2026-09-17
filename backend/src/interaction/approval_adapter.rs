//! Fail-closed approval resolution for the v1 interaction boundary.

use std::sync::Arc;

use thiserror::Error;

use crate::safety::{ApprovalError, ApprovalStore, PendingApproval};
use crate::shared::command::{CommandError, CommandRouter};
use crate::shared::interaction::{InteractionTarget, TargetResolution};

/// The only decisions accepted by the v1 approval adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalVoiceDecision {
    Approve,
    Reject,
}

/// Compatibility alias for callers that use the shorter domain name.
pub type ApprovalDecision = ApprovalVoiceDecision;

impl ApprovalVoiceDecision {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "approve" | "approved" | "yes" | "同意" | "批准" | "确认" | "允许" => {
                Some(Self::Approve)
            }
            "reject" | "rejected" | "deny" | "no" | "拒绝" | "不同意" | "驳回" => {
                Some(Self::Reject)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum ApprovalVoiceAdapterError {
    #[error("approval resolution did not identify exactly one pending approval")]
    Unresolved,
    #[error("approval decision is invalid: {0}")]
    InvalidDecision(String),
    #[error("approval store rejected the decision: {0}")]
    Store(ApprovalError),
}

impl From<ApprovalError> for ApprovalVoiceAdapterError {
    fn from(error: ApprovalError) -> Self {
        Self::Store(error)
    }
}

/// Resolves a bare approval only when one and only one active item exists in
/// the current conversation. Process-wide approval enumeration is never a
/// valid context for a voice/text confirmation.
#[derive(Clone)]
pub struct ApprovalVoiceAdapter {
    store: Arc<ApprovalStore>,
}

impl ApprovalVoiceAdapter {
    pub fn new(store: Arc<ApprovalStore>) -> Self {
        Self { store }
    }

    pub fn store(&self) -> Arc<ApprovalStore> {
        self.store.clone()
    }

    /// Legacy no-context entry point. A bare approve/reject without a current
    /// conversation is fail closed rather than searching every conversation.
    pub fn resolve_unique(&self, decision: ApprovalVoiceDecision) -> TargetResolution {
        let _ = decision;
        TargetResolution::Missing {
            reason: "approval conversation context is missing".to_string(),
        }
    }

    /// Return a side-effect-free target resolution scoped to one conversation.
    /// The actual command path uses the atomic consume methods below instead
    /// of resolving and consuming this snapshot separately.
    pub fn resolve_unique_for_context(
        &self,
        decision: ApprovalVoiceDecision,
        conversation_id: &str,
    ) -> TargetResolution {
        let _ = decision;
        let mut pending = self.store.list_pending_for_conversation(conversation_id);
        pending.sort_by(|left, right| left.approval_id.cmp(&right.approval_id));
        match pending.as_slice() {
            [approval] => TargetResolution::Resolved {
                target: InteractionTarget::Approval {
                    approval_id: approval.approval_id.clone(),
                },
            },
            [] => TargetResolution::Missing {
                reason: "no pending approval exists".to_string(),
            },
            _ => TargetResolution::Ambiguous {
                candidates: pending
                    .into_iter()
                    .take(16)
                    .map(|approval| InteractionTarget::Approval {
                        approval_id: approval.approval_id,
                    })
                    .collect(),
            },
        }
    }

    /// Register the single stable approval command used by Voice/Text. Both
    /// approve and reject use this command with a `resolution` parameter.
    /// Selection and consumption are performed atomically inside the store.
    pub fn register(&self, router: &CommandRouter) -> Result<(), String> {
        let adapter = self.clone();
        router.register("task.approval.resolve", move |request| {
            let raw_resolution = request
                .payload
                .get("resolution")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| CommandError::new("invalid_payload", "resolution is required"))?;
            if raw_resolution.chars().count() > 32 {
                return Err(CommandError::new(
                    "invalid_payload",
                    "resolution must be at most 32 characters",
                ));
            }
            let decision = ApprovalVoiceDecision::parse(raw_resolution).ok_or_else(|| {
                CommandError::new("invalid_payload", "resolution must be approve or reject")
            })?;
            let conversation_id = request
                .payload
                .get("conversation_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    CommandError::new("invalid_payload", "conversation_id is required")
                })?;
            if conversation_id.trim().is_empty() || conversation_id.chars().count() > 128 {
                return Err(CommandError::new(
                    "invalid_payload",
                    "conversation_id must be 1-128 characters",
                ));
            }
            let consumed = adapter
                .resolve_and_consume(decision, conversation_id)
                .map_err(|error| match error {
                    ApprovalVoiceAdapterError::Store(ApprovalError::NotFound) => {
                        CommandError::new("approval_not_found", error.to_string())
                    }
                    ApprovalVoiceAdapterError::Store(ApprovalError::Ambiguous) => {
                        CommandError::new("approval_ambiguous", error.to_string())
                    }
                    _ => CommandError::new("approval_resolve_failed", error.to_string()),
                })?;
            Ok(serde_json::json!({
                "approval_id": consumed.approval_id,
                "status": consumed.status.to_string(),
            }))
        })
    }

    /// Consume a previously resolved approval.  This method rechecks the
    /// target and conversation binding, so a stale or ambiguous resolution
    /// can never be converted into a store mutation.
    pub fn consume(
        &self,
        resolution: &TargetResolution,
        decision: ApprovalVoiceDecision,
        conversation_id: &str,
    ) -> Result<PendingApproval, ApprovalVoiceAdapterError> {
        let TargetResolution::Resolved {
            target: InteractionTarget::Approval { approval_id },
        } = resolution
        else {
            return Err(ApprovalVoiceAdapterError::Unresolved);
        };
        match decision {
            ApprovalVoiceDecision::Approve => self
                .store
                .consume_for_approval(approval_id, conversation_id)
                .map_err(Into::into),
            ApprovalVoiceDecision::Reject => self
                .store
                .consume_for_rejection(approval_id, conversation_id)
                .map_err(Into::into),
        }
    }

    /// Atomically resolve and consume the only active approval in the current
    /// conversation. This deliberately does not call `resolve_unique_*` first
    /// because that would reintroduce a list/consume TOCTOU window.
    pub fn resolve_and_consume(
        &self,
        decision: ApprovalVoiceDecision,
        conversation_id: &str,
    ) -> Result<PendingApproval, ApprovalVoiceAdapterError> {
        match decision {
            ApprovalVoiceDecision::Approve => self
                .store
                .consume_unique_pending_for_approval(conversation_id)
                .map_err(Into::into),
            ApprovalVoiceDecision::Reject => self
                .store
                .consume_unique_pending_for_rejection(conversation_id)
                .map_err(Into::into),
        }
    }
}
