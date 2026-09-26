//! Provider connection verification — a minimal, real call to the configured
//! model/STT/TTS provider. Responses and errors are reduced to a safe,
//! body-free vocabulary.

use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use crate::app::state::AppServer;
use crate::integrations::llm::client::LlmClient;
use crate::integrations::llm::types::ChatMessage;
use crate::modules::voice::{AudioInput, SpeechRequest, VoiceProviderError};

use crate::modules::settings::domain::policy::supported_voice_provider;
use crate::modules::settings::domain::readiness::active_runtime_model;

pub(crate) fn provider_test_error(error: &'static str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": error })),
    )
}

pub(crate) fn normalize_voice_connection_error(error: &VoiceProviderError) -> &'static str {
    match error {
        VoiceProviderError::AuthFailed | VoiceProviderError::RequestFailed(401 | 403) => {
            "INVALID_CREDENTIAL"
        }
        VoiceProviderError::RateLimited | VoiceProviderError::RequestFailed(429) => "RATE_LIMITED",
        VoiceProviderError::Timeout | VoiceProviderError::SttTimeout => "TIMEOUT",
        VoiceProviderError::RequestFailed(404) => "MODEL_NOT_FOUND",
        VoiceProviderError::ProviderUnavailable
        | VoiceProviderError::RequestFailed(_)
        | VoiceProviderError::ProviderRejected(_) => "PROVIDER_UNREACHABLE",
        VoiceProviderError::InvalidAudio(_)
        | VoiceProviderError::InvalidText
        | VoiceProviderError::InvalidResponse
        | VoiceProviderError::UnsupportedMediaType
        | VoiceProviderError::AudioTooLarge
        | VoiceProviderError::UnsupportedAudio
        | VoiceProviderError::TranscriptionFailed => "INVALID_CONFIGURATION",
    }
}

pub(crate) fn minimal_silent_wav() -> Vec<u8> {
    const SAMPLE_RATE: u32 = 16_000;
    const SAMPLES: u32 = 160;
    const DATA_BYTES: u32 = SAMPLES * 2;
    let mut wav = Vec::with_capacity(44 + DATA_BYTES as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + DATA_BYTES).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&DATA_BYTES.to_le_bytes());
    wav.resize(44 + DATA_BYTES as usize, 0);
    wav
}

/// Leaf entry point for a minimal, real provider call. The HTTP handler in
/// `api::routes` mounts this at POST /api/providers/:kind/verify.
pub(crate) async fn verify_provider(
    server: &AppServer,
    kind: &str,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = server.config.read().clone();
    let result = match kind {
        "model" => {
            let model = active_runtime_model(&server.db, &config);
            let client = LlmClient::new(&model, Arc::clone(&server.secret_resolver));
            client
                .invoke(
                    &[ChatMessage {
                        role: "user".to_string(),
                        content: Some("ping".to_string()),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    }],
                    &[],
                )
                .await
                .map(|_| ())
                .map_err(|error| crate::api::llm_models::normalize_provider_error(&error))
        }
        "stt" => {
            if !supported_voice_provider(&config.voice.stt.provider) {
                Err("INVALID_CONFIGURATION")
            } else {
                match server.stt_provider() {
                    Ok(provider) => provider
                        .transcribe(AudioInput {
                            bytes: minimal_silent_wav(),
                            media_type: "audio/wav".to_string(),
                            filename: "provider-connection-test.wav".to_string(),
                            language: Some(config.voice.stt.language),
                        })
                        .await
                        .map(|_| ())
                        .map_err(|error| normalize_voice_connection_error(&error)),
                    Err(error) => Err(normalize_voice_connection_error(&error)),
                }
            }
        }
        "tts" => {
            if !supported_voice_provider(&config.voice.tts.provider) {
                Err("INVALID_CONFIGURATION")
            } else {
                match server.tts_provider() {
                    Ok(provider) => provider
                        .synthesize(&SpeechRequest {
                            text: "你好，我是小涟。".to_string(),
                            voice: config.voice.tts.voice,
                            language: Some(config.voice.tts.language),
                        })
                        .await
                        .map(|_| ())
                        .map_err(|error| normalize_voice_connection_error(&error)),
                    Err(error) => Err(normalize_voice_connection_error(&error)),
                }
            }
        }
        _ => Err("INVALID_CONFIGURATION"),
    };
    result.map_err(provider_test_error)?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}
