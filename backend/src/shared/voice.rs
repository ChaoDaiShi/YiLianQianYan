use crate::shared::contracts::SHARED_SCHEMA_VERSION;
use crate::shared::event::{EventHub, YiEvent};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceSessionState {
    Listening,
    Speaking,
    Interrupted,
    Stopped,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceProfile {
    pub id: String,
    pub locale: String,
    pub voice: String,
}

impl Default for VoiceProfile {
    fn default() -> Self {
        Self {
            id: "foundation-default".to_string(),
            locale: "zh-CN".to_string(),
            voice: "deterministic".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceDeviceState {
    pub input_available: bool,
    pub output_available: bool,
    pub provider: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceSession {
    pub id: String,
    pub state: VoiceSessionState,
    pub profile: VoiceProfile,
    pub device: VoiceDeviceState,
    pub updated_at: i64,
    pub schema_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptResult {
    pub session_id: String,
    pub text: String,
    pub provider: String,
    pub schema_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechOutput {
    pub session_id: String,
    pub audio: Vec<u8>,
    pub media_type: String,
    pub provider: String,
}

pub trait AudioInput: Send + Sync {
    fn capture(&self) -> Result<Vec<u8>, VoiceError>;
}

pub trait AudioOutput: Send + Sync {
    fn play(&self, audio: &[u8]) -> Result<(), VoiceError>;
    fn interrupt(&self) -> Result<(), VoiceError>;
}

pub trait STTProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn transcribe(&self, audio: &[u8]) -> Result<String, VoiceError>;
}

pub trait TTSProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn synthesize(&self, text: &str, profile: &VoiceProfile) -> Result<Vec<u8>, VoiceError>;
    fn interrupt(&self) -> Result<(), VoiceError>;
}

#[derive(Default)]
pub struct DeterministicSTTProvider;

impl STTProvider for DeterministicSTTProvider {
    fn name(&self) -> &'static str {
        "deterministic-stt"
    }

    fn transcribe(&self, audio: &[u8]) -> Result<String, VoiceError> {
        Ok(format!("deterministic transcript: {} bytes", audio.len()))
    }
}

#[derive(Default)]
pub struct DeterministicTTSProvider;

impl TTSProvider for DeterministicTTSProvider {
    fn name(&self) -> &'static str {
        "deterministic-tts"
    }

    fn synthesize(&self, text: &str, _profile: &VoiceProfile) -> Result<Vec<u8>, VoiceError> {
        if text.trim().is_empty() {
            return Err(VoiceError::InvalidInput("speech text is empty".to_string()));
        }
        Ok(format!("MOCK-AUDIO:{text}").into_bytes())
    }

    fn interrupt(&self) -> Result<(), VoiceError> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceActivity {
    Idle,
    Working,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceInteraction {
    None,
    Listening,
    Speaking,
    Interrupted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceAttention {
    None,
    Requested,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenceSnapshot {
    pub activity: PresenceActivity,
    pub interaction: PresenceInteraction,
    pub attention: PresenceAttention,
    pub source: String,
    pub updated_at: i64,
}

impl Default for PresenceSnapshot {
    fn default() -> Self {
        Self {
            activity: PresenceActivity::Idle,
            interaction: PresenceInteraction::None,
            attention: PresenceAttention::None,
            source: "voice-foundation".to_string(),
            updated_at: now(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("no active voice session")]
    NoActiveSession,
    #[error("invalid voice state: {0}")]
    InvalidState(String),
    #[error("invalid voice input: {0}")]
    InvalidInput(String),
    #[error("voice provider failed: {0}")]
    Provider(String),
}

struct VoiceState {
    session: Option<VoiceSession>,
    presence: PresenceSnapshot,
}

#[derive(Clone)]
pub struct VoiceCore {
    state: Arc<Mutex<VoiceState>>,
    stt: Arc<dyn STTProvider>,
    tts: Arc<dyn TTSProvider>,
    events: EventHub,
}

impl VoiceCore {
    pub fn new(stt: Arc<dyn STTProvider>, tts: Arc<dyn TTSProvider>, events: EventHub) -> Self {
        Self {
            state: Arc::new(Mutex::new(VoiceState {
                session: None,
                presence: PresenceSnapshot::default(),
            })),
            stt,
            tts,
            events,
        }
    }

    pub fn deterministic(events: EventHub) -> Self {
        Self::new(
            Arc::new(DeterministicSTTProvider),
            Arc::new(DeterministicTTSProvider),
            events,
        )
    }

    pub fn start(&self) -> Result<VoiceSession, VoiceError> {
        let session = VoiceSession {
            id: Uuid::new_v4().to_string(),
            state: VoiceSessionState::Listening,
            profile: VoiceProfile::default(),
            device: VoiceDeviceState {
                input_available: true,
                output_available: true,
                provider: "deterministic".to_string(),
            },
            updated_at: now(),
            schema_version: SHARED_SCHEMA_VERSION,
        };
        let presence = {
            let mut state = self.state.lock();
            state.session = Some(session.clone());
            state.presence.activity = PresenceActivity::Working;
            state.presence.interaction = PresenceInteraction::Listening;
            state.presence.updated_at = now();
            state.presence.clone()
        };
        self.emit_session("voice.session.started", &session);
        self.emit_presence(&presence);
        Ok(session)
    }

    pub fn transcribe(&self, audio: &[u8]) -> Result<TranscriptResult, VoiceError> {
        let session = self.active_session()?;
        if session.state != VoiceSessionState::Listening {
            return Err(VoiceError::InvalidState(
                "session is not listening".to_string(),
            ));
        }
        let text = self.stt.transcribe(audio)?;
        let result = TranscriptResult {
            session_id: session.id,
            text,
            provider: self.stt.name().to_string(),
            schema_version: SHARED_SCHEMA_VERSION,
        };
        let _ = self.events.publish(YiEvent::new(
            "voice.transcript.ready",
            "voice-core",
            json!({"session_id": result.session_id, "text": result.text, "provider": result.provider}),
        ));
        Ok(result)
    }

    pub fn speak(&self, text: &str) -> Result<SpeechOutput, VoiceError> {
        let mut session = self.active_session()?;
        if matches!(
            session.state,
            VoiceSessionState::Stopped | VoiceSessionState::Cancelled
        ) {
            return Err(VoiceError::InvalidState("session is terminal".to_string()));
        }
        let audio = self.tts.synthesize(text, &session.profile)?;
        session.state = VoiceSessionState::Speaking;
        session.updated_at = now();
        let presence = self.update_session(session.clone(), PresenceInteraction::Speaking);
        let _ = self.events.publish(YiEvent::new(
            "voice.speech.started",
            "voice-core",
            json!({"session_id": session.id, "provider": self.tts.name(), "text_length": text.chars().count()}),
        ));
        self.emit_presence(&presence);
        Ok(SpeechOutput {
            session_id: session.id,
            audio,
            media_type: "application/x-yilian-deterministic-audio".to_string(),
            provider: self.tts.name().to_string(),
        })
    }

    pub fn interrupt(&self) -> Result<VoiceSession, VoiceError> {
        let mut session = self.active_session()?;
        if !matches!(
            session.state,
            VoiceSessionState::Listening | VoiceSessionState::Speaking
        ) {
            return Err(VoiceError::InvalidState(
                "session cannot be interrupted".to_string(),
            ));
        }
        self.tts.interrupt()?;
        session.state = VoiceSessionState::Interrupted;
        session.updated_at = now();
        let presence = self.update_session(session.clone(), PresenceInteraction::Interrupted);
        self.emit_session("voice.session.interrupted", &session);
        self.emit_presence(&presence);
        Ok(session)
    }

    pub fn stop(&self) -> Result<VoiceSession, VoiceError> {
        self.finish(VoiceSessionState::Stopped, "voice.session.stopped")
    }

    pub fn cancel(&self) -> Result<VoiceSession, VoiceError> {
        self.finish(VoiceSessionState::Cancelled, "voice.session.cancelled")
    }

    pub fn session(&self) -> Option<VoiceSession> {
        self.state.lock().session.clone()
    }

    pub fn presence(&self) -> PresenceSnapshot {
        self.state.lock().presence.clone()
    }

    fn active_session(&self) -> Result<VoiceSession, VoiceError> {
        self.session().ok_or(VoiceError::NoActiveSession)
    }

    fn finish(
        &self,
        terminal: VoiceSessionState,
        event_type: &str,
    ) -> Result<VoiceSession, VoiceError> {
        let mut session = self.active_session()?;
        if matches!(
            session.state,
            VoiceSessionState::Stopped | VoiceSessionState::Cancelled
        ) {
            return Err(VoiceError::InvalidState(
                "session is already terminal".to_string(),
            ));
        }
        self.tts.interrupt()?;
        session.state = terminal;
        session.updated_at = now();
        let presence = self.update_session(session.clone(), PresenceInteraction::None);
        self.emit_session(event_type, &session);
        self.emit_presence(&presence);
        Ok(session)
    }

    fn update_session(
        &self,
        session: VoiceSession,
        interaction: PresenceInteraction,
    ) -> PresenceSnapshot {
        let mut state = self.state.lock();
        state.session = Some(session);
        state.presence.interaction = interaction;
        state.presence.activity = if interaction == PresenceInteraction::None {
            PresenceActivity::Idle
        } else {
            PresenceActivity::Working
        };
        state.presence.updated_at = now();
        state.presence.clone()
    }

    fn emit_session(&self, event_type: &str, session: &VoiceSession) {
        let _ = self.events.publish(YiEvent::new(
            event_type,
            "voice-core",
            json!({"session": session}),
        ));
    }

    fn emit_presence(&self, presence: &PresenceSnapshot) {
        let _ = self.events.publish(YiEvent::new(
            "presence.changed",
            "voice-core",
            json!({"presence": presence}),
        ));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceAttentionPolicy {
    Silent,
    Balanced,
    Companion,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NarrationRequest {
    pub event_type: String,
    pub summary: String,
    pub schema_version: u32,
}

impl NarrationRequest {
    pub fn new(event_type: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            event_type: event_type.into(),
            summary: summary.into(),
            schema_version: SHARED_SCHEMA_VERSION,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrationDelivery {
    Suppressed,
    Text,
    TextAndVoice,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NarrationResult {
    pub delivery: NarrationDelivery,
    pub text: Option<String>,
    pub deterministic: bool,
    pub schema_version: u32,
}

pub struct DeterministicNarrator;

impl DeterministicNarrator {
    pub fn narrate(request: &NarrationRequest, policy: VoiceAttentionPolicy) -> NarrationResult {
        let (delivery, text) = match policy {
            VoiceAttentionPolicy::Silent => (NarrationDelivery::Suppressed, None),
            VoiceAttentionPolicy::Balanced => {
                (NarrationDelivery::Text, Some(request.summary.clone()))
            }
            VoiceAttentionPolicy::Companion => (
                NarrationDelivery::TextAndVoice,
                Some(request.summary.clone()),
            ),
        };
        NarrationResult {
            delivery,
            text,
            deterministic: true,
            schema_version: SHARED_SCHEMA_VERSION,
        }
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_voice_session_supports_interrupt_and_presence() {
        let core = VoiceCore::deterministic(EventHub::new(16));
        let session = core.start().unwrap();
        assert_eq!(session.state, VoiceSessionState::Listening);
        assert_eq!(core.presence().interaction, PresenceInteraction::Listening);

        let transcript = core.transcribe(b"hello").unwrap();
        assert_eq!(transcript.text, "deterministic transcript: 5 bytes");
        let speech = core.speak("hello").unwrap();
        assert!(!speech.audio.is_empty());
        assert_eq!(core.presence().interaction, PresenceInteraction::Speaking);

        let interrupted = core.interrupt().unwrap();
        assert_eq!(interrupted.state, VoiceSessionState::Interrupted);
        assert_eq!(
            core.presence().interaction,
            PresenceInteraction::Interrupted
        );
    }

    #[test]
    fn stop_and_cancel_are_terminal_and_fail_closed() {
        let core = VoiceCore::deterministic(EventHub::new(4));
        core.start().unwrap();
        assert_eq!(core.stop().unwrap().state, VoiceSessionState::Stopped);
        assert!(core.cancel().is_err());
    }

    #[test]
    fn narration_fallback_obeys_attention_policy() {
        let request = NarrationRequest::new("task.progress", "步骤完成");
        let silent = DeterministicNarrator::narrate(&request, VoiceAttentionPolicy::Silent);
        assert_eq!(silent.delivery, NarrationDelivery::Suppressed);
        let balanced = DeterministicNarrator::narrate(&request, VoiceAttentionPolicy::Balanced);
        assert_eq!(balanced.text.as_deref(), Some("步骤完成"));
        assert_eq!(balanced.delivery, NarrationDelivery::Text);
    }

    #[test]
    fn presence_accepts_unknown_additive_fields() {
        let value = serde_json::json!({
            "activity": "working",
            "interaction": "none",
            "attention": "none",
            "source": "test",
            "updated_at": 1,
            "future": true
        });
        let presence: PresenceSnapshot = serde_json::from_value(value).unwrap();
        assert_eq!(presence.activity, PresenceActivity::Working);
    }
}
