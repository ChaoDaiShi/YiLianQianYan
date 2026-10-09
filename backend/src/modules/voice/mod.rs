//! Global voice runtime and domain adapters.
//!
//! The voice module owns only voice lifecycle and routing context.  It does
//! not own conversations, TaskGraph state, or memory.

pub mod dispatch;
pub mod provider;
pub mod runtime;

pub use runtime::{
    AcceptedFinalTranscript, GlobalVoiceSessionRuntime, VoiceApprovalAttestation,
    VoiceDispatchOnceError, VoiceRuntimeError, VoiceRuntimeSnapshot,
};

pub use dispatch::{
    VoiceApprovalDecision, VoiceContinuation, VoiceDispatchError, VoiceDispatchHook,
    VoiceDispatchOutcome, VoiceDispatchRequest,
};

pub use provider::{
    AudioInput, FinalTranscript, MiniMaxSttProvider, MiniMaxTtsProvider,
    OpenAICompatibleVoiceProvider, OpenAiCompatibleSttProvider, OpenAiCompatibleTtsProvider,
    OpenAiCompatibleVoiceProvider, SpeechAudio, SpeechRequest, SpeechToTextProvider,
    TextToSpeechProvider, VoiceProviderConfig, VoiceProviderError,
};
