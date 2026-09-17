//! Trusted hand-off from the voice runtime into the existing interaction
//! router.  The HTTP endpoint supplies only an accepted transcript and
//! generation metadata; this hook receives the authoritative session context
//! from the server runtime and is the only place allowed to resolve intent or
//! target.

use serde::Serialize;

use crate::shared::command::CommandResult;
use crate::shared::voice::{GlobalVoiceSession, VoiceTurn};
use crate::voice::runtime::AcceptedFinalTranscript;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceApprovalDecision {
    Approve,
    Reject,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VoiceContinuation {
    Conversation {
        conversation_id: String,
        message: String,
    },
    Approval {
        approval_id: String,
        conversation_id: String,
        decision: VoiceApprovalDecision,
    },
}

#[derive(Clone, Debug)]
pub struct VoiceDispatchRequest {
    pub accepted: AcceptedFinalTranscript,
    pub session: GlobalVoiceSession,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum VoiceDispatchError {
    #[error("trusted voice dispatch router is unavailable")]
    Unavailable,
    #[error("trusted voice dispatch router rejected the transcript: {0}")]
    Rejected(String),
}

/// Trusted routing result returned to the global Voice host.  The session-local
/// `VoiceTurn` stays the routing fact, while domain execution evidence and a
/// short human narration remain an API response rather than a second history.
#[derive(Clone, Debug, Serialize)]
pub struct VoiceDispatchOutcome {
    pub turn: VoiceTurn,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_result: Option<CommandResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narration: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<VoiceContinuation>,
}

impl VoiceDispatchOutcome {
    pub fn turn_only(turn: VoiceTurn) -> Self {
        Self {
            turn,
            command_result: None,
            narration: None,
            continuation: None,
        }
    }
}

pub trait VoiceDispatchHook: Send + Sync {
    fn dispatch(
        &self,
        request: VoiceDispatchRequest,
    ) -> Result<VoiceDispatchOutcome, VoiceDispatchError>;
}
