use serde::{Deserialize, Serialize};

pub const MAX_INTERACTION_UTTERANCE_CHARS: usize = 4_096;
pub const MAX_INTERACTION_IDENTIFIER_CHARS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusedSurface {
    Conversation,
    TaskCanvas,
    Workspace,
    Memory,
    CapabilityCenter,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationalAnchor {
    pub conversation_id: String,
    pub title: String,
    pub updated_at: i64,
}

impl ConversationalAnchor {
    pub fn new(
        conversation_id: impl Into<String>,
        title: impl Into<String>,
        updated_at: i64,
    ) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            title: title.into(),
            updated_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextAnchorSnapshot {
    pub focused_surface: FocusedSurface,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversational_anchor: Option<ConversationalAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_task: Option<String>,
}

impl ContextAnchorSnapshot {
    pub fn new(focused_surface: FocusedSurface) -> Self {
        Self {
            focused_surface,
            conversational_anchor: None,
            active_task: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionSource {
    Text,
    Voice,
    GlobalCommand,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionInput {
    pub source: InteractionSource,
    pub utterance: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_session_id: Option<String>,
    pub focused_surface: FocusedSurface,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversational_anchor: Option<ConversationalAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_task: Option<String>,
}

impl InteractionInput {
    pub fn validate(&self) -> Result<(), InteractionContractError> {
        let utterance = self.utterance.trim();
        if utterance.is_empty() || utterance.chars().count() > MAX_INTERACTION_UTTERANCE_CHARS {
            return Err(InteractionContractError::InvalidUtterance);
        }
        for identifier in [
            self.voice_session_id.as_deref(),
            self.active_task.as_deref(),
            self.conversational_anchor
                .as_ref()
                .map(|anchor| anchor.conversation_id.as_str()),
        ]
        .into_iter()
        .flatten()
        {
            if identifier.trim().is_empty()
                || identifier.chars().count() > MAX_INTERACTION_IDENTIFIER_CHARS
            {
                return Err(InteractionContractError::InvalidIdentifier);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionIntent {
    ConversationTurn,
    Query { name: String },
    Command { name: String },
    GraphMutationProposal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionTarget {
    Conversation { conversation_id: String },
    Task { graph_id: String },
    Approval { approval_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TargetResolution {
    Resolved { target: InteractionTarget },
    Ambiguous { candidates: Vec<InteractionTarget> },
    Missing { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphMutationOperation {
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_node_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphProposalValidation {
    pub valid: bool,
    #[serde(default)]
    pub issues: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphMutationProposal {
    pub proposal_id: String,
    pub graph_id: String,
    pub base_revision: u64,
    pub candidate_revision: u64,
    pub operations: Vec<GraphMutationOperation>,
    pub validation: GraphProposalValidation,
    pub impact_summary: String,
    pub requires_confirmation: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InteractionContractError {
    #[error("interaction utterance must be 1-4096 characters")]
    InvalidUtterance,
    #[error("interaction identifier must be 1-128 characters")]
    InvalidIdentifier,
}

impl InteractionContractError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidUtterance => "invalid_utterance",
            Self::InvalidIdentifier => "invalid_identifier",
        }
    }
}
