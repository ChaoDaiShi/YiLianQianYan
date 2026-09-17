//! Application-lifetime global Voice session state.
//!
//! A route or surface is allowed to come and go while this runtime remains
//! alive.  The runtime stores only bounded, short-lived voice context; audio
//! bytes are deliberately not represented by any state type here.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::shared::event::{EventHub, YiEvent};
use crate::shared::interaction::{ContextAnchorSnapshot, FocusedSurface};
use crate::shared::voice::{
    GlobalVoiceSession, PresenceActivity, PresenceAttention, PresenceInteraction, PresenceSnapshot,
    VoiceInputLease, VoiceInputOwner, VoiceSessionState, VoiceTurn,
};
use crate::voice::{
    VoiceDispatchError, VoiceDispatchHook, VoiceDispatchOutcome, VoiceDispatchRequest,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const MAX_VOICE_TURN_HISTORY: usize = 64;
pub const MAX_VOICE_TRANSCRIPT_CHARS: usize = 4_096;
const MAX_CONSUMED_LEASE_IDS: usize = 64;
pub const VOICE_APPROVAL_ATTESTATION_TTL_MS: i64 = 120_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceApprovalAttestation {
    pub attestation_id: String,
    pub display_id: String,
    pub approval_id: String,
    pub conversation_id: String,
    pub voice_session_id: String,
    pub generation: u64,
    pub displayed_at: i64,
    pub expires_at: i64,
    pub dispatched_lease_id: Option<String>,
}

impl VoiceApprovalAttestation {
    fn matches(
        &self,
        accepted: &AcceptedFinalTranscript,
        session: &GlobalVoiceSession,
        now: i64,
    ) -> bool {
        accepted.input_owner == VoiceInputOwner::PushToTalk
            && self.dispatched_lease_id.is_none()
            && accepted.created_at >= self.displayed_at
            && self.displayed_at <= now
            && now <= self.expires_at
            && self.voice_session_id == accepted.session_id
            && self.voice_session_id == session.voice_session_id
            && self.generation == accepted.generation
            && self.generation == session.generation
            && session
                .conversational_anchor
                .as_ref()
                .is_some_and(|anchor| anchor.conversation_id == self.conversation_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VoiceRuntimeError {
    #[error("no active voice session")]
    NoActiveSession,
    #[error("voice session id does not match the active session")]
    SessionMismatch,
    #[error("voice input belongs to a stale session generation")]
    StaleGeneration,
    #[error("voice session has ended")]
    SessionEnded,
    #[error("another voice input owner already holds the active lease")]
    LeaseConflict,
    #[error("voice input lease is not active")]
    LeaseNotFound,
    #[error("final transcript has already been accepted for this lease")]
    DuplicateFinal,
    #[error("voice session is not in a state that accepts this operation: {0}")]
    InvalidState(String),
    #[error("voice transcript must be 1-{MAX_VOICE_TRANSCRIPT_CHARS} characters")]
    InvalidTranscript,
    #[error("voice turn does not belong to the active session")]
    InvalidTurn,
}

#[derive(Debug, thiserror::Error)]
pub enum VoiceDispatchOnceError {
    #[error(transparent)]
    Runtime(#[from] VoiceRuntimeError),
    #[error(transparent)]
    Dispatch(#[from] VoiceDispatchError),
}

impl VoiceRuntimeError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NoActiveSession => "no_active_session",
            Self::SessionMismatch => "session_mismatch",
            Self::StaleGeneration => "stale_generation",
            Self::SessionEnded => "session_ended",
            Self::LeaseConflict => "lease_conflict",
            Self::LeaseNotFound => "lease_not_found",
            Self::DuplicateFinal => "duplicate_final",
            Self::InvalidState(_) => "invalid_state",
            Self::InvalidTranscript => "invalid_transcript",
            Self::InvalidTurn => "invalid_turn",
        }
    }
}

/// A final transcript accepted exactly once for a current-generation lease.
/// This value contains text metadata only; raw audio never reaches the
/// runtime snapshot or EventHub.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedFinalTranscript {
    pub session_id: String,
    pub generation: u64,
    pub lease_id: String,
    pub input_owner: VoiceInputOwner,
    pub text: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceRuntimeSnapshot {
    pub session: Option<GlobalVoiceSession>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease: Option<VoiceInputLease>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_transcript: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_transcript: Option<String>,
    #[serde(default)]
    pub turns: Vec<VoiceTurn>,
    pub presence: PresenceSnapshot,
}

struct VoiceRuntimeState {
    session: Option<GlobalVoiceSession>,
    lease: Option<VoiceInputLease>,
    partial_transcript: Option<String>,
    final_transcript: Option<String>,
    turns: VecDeque<VoiceTurn>,
    consumed_lease_ids: VecDeque<String>,
    last_accepted: Option<AcceptedFinalTranscript>,
    approval_attestation: Option<VoiceApprovalAttestation>,
    presence: PresenceSnapshot,
}

/// The one application-lifetime voice session runtime.
#[derive(Clone)]
pub struct GlobalVoiceSessionRuntime {
    state: Arc<Mutex<VoiceRuntimeState>>,
    events: EventHub,
    history_limit: usize,
}

impl GlobalVoiceSessionRuntime {
    pub fn new(events: EventHub) -> Self {
        Self::with_history_limit(events, MAX_VOICE_TURN_HISTORY)
    }

    pub fn with_history_limit(events: EventHub, history_limit: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(VoiceRuntimeState {
                session: None,
                lease: None,
                partial_transcript: None,
                final_transcript: None,
                turns: VecDeque::with_capacity(history_limit.min(MAX_VOICE_TURN_HISTORY)),
                consumed_lease_ids: VecDeque::new(),
                last_accepted: None,
                approval_attestation: None,
                presence: PresenceSnapshot {
                    source: "global-voice-runtime".to_string(),
                    ..PresenceSnapshot::default()
                },
            })),
            events,
            history_limit: history_limit.min(MAX_VOICE_TURN_HISTORY),
        }
    }

    /// Start a session, or update the focused surface of the existing
    /// application-lifetime session.  A route change never creates a second
    /// session and never ends the current one.
    pub fn start(
        &self,
        focused_surface: FocusedSurface,
    ) -> Result<GlobalVoiceSession, VoiceRuntimeError> {
        let (session, presence, event_type) = {
            let mut state = self.state.lock();
            let now = now_millis();
            match state.session.clone() {
                Some(mut existing) if existing.state != VoiceSessionState::Ended => {
                    existing.focused_surface = focused_surface;
                    existing.updated_at = now;
                    state.session = Some(existing.clone());
                    (existing, None, "voice.session.context_changed")
                }
                _ => {
                    let session = GlobalVoiceSession::new(focused_surface, now);
                    state.session = Some(session.clone());
                    state.lease = None;
                    state.partial_transcript = None;
                    state.final_transcript = None;
                    state.last_accepted = None;
                    state.approval_attestation = None;
                    state.consumed_lease_ids.clear();
                    state.presence.activity = PresenceActivity::Working;
                    state.presence.interaction = PresenceInteraction::Listening;
                    state.presence.attention = PresenceAttention::None;
                    state.presence.updated_at = now;
                    (
                        session,
                        Some(state.presence.clone()),
                        "voice.session.started",
                    )
                }
            }
        };
        self.emit_session(event_type, &session);
        if let Some(presence) = presence {
            self.emit_presence(&presence);
        }
        Ok(session)
    }

    pub fn start_default(&self) -> Result<GlobalVoiceSession, VoiceRuntimeError> {
        self.start(FocusedSurface::Conversation)
    }

    /// Reinitialize only the input side of a live session.  The session id is
    /// stable while generation advances, invalidating all late ASR results.
    pub fn reinitialize_input(
        &self,
        session_id: &str,
        generation: u64,
    ) -> Result<GlobalVoiceSession, VoiceRuntimeError> {
        let (session, presence) = {
            let mut state = self.state.lock();
            Self::validate_session(&state, session_id, generation)?;
            let mut session = state
                .session
                .clone()
                .ok_or(VoiceRuntimeError::NoActiveSession)?;
            session.generation = session.generation.checked_add(1).ok_or_else(|| {
                VoiceRuntimeError::InvalidState("generation overflow".to_string())
            })?;
            session.state = VoiceSessionState::Listening;
            session.updated_at = now_millis();
            state.session = Some(session.clone());
            state.lease = None;
            state.partial_transcript = None;
            state.final_transcript = None;
            state.last_accepted = None;
            state.approval_attestation = None;
            state.presence.activity = PresenceActivity::Working;
            state.presence.interaction = PresenceInteraction::Listening;
            state.presence.attention = PresenceAttention::None;
            state.presence.updated_at = session.updated_at;
            (session, state.presence.clone())
        };
        self.emit_session("voice.input.reinitialized", &session);
        self.emit_presence(&presence);
        Ok(session)
    }

    pub fn acquire_lease(
        &self,
        session_id: &str,
        generation: u64,
        owner: VoiceInputOwner,
    ) -> Result<VoiceInputLease, VoiceRuntimeError> {
        let (lease, presence) = {
            let mut state = self.state.lock();
            Self::validate_session(&state, session_id, generation)?;
            if state.lease.is_some() {
                return Err(VoiceRuntimeError::LeaseConflict);
            }
            let session = state
                .session
                .clone()
                .ok_or(VoiceRuntimeError::NoActiveSession)?;
            let lease = VoiceInputLease::new(&session, owner, now_millis());
            state.lease = Some(lease.clone());
            state.presence.activity = PresenceActivity::Working;
            state.presence.interaction = PresenceInteraction::Listening;
            state.presence.updated_at = now_millis();
            (lease, state.presence.clone())
        };
        self.emit("voice.input.lease.acquired", json!({"lease": lease}));
        self.emit_presence(&presence);
        Ok(lease)
    }

    pub fn update_context(
        &self,
        session_id: &str,
        generation: u64,
        context: ContextAnchorSnapshot,
    ) -> Result<GlobalVoiceSession, VoiceRuntimeError> {
        let (session, presence) = {
            let mut state = self.state.lock();
            Self::validate_session(&state, session_id, generation)?;
            let mut session = state
                .session
                .clone()
                .ok_or(VoiceRuntimeError::NoActiveSession)?;
            let identity_changed = session
                .conversational_anchor
                .as_ref()
                .map(|a| &a.conversation_id)
                != context
                    .conversational_anchor
                    .as_ref()
                    .map(|a| &a.conversation_id);
            if identity_changed {
                session.generation = session.generation.checked_add(1).ok_or_else(|| {
                    VoiceRuntimeError::InvalidState("generation overflow".to_string())
                })?;
                session.state = VoiceSessionState::Listening;
                state.lease = None;
                state.last_accepted = None;
                state.approval_attestation = None;
                state.partial_transcript = None;
                state.final_transcript = None;
                state.presence.interaction = PresenceInteraction::Listening;
            }
            session.focused_surface = context.focused_surface;
            session.conversational_anchor = context.conversational_anchor;
            session.active_task = context.active_task;
            session.updated_at = now_millis();
            state.session = Some(session.clone());
            state.presence.updated_at = session.updated_at;
            (session, state.presence.clone())
        };
        self.emit_session("voice.session.context_changed", &session);
        self.emit_presence(&presence);
        Ok(session)
    }

    pub fn attest_displayed_approval(
        &self,
        approval_id: &str,
        conversation_id: &str,
        expected_session_id: &str,
        expected_generation: u64,
        display_id: &str,
        displayed_at: i64,
    ) -> Result<VoiceApprovalAttestation, VoiceRuntimeError> {
        if approval_id.trim().is_empty()
            || conversation_id.trim().is_empty()
            || display_id.trim().is_empty()
        {
            return Err(VoiceRuntimeError::InvalidState(
                "approval attestation identity is missing".to_string(),
            ));
        }
        let mut state = self.state.lock();
        let session = state
            .session
            .clone()
            .ok_or(VoiceRuntimeError::NoActiveSession)?;
        if session.state == VoiceSessionState::Ended {
            return Err(VoiceRuntimeError::SessionEnded);
        }
        if session.voice_session_id != expected_session_id {
            return Err(VoiceRuntimeError::SessionMismatch);
        }
        if session.generation != expected_generation {
            return Err(VoiceRuntimeError::StaleGeneration);
        }
        if !session
            .conversational_anchor
            .as_ref()
            .is_some_and(|anchor| anchor.conversation_id == conversation_id)
        {
            return Err(VoiceRuntimeError::InvalidState(
                "displayed approval does not belong to the active conversation".to_string(),
            ));
        }
        let attestation = VoiceApprovalAttestation {
            attestation_id: uuid::Uuid::new_v4().to_string(),
            display_id: display_id.to_string(),
            approval_id: approval_id.to_string(),
            conversation_id: conversation_id.to_string(),
            voice_session_id: session.voice_session_id,
            generation: session.generation,
            displayed_at,
            expires_at: displayed_at.saturating_add(VOICE_APPROVAL_ATTESTATION_TTL_MS),
            dispatched_lease_id: None,
        };
        state.approval_attestation = Some(attestation.clone());
        Ok(attestation)
    }

    pub fn approval_attestation_for(
        &self,
        accepted: &AcceptedFinalTranscript,
        now: i64,
    ) -> Option<VoiceApprovalAttestation> {
        let state = self.state.lock();
        let session = state.session.as_ref()?;
        state
            .approval_attestation
            .as_ref()
            .filter(|attestation| attestation.matches(accepted, session, now))
            .cloned()
    }

    pub fn revoke_displayed_approval(&self, attestation_id: &str, display_id: &str) -> bool {
        let mut state = self.state.lock();
        let matches = state
            .approval_attestation
            .as_ref()
            .is_some_and(|attestation| {
                attestation.attestation_id == attestation_id && attestation.display_id == display_id
            });
        if matches {
            state.approval_attestation = None;
        }
        matches
    }

    pub fn consume_spoken_approval_attestation(
        &self,
        attestation_id: &str,
        approval_id: &str,
        conversation_id: &str,
        now: i64,
    ) -> Result<(), VoiceRuntimeError> {
        let mut state = self.state.lock();
        let session = state
            .session
            .as_ref()
            .ok_or(VoiceRuntimeError::NoActiveSession)?;
        let valid = state
            .approval_attestation
            .as_ref()
            .is_some_and(|attestation| {
                attestation.attestation_id == attestation_id
                    && attestation.approval_id == approval_id
                    && attestation.conversation_id == conversation_id
                    && attestation.voice_session_id == session.voice_session_id
                    && attestation.generation == session.generation
                    && attestation.displayed_at <= now
                    && now <= attestation.expires_at
                    && attestation.dispatched_lease_id.is_some()
                    && session
                        .conversational_anchor
                        .as_ref()
                        .is_some_and(|anchor| anchor.conversation_id == conversation_id)
            });
        if !valid {
            return Err(VoiceRuntimeError::InvalidState(
                "spoken approval attestation is missing, stale, or mismatched".to_string(),
            ));
        }
        state.approval_attestation = None;
        Ok(())
    }

    pub fn begin_processing(
        &self,
        session_id: &str,
        generation: u64,
    ) -> Result<GlobalVoiceSession, VoiceRuntimeError> {
        let (session, presence) = self.transition(
            session_id,
            generation,
            VoiceSessionState::Processing,
            PresenceInteraction::None,
        )?;
        self.emit_session("voice.processing.started", &session);
        self.emit_presence(&presence);
        Ok(session)
    }

    /// Update a partial transcript for display only.  This operation cannot
    /// create a VoiceTurn or dispatch a command.
    pub fn update_partial(
        &self,
        session_id: &str,
        generation: u64,
        lease_id: &str,
        transcript: impl Into<String>,
    ) -> Result<VoiceRuntimeSnapshot, VoiceRuntimeError> {
        let mut state = self.state.lock();
        Self::validate_session(&state, session_id, generation)?;
        Self::validate_lease(&state, lease_id, generation)?;
        let text = transcript.into().trim().to_string();
        if text.is_empty() || text.chars().count() > MAX_VOICE_TRANSCRIPT_CHARS {
            return Err(VoiceRuntimeError::InvalidTranscript);
        }
        state.partial_transcript = Some(text);
        Ok(Self::snapshot_locked(&state))
    }

    pub fn validate_input(
        &self,
        session_id: &str,
        generation: u64,
        lease_id: &str,
    ) -> Result<(), VoiceRuntimeError> {
        let state = self.state.lock();
        Self::validate_session(&state, session_id, generation)?;
        if state
            .consumed_lease_ids
            .iter()
            .any(|consumed| consumed == lease_id)
        {
            return Err(VoiceRuntimeError::DuplicateFinal);
        }
        Self::validate_lease(&state, lease_id, generation)
    }

    /// Validate an explicit session generation before beginning provider work.
    /// Callers must validate again when committing a state transition because
    /// the provider request itself may race with interruption.
    pub fn validate_generation(
        &self,
        session_id: &str,
        generation: u64,
    ) -> Result<(), VoiceRuntimeError> {
        let state = self.state.lock();
        Self::validate_session(&state, session_id, generation)
    }

    pub fn commit_final(
        &self,
        session_id: &str,
        generation: u64,
        lease_id: &str,
        transcript: impl Into<String>,
    ) -> Result<AcceptedFinalTranscript, VoiceRuntimeError> {
        let (accepted, session, presence) = {
            let mut state = self.state.lock();
            Self::validate_session(&state, session_id, generation)?;
            if state
                .consumed_lease_ids
                .iter()
                .any(|consumed| consumed == lease_id)
            {
                return Err(VoiceRuntimeError::DuplicateFinal);
            }
            Self::validate_lease(&state, lease_id, generation)?;
            let text = transcript.into().trim().to_string();
            if text.is_empty() || text.chars().count() > MAX_VOICE_TRANSCRIPT_CHARS {
                return Err(VoiceRuntimeError::InvalidTranscript);
            }
            let created_at = now_millis();
            let accepted = AcceptedFinalTranscript {
                session_id: session_id.to_string(),
                generation,
                lease_id: lease_id.to_string(),
                input_owner: state
                    .lease
                    .as_ref()
                    .expect("validated lease remains present")
                    .owner,
                text: text.clone(),
                created_at,
            };
            state.final_transcript = Some(text);
            state.partial_transcript = None;
            if state.consumed_lease_ids.len() >= MAX_CONSUMED_LEASE_IDS {
                state.consumed_lease_ids.pop_front();
            }
            state.consumed_lease_ids.push_back(lease_id.to_string());
            state.last_accepted = Some(accepted.clone());
            state.lease = None;
            let mut session = state
                .session
                .clone()
                .ok_or(VoiceRuntimeError::NoActiveSession)?;
            session.state = VoiceSessionState::Processing;
            session.updated_at = created_at;
            state.session = Some(session.clone());
            state.presence.activity = PresenceActivity::Working;
            state.presence.interaction = PresenceInteraction::None;
            state.presence.updated_at = created_at;
            (accepted, session, state.presence.clone())
        };
        self.emit(
            "voice.transcript.final",
            json!({
                "session_id": accepted.session_id,
                "generation": accepted.generation,
                "lease_id": accepted.lease_id,
                "text": accepted.text,
                "created_at": accepted.created_at,
            }),
        );
        self.emit_session("voice.processing.started", &session);
        self.emit_presence(&presence);
        Ok(accepted)
    }

    /// Return the one final transcript that is waiting to be routed into a
    /// VoiceTurn.  The STT endpoint commits the final text before the router
    /// receives it, so the router may consume this hand-off exactly once.
    pub fn accepted_final(
        &self,
        session_id: &str,
        generation: u64,
        lease_id: &str,
        transcript: &str,
    ) -> Result<AcceptedFinalTranscript, VoiceRuntimeError> {
        let state = self.state.lock();
        Self::validate_session(&state, session_id, generation)?;
        let Some(accepted) = state.last_accepted.as_ref() else {
            return Err(VoiceRuntimeError::DuplicateFinal);
        };
        if accepted.session_id != session_id
            || accepted.generation != generation
            || accepted.lease_id != lease_id
            || accepted.text != transcript.trim()
        {
            return Err(VoiceRuntimeError::DuplicateFinal);
        }
        Ok(accepted.clone())
    }

    /// Execute one trusted final transcript and append its VoiceTurn under the
    /// same runtime lock used by interrupt/reinitialize.  This is the
    /// linearization boundary for generation safety and at-most-once domain
    /// effects: either dispatch owns the current generation first, or the
    /// generation change wins and no hook is called.
    pub fn dispatch_once(
        &self,
        accepted: AcceptedFinalTranscript,
        hook: &dyn VoiceDispatchHook,
    ) -> Result<VoiceDispatchOutcome, VoiceDispatchOnceError> {
        let (outcome, presence) = {
            let mut state = self.state.lock();
            Self::validate_session(&state, &accepted.session_id, accepted.generation)?;
            let pending = state
                .last_accepted
                .as_ref()
                .filter(|pending| **pending == accepted)
                .ok_or(VoiceRuntimeError::DuplicateFinal)?;
            if state
                .turns
                .iter()
                .any(|turn| turn.lease_id == accepted.lease_id)
            {
                return Err(VoiceRuntimeError::DuplicateFinal.into());
            }
            let session = state
                .session
                .clone()
                .ok_or(VoiceRuntimeError::NoActiveSession)?;
            let routed_at = now_millis();
            let approval_attestation = state
                .approval_attestation
                .as_ref()
                .filter(|attestation| attestation.matches(pending, &session, routed_at))
                .cloned();
            let dispatch = hook.dispatch(VoiceDispatchRequest {
                accepted: pending.clone(),
                session,
                approval_attestation,
                routed_at,
            });
            // A final is never dispatched twice, including when a domain hook
            // fails after beginning a side effect.
            state.last_accepted = None;
            let mut outcome = dispatch?;
            if let Some(crate::voice::VoiceContinuation::Approval { approval_id, .. }) =
                outcome.continuation.as_ref()
            {
                if let Some(attestation) = state.approval_attestation.as_mut() {
                    if attestation.approval_id == *approval_id
                        && attestation.dispatched_lease_id.is_none()
                    {
                        attestation.dispatched_lease_id = Some(accepted.lease_id.clone());
                    }
                }
            }
            if outcome.turn.voice_session_id != accepted.session_id
                || outcome.turn.generation != accepted.generation
                || outcome.turn.lease_id != accepted.lease_id
            {
                return Err(VoiceRuntimeError::InvalidTurn.into());
            }
            if self.history_limit > 0 {
                while state.turns.len() >= self.history_limit {
                    state.turns.pop_front();
                }
                state.turns.push_back(outcome.turn.clone());
            }
            if let Some(session) = state.session.as_mut() {
                session.state = VoiceSessionState::Listening;
                session.updated_at = now_millis();
            }
            state.presence.activity = PresenceActivity::Working;
            state.presence.interaction = PresenceInteraction::Listening;
            state.presence.updated_at = now_millis();
            outcome.turn = state.turns.back().cloned().unwrap_or(outcome.turn);
            (outcome, state.presence.clone())
        };
        self.emit(
            "voice.turn.accepted",
            json!({
                "turn_id": outcome.turn.turn_id,
                "voice_session_id": outcome.turn.voice_session_id,
                "generation": outcome.turn.generation,
                "source": outcome.turn.source,
                "focused_surface": outcome.turn.focused_surface,
                "resolved_target": outcome.turn.resolved_target,
                "intent": outcome.turn.intent,
            }),
        );
        self.emit_presence(&presence);
        Ok(outcome)
    }

    /// Append a fully routed VoiceTurn to bounded session-local history.
    /// Long-term facts remain in the owning domain; this is routing context
    /// only.
    pub fn record_turn(&self, turn: VoiceTurn) -> Result<VoiceTurn, VoiceRuntimeError> {
        let snapshot = {
            let mut state = self.state.lock();
            let session = state
                .session
                .clone()
                .ok_or(VoiceRuntimeError::NoActiveSession)?;
            if session.voice_session_id != turn.voice_session_id
                || session.generation != turn.generation
            {
                return Err(VoiceRuntimeError::InvalidTurn);
            }
            if state
                .turns
                .iter()
                .any(|existing| existing.lease_id == turn.lease_id)
            {
                return Err(VoiceRuntimeError::DuplicateFinal);
            }
            if self.history_limit > 0 {
                while state.turns.len() >= self.history_limit {
                    state.turns.pop_front();
                }
                state.turns.push_back(turn.clone());
            }
            Self::snapshot_locked(&state)
        };
        self.emit(
            "voice.turn.accepted",
            json!({
                "turn_id": turn.turn_id,
                "voice_session_id": turn.voice_session_id,
                "generation": turn.generation,
                "source": turn.source,
                "focused_surface": turn.focused_surface,
                "resolved_target": turn.resolved_target,
                "intent": turn.intent,
            }),
        );
        Ok(snapshot.turns.last().cloned().unwrap_or(turn))
    }

    pub fn begin_speaking(
        &self,
        session_id: &str,
        generation: u64,
    ) -> Result<GlobalVoiceSession, VoiceRuntimeError> {
        let (session, presence) = self.transition(
            session_id,
            generation,
            VoiceSessionState::Speaking,
            PresenceInteraction::Speaking,
        )?;
        self.emit_session("voice.speech.started", &session);
        self.emit_presence(&presence);
        Ok(session)
    }

    /// Confirm that browser playback reached its natural end. Only the
    /// current Speaking generation may return to Listening; stale callbacks
    /// and synthesis-only requests fail closed.
    pub fn finish_speaking(
        &self,
        session_id: &str,
        generation: u64,
    ) -> Result<GlobalVoiceSession, VoiceRuntimeError> {
        let (session, presence) = {
            let mut state = self.state.lock();
            Self::validate_session(&state, session_id, generation)?;
            let mut session = state
                .session
                .clone()
                .ok_or(VoiceRuntimeError::NoActiveSession)?;
            if session.state != VoiceSessionState::Speaking {
                return Err(VoiceRuntimeError::InvalidState(
                    "speech is not currently playing".to_string(),
                ));
            }
            session.state = VoiceSessionState::Listening;
            session.updated_at = now_millis();
            state.session = Some(session.clone());
            state.presence.activity = PresenceActivity::Working;
            state.presence.interaction = PresenceInteraction::Listening;
            state.presence.updated_at = session.updated_at;
            (session, state.presence.clone())
        };
        self.emit_session("voice.speech.finished", &session);
        self.emit_presence(&presence);
        Ok(session)
    }

    pub fn interrupt(
        &self,
        session_id: &str,
        generation: u64,
    ) -> Result<GlobalVoiceSession, VoiceRuntimeError> {
        let (session, presence) = {
            let mut state = self.state.lock();
            Self::validate_session(&state, session_id, generation)?;
            let mut session = state
                .session
                .clone()
                .ok_or(VoiceRuntimeError::NoActiveSession)?;
            if matches!(session.state, VoiceSessionState::Ended) {
                return Err(VoiceRuntimeError::SessionEnded);
            }
            session.generation = session.generation.checked_add(1).ok_or_else(|| {
                VoiceRuntimeError::InvalidState("generation overflow".to_string())
            })?;
            session.state = VoiceSessionState::Interrupted;
            session.updated_at = now_millis();
            state.session = Some(session.clone());
            state.lease = None;
            state.partial_transcript = None;
            state.final_transcript = None;
            state.last_accepted = None;
            state.approval_attestation = None;
            state.presence.activity = PresenceActivity::Working;
            state.presence.interaction = PresenceInteraction::Interrupted;
            state.presence.updated_at = session.updated_at;
            (session, state.presence.clone())
        };
        self.emit_session("voice.session.interrupted", &session);
        self.emit_presence(&presence);
        Ok(session)
    }

    pub fn end(
        &self,
        session_id: &str,
        generation: u64,
    ) -> Result<GlobalVoiceSession, VoiceRuntimeError> {
        let (session, presence) = {
            let mut state = self.state.lock();
            Self::validate_session(&state, session_id, generation)?;
            let mut session = state
                .session
                .clone()
                .ok_or(VoiceRuntimeError::NoActiveSession)?;
            session.generation = session.generation.checked_add(1).ok_or_else(|| {
                VoiceRuntimeError::InvalidState("generation overflow".to_string())
            })?;
            session.state = VoiceSessionState::Ended;
            session.updated_at = now_millis();
            state.session = Some(session.clone());
            state.lease = None;
            state.partial_transcript = None;
            state.final_transcript = None;
            state.last_accepted = None;
            state.approval_attestation = None;
            state.presence.activity = PresenceActivity::Idle;
            state.presence.interaction = PresenceInteraction::None;
            state.presence.attention = PresenceAttention::None;
            state.presence.updated_at = session.updated_at;
            (session, state.presence.clone())
        };
        self.emit_session("voice.session.ended", &session);
        self.emit_presence(&presence);
        Ok(session)
    }

    pub fn snapshot(&self) -> VoiceRuntimeSnapshot {
        Self::snapshot_locked(&self.state.lock())
    }

    pub fn session(&self) -> Option<GlobalVoiceSession> {
        self.state.lock().session.clone()
    }

    pub fn lease(&self) -> Option<VoiceInputLease> {
        self.state.lock().lease.clone()
    }

    pub fn input_lease(&self) -> Option<VoiceInputLease> {
        self.lease()
    }

    pub fn presence(&self) -> PresenceSnapshot {
        self.state.lock().presence.clone()
    }

    fn transition(
        &self,
        session_id: &str,
        generation: u64,
        next_state: VoiceSessionState,
        interaction: PresenceInteraction,
    ) -> Result<(GlobalVoiceSession, PresenceSnapshot), VoiceRuntimeError> {
        let mut state = self.state.lock();
        Self::validate_session(&state, session_id, generation)?;
        let mut session = state
            .session
            .clone()
            .ok_or(VoiceRuntimeError::NoActiveSession)?;
        if session.state == VoiceSessionState::Ended {
            return Err(VoiceRuntimeError::SessionEnded);
        }
        session.state = next_state;
        session.updated_at = now_millis();
        state.session = Some(session.clone());
        state.presence.activity = PresenceActivity::Working;
        state.presence.interaction = interaction;
        state.presence.updated_at = session.updated_at;
        Ok((session, state.presence.clone()))
    }

    fn validate_session(
        state: &VoiceRuntimeState,
        session_id: &str,
        generation: u64,
    ) -> Result<(), VoiceRuntimeError> {
        let Some(session) = state.session.as_ref() else {
            return Err(VoiceRuntimeError::NoActiveSession);
        };
        if session.voice_session_id != session_id {
            return Err(VoiceRuntimeError::SessionMismatch);
        }
        if session.generation != generation {
            return Err(VoiceRuntimeError::StaleGeneration);
        }
        if session.state == VoiceSessionState::Ended {
            return Err(VoiceRuntimeError::SessionEnded);
        }
        Ok(())
    }

    fn validate_lease(
        state: &VoiceRuntimeState,
        lease_id: &str,
        generation: u64,
    ) -> Result<(), VoiceRuntimeError> {
        let Some(lease) = state.lease.as_ref() else {
            return Err(VoiceRuntimeError::LeaseNotFound);
        };
        if lease.lease_id != lease_id {
            return Err(VoiceRuntimeError::LeaseNotFound);
        }
        if lease.generation != generation {
            return Err(VoiceRuntimeError::StaleGeneration);
        }
        Ok(())
    }

    fn snapshot_locked(state: &VoiceRuntimeState) -> VoiceRuntimeSnapshot {
        VoiceRuntimeSnapshot {
            session: state.session.clone(),
            lease: state.lease.clone(),
            partial_transcript: state.partial_transcript.clone(),
            final_transcript: state.final_transcript.clone(),
            turns: state.turns.iter().cloned().collect(),
            presence: state.presence.clone(),
        }
    }

    fn emit_session(&self, event_type: &str, session: &GlobalVoiceSession) {
        self.emit(event_type, json!({"session": session}));
    }

    fn emit_presence(&self, presence: &PresenceSnapshot) {
        self.emit("presence.changed", json!({"presence": presence}));
    }

    fn emit(&self, event_type: &str, payload: serde_json::Value) {
        let _ = self
            .events
            .publish(YiEvent::new(event_type, "global-voice-runtime", payload));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumed_lease_history_is_bounded_and_new_session_clears_it() {
        let runtime = GlobalVoiceSessionRuntime::new(EventHub::new(128));
        let session = runtime.start(FocusedSurface::Conversation).unwrap();

        for index in 0..65 {
            let lease = runtime
                .acquire_lease(
                    &session.voice_session_id,
                    session.generation,
                    VoiceInputOwner::BuiltinAsr,
                )
                .unwrap();
            runtime
                .commit_final(
                    &session.voice_session_id,
                    session.generation,
                    &lease.lease_id,
                    format!("transcript-{index}"),
                )
                .unwrap();
        }

        assert_eq!(runtime.state.lock().consumed_lease_ids.len(), 64);

        runtime
            .end(&session.voice_session_id, session.generation)
            .unwrap();
        runtime.start(FocusedSurface::Conversation).unwrap();
        assert!(runtime.state.lock().consumed_lease_ids.is_empty());
    }

    #[test]
    fn displayed_approval_attestation_is_identity_generation_and_input_bound() {
        let runtime = GlobalVoiceSessionRuntime::new(EventHub::new(32));
        let started = runtime.start(FocusedSurface::Conversation).unwrap();
        let session = runtime
            .update_context(
                &started.voice_session_id,
                started.generation,
                ContextAnchorSnapshot {
                    focused_surface: FocusedSurface::Conversation,
                    conversational_anchor: Some(
                        crate::shared::interaction::ConversationalAnchor::new(
                            "conversation-a",
                            "A",
                            1,
                        ),
                    ),
                    active_task: None,
                },
            )
            .unwrap();
        let attestation = runtime
            .attest_displayed_approval(
                "approval-a",
                "conversation-a",
                &session.voice_session_id,
                session.generation,
                "display-a",
                10,
            )
            .unwrap();
        assert_eq!(attestation.voice_session_id, session.voice_session_id);
        assert_eq!(attestation.generation, session.generation);

        let automatic = AcceptedFinalTranscript {
            session_id: session.voice_session_id.clone(),
            generation: session.generation,
            lease_id: "lease-auto".into(),
            input_owner: VoiceInputOwner::BuiltinAsr,
            text: "同意".into(),
            created_at: 11,
        };
        assert!(runtime.approval_attestation_for(&automatic, 11).is_none());

        let explicit = AcceptedFinalTranscript {
            input_owner: VoiceInputOwner::PushToTalk,
            lease_id: "lease-explicit".into(),
            ..automatic
        };
        assert!(runtime.approval_attestation_for(&explicit, 11).is_some());

        runtime
            .reinitialize_input(&session.voice_session_id, session.generation)
            .unwrap();
        assert!(runtime.approval_attestation_for(&explicit, 12).is_none());
    }
}

fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
