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

use crate::modules::settings::domain::policy::{
    supported_voice_stt_provider, supported_voice_tts_provider,
};
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

/// Fixed spoken Chinese phrase synthesized with Windows SAPI, mono 16 kHz PCM.
/// Silence is not a connection test: an empty transcript is a valid ASR response.
pub(crate) fn connection_test_audio() -> Vec<u8> {
    include_bytes!("fixtures/provider-connection-test.wav").to_vec()
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
                        reasoning_content: None,
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
        "embedding" => {
            let client = LlmClient::new(&config.model, Arc::clone(&server.secret_resolver));
            match client.embed("忆涟千言：向量连接测试").await {
                Ok(vector) => {
                    return Ok(Json(
                        serde_json::json!({"status":"ok","dimensions":vector.len(),"vector":vector}),
                    ))
                }
                Err(error) => Err(crate::api::llm_models::normalize_provider_error(&error)),
            }
        }
        "stt" => {
            if !supported_voice_stt_provider(&config.voice.stt.provider) {
                Err("INVALID_CONFIGURATION")
            } else {
                match server.stt_provider() {
                    Ok(provider) => provider
                        .transcribe(AudioInput {
                            bytes: connection_test_audio(),
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
            if !supported_voice_tts_provider(&config.voice.tts.provider) {
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
