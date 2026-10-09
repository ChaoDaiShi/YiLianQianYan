//! Bounded v1 interaction routing for Task and Conversation surfaces.
//!
//! This module is deliberately an adapter boundary.  It classifies a small
//! deterministic fast path and returns a shared-contract intent plus explicit
//! target resolution; it does not own a task runtime, conversation store, or
//! native application capability.

mod approval_adapter;
mod conversation_adapter;
mod graph_proposal;
mod narration;
mod router;
mod voice_dispatch;

pub use approval_adapter::{
    ApprovalDecision, ApprovalVoiceAdapter, ApprovalVoiceAdapterError, ApprovalVoiceDecision,
};
pub use conversation_adapter::{ConversationVoiceAdapter, ConversationVoiceAdapterError};
pub use graph_proposal::{GraphProposalError, GraphProposalService};
pub use narration::{NarrationRequest, TaskNarrator};
pub use router::InteractionRouter;
pub use voice_dispatch::InteractionVoiceDispatch;

use crate::shared::interaction::{ConversationalAnchor, InteractionTarget};

/// A bounded candidate used when resolving a voice/text reference to a task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskCandidate {
    pub graph_id: String,
    pub title: String,
}

/// A bounded candidate used when resolving a voice/text reference to a
/// persisted conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationCandidate {
    pub conversation_id: String,
    pub title: String,
}

/// A pending approval candidate.  Approval details stay outside the router;
/// only the stable identity needed for unique selection crosses this boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalCandidate {
    pub approval_id: String,
    pub title: String,
    /// Conversation binding required before a bare approval can be routed.
    /// `None` means the owning context did not provide enough information and
    /// therefore cannot be used for an executable decision.
    pub conversation_id: Option<String>,
    /// Actual ApprovalStore conversation identity passed to the atomic
    /// resolver. It may differ from the Voice-session binding used for a
    /// voice request initiated outside a persisted conversation.
    pub store_conversation_id: Option<String>,
}

/// Read-only context supplied by the owning domains to the fast-path router.
/// The vectors are intentionally bounded by [`InteractionRouter`] before any
/// candidate is returned to a caller.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InteractionContext {
    pub tasks: Vec<TaskCandidate>,
    pub conversations: Vec<ConversationCandidate>,
    pub approvals: Vec<ApprovalCandidate>,
    pub active_task: Option<String>,
    pub conversational_anchor: Option<ConversationalAnchor>,
}

impl InteractionContext {
    pub fn with_task(mut self, graph_id: impl Into<String>, title: impl Into<String>) -> Self {
        self.tasks.push(TaskCandidate {
            graph_id: graph_id.into(),
            title: title.into(),
        });
        self
    }

    pub fn with_active_task(mut self, graph_id: impl Into<String>) -> Self {
        self.active_task = Some(graph_id.into());
        self
    }

    pub fn with_conversation(
        mut self,
        conversation_id: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        self.conversations.push(ConversationCandidate {
            conversation_id: conversation_id.into(),
            title: title.into(),
        });
        self
    }

    pub fn with_anchor(mut self, anchor: ConversationalAnchor) -> Self {
        self.conversational_anchor = Some(anchor);
        self
    }

    pub fn with_approval(
        mut self,
        approval_id: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        self.approvals.push(ApprovalCandidate {
            approval_id: approval_id.into(),
            title: title.into(),
            conversation_id: None,
            store_conversation_id: None,
        });
        self
    }

    /// Add an approval candidate bound to the currently active conversation.
    /// Bare approve/reject routing only considers candidates carrying this
    /// binding.
    pub fn with_approval_in_conversation(
        mut self,
        approval_id: impl Into<String>,
        conversation_id: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        let conversation_id = conversation_id.into();
        self.approvals.push(ApprovalCandidate {
            approval_id: approval_id.into(),
            title: title.into(),
            conversation_id: Some(conversation_id.clone()),
            store_conversation_id: Some(conversation_id),
        });
        self
    }

    pub fn with_approval_in_context(
        mut self,
        approval_id: impl Into<String>,
        context_id: impl Into<String>,
        store_conversation_id: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        self.approvals.push(ApprovalCandidate {
            approval_id: approval_id.into(),
            title: title.into(),
            conversation_id: Some(context_id.into()),
            store_conversation_id: Some(store_conversation_id.into()),
        });
        self
    }

    pub fn task_targets(&self) -> impl Iterator<Item = InteractionTarget> + '_ {
        self.tasks.iter().map(|candidate| InteractionTarget::Task {
            graph_id: candidate.graph_id.clone(),
        })
    }
}

/// The result of deterministic classification and target resolution.
///
/// `command` is present only for a uniquely resolved command target.  An
/// ambiguous or missing decision therefore cannot be accidentally dispatched
/// by a caller that follows this value instead of re-resolving the target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionDecision {
    pub intent: crate::shared::interaction::InteractionIntent,
    pub target: crate::shared::interaction::TargetResolution,
    pub command: Option<String>,
    /// Local command parameters kept outside the frozen shared interaction
    /// contract. Approval routing uses `resolution` and a conversation
    /// binding so a downstream handler can execute the exact registered
    /// `task.approval.resolve` command.
    pub parameters: serde_json::Value,
}

impl InteractionDecision {
    pub fn is_executable(&self) -> bool {
        self.command.is_some()
            && matches!(
                self.target,
                crate::shared::interaction::TargetResolution::Resolved { .. }
            )
    }
}
