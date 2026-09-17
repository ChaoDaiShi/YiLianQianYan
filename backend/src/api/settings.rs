// ============================================================
// Settings API handlers — write-only Secret UX + safe config.
//
// API keys never round-trip as plaintext: GET returns "" for the literal and a
// computed `configured`/`source` pair; PUT writes new keys into the SecretStore
// behind a stable SecretRef (or clears them) and persists only the ref.
// ============================================================

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use secrecy::SecretString;
use std::sync::Arc;

use crate::config::types::AppConfig;
use crate::llm::client::LlmClient;
use crate::llm::types::ChatMessage;
use crate::safety::AuditEventType;
use crate::secret::{
    record_secret_event, SecretKind, SecretRef, SecretResolver, SecretSource, CHAT_KEY_REF,
    EMBEDDING_KEY_REF, STT_KEY_REF, TTS_KEY_REF, VOICE_KEY_REF,
};
use crate::server::AppServer;
use crate::voice::{AudioInput, SpeechRequest, VoiceProviderError};

#[derive(serde::Serialize)]
struct ProviderReadinessItem {
    configured: bool,
    available: bool,
    provider: String,
    model: String,
}

#[derive(serde::Serialize)]
struct ProviderReadiness {
    model: ProviderReadinessItem,
    stt: ProviderReadinessItem,
    tts: ProviderReadinessItem,
}

async fn delete_secret_or_fail(
    server: &AppServer,
    secret_ref: &SecretRef,
    kind: SecretKind,
) -> Result<(), Json<serde_json::Value>> {
    match server.secret_store.delete(secret_ref).await {
        Ok(()) => {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretDeleted,
                kind,
                secret_ref,
                "clear",
                true,
            );
            Ok(())
        }
        Err(_) => {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretDeleted,
                kind,
                secret_ref,
                "clear",
                false,
            );
            Err(Json(serde_json::json!({
                "error": "系统安全凭据库删除失败，密钥未清除"
            })))
        }
    }
}

pub(crate) async fn chat_source(
    resolver: &SecretResolver,
    config: &AppConfig,
) -> (SecretSource, bool) {
    if let Some(secret_ref) = &config.model.api_key_ref {
        let ok = matches!(resolver.resolve_ref(secret_ref).await, Ok(Some(_)));
        return (SecretSource::SecretStore, ok);
    }
    if !config.model.api_key_env.is_empty() {
        let ok = std::env::var(&config.model.api_key_env).is_ok();
        return (SecretSource::Environment, ok);
    }
    if !config.model.api_key.is_empty() {
        return (SecretSource::LegacyPending, true);
    }
    (SecretSource::None, false)
}

pub(crate) async fn embedding_source(
    resolver: &SecretResolver,
    config: &AppConfig,
) -> (SecretSource, bool) {
    if let Some(secret_ref) = &config.model.embedding_api_key_ref {
        let ok = matches!(resolver.resolve_ref(secret_ref).await, Ok(Some(_)));
        return (SecretSource::SecretStore, ok);
    }
    if !config.model.embedding_api_key_env.is_empty() {
        let ok = std::env::var(&config.model.embedding_api_key_env).is_ok();
        return (SecretSource::Environment, ok);
    }
    if !config.model.embedding_api_key.is_empty() {
        return (SecretSource::LegacyPending, true);
    }
    (SecretSource::None, false)
}

async fn voice_stt_source(resolver: &SecretResolver, config: &AppConfig) -> (SecretSource, bool) {
    if let Some(secret_ref) = &config.voice.stt.api_key_ref {
        let ok = matches!(resolver.resolve_ref(secret_ref).await, Ok(Some(_)));
        return (SecretSource::SecretStore, ok);
    }
    if !config.voice.stt.api_key_env.is_empty() {
        let ok = std::env::var(&config.voice.stt.api_key_env).is_ok();
        return (SecretSource::Environment, ok);
    }
    if !config.voice.stt.api_key.is_empty() {
        return (SecretSource::LegacyPending, true);
    }
    (SecretSource::None, false)
}

async fn voice_tts_source(resolver: &SecretResolver, config: &AppConfig) -> (SecretSource, bool) {
    if let Some(secret_ref) = &config.voice.tts.api_key_ref {
        let ok = matches!(resolver.resolve_ref(secret_ref).await, Ok(Some(_)));
        return (SecretSource::SecretStore, ok);
    }
    if !config.voice.tts.api_key_env.is_empty() {
        let ok = std::env::var(&config.voice.tts.api_key_env).is_ok();
        return (SecretSource::Environment, ok);
    }
    if !config.voice.tts.api_key.is_empty() {
        return (SecretSource::LegacyPending, true);
    }
    (SecretSource::None, false)
}

fn valid_provider_url(value: &str) -> bool {
    url::Url::parse(value.trim())
        .map(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        .unwrap_or(false)
}

fn supported_voice_provider(provider: &str) -> bool {
    provider.eq_ignore_ascii_case("openai-compatible") || provider.eq_ignore_ascii_case("minimax")
}

async fn build_provider_readiness(server: &AppServer, config: &AppConfig) -> ProviderReadiness {
    // Keep the same precedence as chat_handler: the active profile is the
    // runtime model, while legacy settings are only its fallback.
    let active = server.db.get_active_llm_model().ok().flatten();
    let (runtime_model, provider, model) = match active {
        Some(profile) => {
            let provider = profile.provider.clone();
            let model = profile.model.clone();
            (profile.to_model_config(), provider, model)
        }
        None => (
            config.model.clone(),
            config.model.provider.clone(),
            config.model.name.clone(),
        ),
    };
    let runtime_config = AppConfig {
        model: runtime_model.clone(),
        ..config.clone()
    };
    let (_, model_configured) = chat_source(&server.secret_resolver, &runtime_config).await;
    let (_, stt_configured) = voice_stt_source(&server.secret_resolver, config).await;
    let (_, tts_configured) = voice_tts_source(&server.secret_resolver, config).await;

    ProviderReadiness {
        model: ProviderReadinessItem {
            configured: model_configured,
            available: model_configured
                && !provider.trim().is_empty()
                && !model.trim().is_empty()
                && valid_provider_url(&runtime_model.base_url)
                && runtime_model.invoke_timeout_ms > 0,
            provider,
            model,
        },
        stt: ProviderReadinessItem {
            configured: stt_configured,
            available: stt_configured
                && supported_voice_provider(&config.voice.stt.provider)
                && config.voice.stt.structurally_configured(),
            provider: config.voice.stt.provider.clone(),
            model: config.voice.stt.model.clone(),
        },
        tts: ProviderReadinessItem {
            configured: tts_configured,
            available: tts_configured
                && supported_voice_provider(&config.voice.tts.provider)
                && config.voice.tts.structurally_configured(),
            provider: config.voice.tts.provider.clone(),
            model: config.voice.tts.model.clone(),
        },
    }
}

fn active_runtime_model(
    server: &AppServer,
    config: &AppConfig,
) -> crate::config::types::ModelConfig {
    server
        .db
        .get_active_llm_model()
        .ok()
        .flatten()
        .map(|profile| profile.to_model_config())
        .unwrap_or_else(|| config.model.clone())
}

fn provider_test_error(error: &'static str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": error })),
    )
}

fn normalize_voice_connection_error(error: &VoiceProviderError) -> &'static str {
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

fn minimal_silent_wav() -> Vec<u8> {
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

/// Leaf handler for a minimal, real provider call. `api::mod` owns protected
/// router registration; mount this at POST /api/providers/:kind/verify.
pub async fn verify_provider_handler(
    State(server): State<Arc<AppServer>>,
    Path(kind): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = server.config.read().clone();
    let result = match kind.as_str() {
        "model" => {
            let model = active_runtime_model(&server, &config);
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

async fn build_redacted_config(server: &AppServer) -> serde_json::Value {
    let config = server.config.read().clone();
    let mut value = serde_json::to_value(&config).unwrap_or_else(|_| serde_json::json!({}));

    let (chat_src, chat_configured) = chat_source(&server.secret_resolver, &config).await;
    let (embed_src, embed_configured) = embedding_source(&server.secret_resolver, &config).await;
    let (stt_src, stt_configured) = voice_stt_source(&server.secret_resolver, &config).await;
    let (tts_src, tts_configured) = voice_tts_source(&server.secret_resolver, &config).await;
    let readiness = build_provider_readiness(server, &config).await;

    if let Some(model) = value
        .get_mut("model")
        .and_then(serde_json::Value::as_object_mut)
    {
        model.insert(
            "api_key".to_string(),
            serde_json::Value::String(String::new()),
        );
        model.insert(
            "embedding_api_key".to_string(),
            serde_json::Value::String(String::new()),
        );
        model.insert(
            "api_key_configured".to_string(),
            serde_json::Value::Bool(chat_configured),
        );
        model.insert(
            "embedding_api_key_configured".to_string(),
            serde_json::Value::Bool(embed_configured),
        );
        model.insert(
            "api_key_source".to_string(),
            serde_json::to_value(chat_src).unwrap(),
        );
        model.insert(
            "embedding_api_key_source".to_string(),
            serde_json::to_value(embed_src).unwrap(),
        );
    }

    if let Some(stt) = value
        .pointer_mut("/voice/stt")
        .and_then(serde_json::Value::as_object_mut)
    {
        stt.insert(
            "api_key".to_string(),
            serde_json::Value::String(String::new()),
        );
        stt.insert(
            "api_key_configured".to_string(),
            serde_json::Value::Bool(stt_configured),
        );
        stt.insert(
            "api_key_source".to_string(),
            serde_json::to_value(stt_src).unwrap(),
        );
    }
    if let Some(tts) = value
        .pointer_mut("/voice/tts")
        .and_then(serde_json::Value::as_object_mut)
    {
        tts.insert(
            "api_key".to_string(),
            serde_json::Value::String(String::new()),
        );
        tts.insert(
            "api_key_configured".to_string(),
            serde_json::Value::Bool(tts_configured),
        );
        tts.insert(
            "api_key_source".to_string(),
            serde_json::to_value(tts_src).unwrap(),
        );
    }

    let migration_pending = {
        let mut pending = 0usize;
        if !config.model.api_key.is_empty() {
            pending += 1;
        }
        if !config.model.embedding_api_key.is_empty() {
            pending += 1;
        }
        if !config.voice.stt.api_key.is_empty() {
            pending += 1;
        }
        if !config.voice.tts.api_key.is_empty() {
            pending += 1;
        }
        pending += server
            .db
            .list_mcp_servers()
            .unwrap_or_default()
            .iter()
            .filter(|s| s.transport == "stdio")
            .filter(|s| {
                s.env
                    .as_ref()
                    .and_then(|e| e.as_object())
                    .map(|o| !o.is_empty())
                    .unwrap_or(false)
            })
            .count();
        pending
    };
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "secret_store_status".to_string(),
            serde_json::to_value(server.secret_resolver.status().await).unwrap(),
        );
        obj.insert(
            "migration_pending".to_string(),
            serde_json::Value::from(migration_pending as u64),
        );
        obj.insert(
            "provider_readiness".to_string(),
            serde_json::to_value(readiness).unwrap_or_else(|_| serde_json::json!({})),
        );
    }

    value
}

pub async fn get_handler(State(server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    Json(build_redacted_config(&server).await)
}

pub async fn update_handler(
    State(server): State<Arc<AppServer>>,
    Json(mut incoming): Json<AppConfig>,
) -> Json<serde_json::Value> {
    let existing = server.config.read().clone();

    // ── Chat API key: clear → delete; non-empty → rotate; empty → preserve. ──
    if incoming.model.clear_api_key {
        if let Some(secret_ref) = existing.model.api_key_ref.clone() {
            if let Err(error) =
                delete_secret_or_fail(&server, &secret_ref, SecretKind::ChatApiKey).await
            {
                return error;
            }
        }
        incoming.model.api_key_ref = None;
        incoming.model.api_key = String::new();
    } else if !incoming.model.api_key.is_empty() {
        let secret_ref = existing
            .model
            .api_key_ref
            .clone()
            .unwrap_or_else(|| SecretRef::new(CHAT_KEY_REF));
        if let Err(e) = server
            .secret_store
            .put(
                &secret_ref,
                SecretString::from(incoming.model.api_key.clone()),
            )
            .await
        {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretStoreUnavailable,
                SecretKind::ChatApiKey,
                &secret_ref,
                "rotate",
                false,
            );
            return Json(
                serde_json::json!({ "error": format!("系统安全凭据库不可用，API Key 未保存: {e}") }),
            );
        }
        record_secret_event(
            &server.audit_recorder,
            AuditEventType::SecretRotated,
            SecretKind::ChatApiKey,
            &secret_ref,
            "rotate",
            true,
        );
        incoming.model.api_key_ref = Some(secret_ref);
        incoming.model.api_key = String::new();
    } else {
        incoming.model.api_key_ref = existing.model.api_key_ref.clone();
        incoming.model.api_key = String::new();
    }
    incoming.model.clear_api_key = false;

    // ── Embedding API key ──
    if incoming.model.clear_embedding_api_key {
        if let Some(secret_ref) = existing.model.embedding_api_key_ref.clone() {
            if let Err(error) =
                delete_secret_or_fail(&server, &secret_ref, SecretKind::EmbeddingApiKey).await
            {
                return error;
            }
        }
        incoming.model.embedding_api_key_ref = None;
        incoming.model.embedding_api_key = String::new();
    } else if !incoming.model.embedding_api_key.is_empty() {
        let secret_ref = existing
            .model
            .embedding_api_key_ref
            .clone()
            .unwrap_or_else(|| SecretRef::new(EMBEDDING_KEY_REF));
        if let Err(e) = server
            .secret_store
            .put(
                &secret_ref,
                SecretString::from(incoming.model.embedding_api_key.clone()),
            )
            .await
        {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretStoreUnavailable,
                SecretKind::EmbeddingApiKey,
                &secret_ref,
                "rotate",
                false,
            );
            return Json(
                serde_json::json!({ "error": format!("系统安全凭据库不可用，Embedding API Key 未保存: {e}") }),
            );
        }
        record_secret_event(
            &server.audit_recorder,
            AuditEventType::SecretRotated,
            SecretKind::EmbeddingApiKey,
            &secret_ref,
            "rotate",
            true,
        );
        incoming.model.embedding_api_key_ref = Some(secret_ref);
        incoming.model.embedding_api_key = String::new();
    } else {
        incoming.model.embedding_api_key_ref = existing.model.embedding_api_key_ref.clone();
        incoming.model.embedding_api_key = String::new();
    }
    incoming.model.clear_embedding_api_key = false;

    // ── STT API key ──
    if incoming.voice.stt.clear_api_key {
        if let Some(secret_ref) = existing.voice.stt.api_key_ref.clone() {
            let shared_legacy = secret_ref.key == VOICE_KEY_REF
                && existing.voice.tts.api_key_ref.as_ref() == Some(&secret_ref);
            if shared_legacy {
                record_secret_event(
                    &server.audit_recorder,
                    AuditEventType::SecretDeleted,
                    SecretKind::VoiceSttApiKey,
                    &secret_ref,
                    "clear_shared_legacy_ref",
                    true,
                );
            } else if let Err(error) =
                delete_secret_or_fail(&server, &secret_ref, SecretKind::VoiceSttApiKey).await
            {
                return error;
            }
        }
        incoming.voice.stt.api_key_ref = None;
        incoming.voice.stt.api_key = String::new();
    } else if !incoming.voice.stt.api_key.is_empty() {
        let secret_ref = existing
            .voice
            .stt
            .api_key_ref
            .clone()
            .filter(|value| value.key != VOICE_KEY_REF)
            .unwrap_or_else(|| SecretRef::new(STT_KEY_REF));
        if let Err(e) = server
            .secret_store
            .put(
                &secret_ref,
                SecretString::from(incoming.voice.stt.api_key.clone()),
            )
            .await
        {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretStoreUnavailable,
                SecretKind::VoiceSttApiKey,
                &secret_ref,
                "rotate",
                false,
            );
            return Json(
                serde_json::json!({ "error": format!("系统安全凭据库不可用，STT API Key 未保存: {e}") }),
            );
        }
        record_secret_event(
            &server.audit_recorder,
            AuditEventType::SecretRotated,
            SecretKind::VoiceSttApiKey,
            &secret_ref,
            "rotate",
            true,
        );
        incoming.voice.stt.api_key_ref = Some(secret_ref);
        incoming.voice.stt.api_key = String::new();
    } else {
        incoming.voice.stt.api_key_ref = existing.voice.stt.api_key_ref.clone();
        incoming.voice.stt.api_key = String::new();
    }
    incoming.voice.stt.clear_api_key = false;

    // ── TTS API key ──
    if incoming.voice.tts.clear_api_key {
        if let Some(secret_ref) = existing.voice.tts.api_key_ref.clone() {
            let shared_legacy = secret_ref.key == VOICE_KEY_REF
                && existing.voice.stt.api_key_ref.as_ref() == Some(&secret_ref);
            if shared_legacy {
                record_secret_event(
                    &server.audit_recorder,
                    AuditEventType::SecretDeleted,
                    SecretKind::VoiceTtsApiKey,
                    &secret_ref,
                    "clear_shared_legacy_ref",
                    true,
                );
            } else if let Err(error) =
                delete_secret_or_fail(&server, &secret_ref, SecretKind::VoiceTtsApiKey).await
            {
                return error;
            }
        }
        incoming.voice.tts.api_key_ref = None;
        incoming.voice.tts.api_key = String::new();
    } else if !incoming.voice.tts.api_key.is_empty() {
        let secret_ref = existing
            .voice
            .tts
            .api_key_ref
            .clone()
            .filter(|value| value.key != VOICE_KEY_REF)
            .unwrap_or_else(|| SecretRef::new(TTS_KEY_REF));
        if let Err(e) = server
            .secret_store
            .put(
                &secret_ref,
                SecretString::from(incoming.voice.tts.api_key.clone()),
            )
            .await
        {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretStoreUnavailable,
                SecretKind::VoiceTtsApiKey,
                &secret_ref,
                "rotate",
                false,
            );
            return Json(
                serde_json::json!({ "error": format!("系统安全凭据库不可用，TTS API Key 未保存: {e}") }),
            );
        }
        record_secret_event(
            &server.audit_recorder,
            AuditEventType::SecretRotated,
            SecretKind::VoiceTtsApiKey,
            &secret_ref,
            "rotate",
            true,
        );
        incoming.voice.tts.api_key_ref = Some(secret_ref);
        incoming.voice.tts.api_key = String::new();
    } else {
        incoming.voice.tts.api_key_ref = existing.voice.tts.api_key_ref.clone();
        incoming.voice.tts.api_key = String::new();
    }
    incoming.voice.tts.clear_api_key = false;

    match server.db.save_settings(&incoming) {
        Ok(_) => {
            *server.config.write() = incoming;
            Json(serde_json::json!({"status": "saved"}))
        }
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::ControlSession;
    use crate::secret::{InMemorySecretStore, SecretStore, SecretStoreError, SecretStoreStatus};

    struct DeleteFailingStore {
        inner: InMemorySecretStore,
    }

    impl DeleteFailingStore {
        fn new() -> Self {
            Self {
                inner: InMemorySecretStore::new(),
            }
        }
    }

    #[async_trait::async_trait]
    impl SecretStore for DeleteFailingStore {
        async fn put(
            &self,
            secret_ref: &SecretRef,
            value: SecretString,
        ) -> Result<(), SecretStoreError> {
            self.inner.put(secret_ref, value).await
        }

        async fn get(
            &self,
            secret_ref: &SecretRef,
        ) -> Result<Option<SecretString>, SecretStoreError> {
            self.inner.get(secret_ref).await
        }

        async fn delete(&self, _secret_ref: &SecretRef) -> Result<(), SecretStoreError> {
            Err(SecretStoreError::Backend)
        }

        async fn status(&self) -> SecretStoreStatus {
            SecretStoreStatus::Available
        }
    }

    #[tokio::test]
    async fn settings_get_never_returns_secret_value() {
        let path = std::env::temp_dir().join(format!(
            "yilian-settings-redact-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store: Arc<dyn crate::secret::SecretStore> = Arc::new(InMemorySecretStore::new());
        let server = AppServer::new_with_control_session_and_store(
            &path,
            ".",
            ControlSession::generate(),
            Arc::clone(&store),
        )
        .unwrap();
        store
            .put(
                &SecretRef::new(CHAT_KEY_REF),
                SecretString::from("SUPER_SECRET".to_string()),
            )
            .await
            .unwrap();
        {
            let mut cfg = server.config.write();
            cfg.model.api_key_ref = Some(SecretRef::new(CHAT_KEY_REF));
        }

        let json = build_redacted_config(&server).await;
        let text = json.to_string();
        assert!(!text.contains("SUPER_SECRET"));
        assert_eq!(json["model"]["api_key"], "");
        assert_eq!(json["model"]["api_key_configured"], true);
        assert_eq!(json["model"]["api_key_source"], "secret_store");

        drop(server);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn legacy_flat_voice_settings_migrate_without_guessing_models() {
        let migrated: crate::config::types::VoiceConfig =
            serde_json::from_value(serde_json::json!({
                "provider": "openai-compatible",
                "base_url": "https://voice.example/v1",
                "stt_model": "stt-real",
                "tts_model": "tts-real",
                "voice": "legacy-voice",
                "language": "zh",
                "api_key_env": "VOICE_KEY",
                "timeout_ms": 4321
            }))
            .unwrap();

        assert_eq!(migrated.stt.provider, "openai-compatible");
        assert_eq!(migrated.stt.model, "stt-real");
        assert_eq!(migrated.tts.provider, "openai-compatible");
        assert_eq!(migrated.tts.model, "tts-real");
        assert_eq!(migrated.tts.voice, "legacy-voice");
        let encoded = serde_json::to_value(&migrated).unwrap();
        assert!(encoded.get("stt").is_some());
        assert!(encoded.get("tts").is_some());
        assert!(encoded.get("provider").is_none());

        let missing_tts: crate::config::types::VoiceConfig =
            serde_json::from_value(serde_json::json!({
                "provider": "openai-compatible",
                "base_url": "https://voice.example/v1",
                "stt_model": "stt-real",
                "voice": "legacy-voice",
                "language": "zh",
                "api_key_env": "VOICE_KEY",
                "timeout_ms": 4321
            }))
            .unwrap();
        assert!(missing_tts.tts.model.is_empty());
        assert!(!missing_tts.tts.structurally_configured());
    }

    #[tokio::test]
    async fn voice_settings_redact_stt_and_tts_secrets_independently() {
        let path = std::env::temp_dir().join(format!(
            "yilian-settings-voice-split-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store: Arc<dyn crate::secret::SecretStore> = Arc::new(InMemorySecretStore::new());
        let server = AppServer::new_with_control_session_and_store(
            &path,
            ".",
            ControlSession::generate(),
            Arc::clone(&store),
        )
        .unwrap();
        let stt_ref = SecretRef::new(crate::secret::STT_KEY_REF);
        let tts_ref = SecretRef::new(crate::secret::TTS_KEY_REF);
        store
            .put(&stt_ref, SecretString::from("STT_SECRET"))
            .await
            .unwrap();
        store
            .put(&tts_ref, SecretString::from("TTS_SECRET"))
            .await
            .unwrap();
        {
            let mut config = server.config.write();
            config.voice.stt.api_key_ref = Some(stt_ref);
            config.voice.tts.api_key_ref = Some(tts_ref);
        }

        let value = build_redacted_config(&server).await;
        let text = value.to_string();
        assert!(!text.contains("STT_SECRET"));
        assert!(!text.contains("TTS_SECRET"));
        assert_eq!(value["voice"]["stt"]["api_key"], "");
        assert_eq!(value["voice"]["stt"]["api_key_configured"], true);
        assert_eq!(value["voice"]["tts"]["api_key"], "");
        assert_eq!(value["voice"]["tts"]["api_key_configured"], true);

        drop(server);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn clear_delete_failure_preserves_each_persisted_secret_reference() {
        for kind in ["model", "embedding", "stt", "tts"] {
            let path = std::env::temp_dir().join(format!(
                "yilian-settings-clear-failure-{kind}-{}.db",
                uuid::Uuid::new_v4()
            ));
            let store: Arc<dyn SecretStore> = Arc::new(DeleteFailingStore::new());
            let server = Arc::new(
                AppServer::new_with_control_session_and_store(
                    &path,
                    ".",
                    ControlSession::generate(),
                    Arc::clone(&store),
                )
                .unwrap(),
            );
            let secret_ref = SecretRef::new(format!("test.clear.failure.{kind}"));
            store
                .put(&secret_ref, SecretString::from("test-clear-key"))
                .await
                .unwrap();
            let mut persisted = server.config.read().clone();
            match kind {
                "model" => persisted.model.api_key_ref = Some(secret_ref.clone()),
                "embedding" => persisted.model.embedding_api_key_ref = Some(secret_ref.clone()),
                "stt" => {
                    persisted.voice.stt.model = "test-stt".to_string();
                    persisted.voice.stt.api_key_ref = Some(secret_ref.clone());
                }
                "tts" => persisted.voice.tts.api_key_ref = Some(secret_ref.clone()),
                _ => unreachable!(),
            }
            server.db.save_settings(&persisted).unwrap();
            *server.config.write() = persisted.clone();

            let mut incoming = persisted;
            match kind {
                "model" => incoming.model.clear_api_key = true,
                "embedding" => incoming.model.clear_embedding_api_key = true,
                "stt" => incoming.voice.stt.clear_api_key = true,
                "tts" => incoming.voice.tts.clear_api_key = true,
                _ => unreachable!(),
            }
            let response = update_handler(State(Arc::clone(&server)), Json(incoming))
                .await
                .0;
            assert!(
                response.get("error").is_some(),
                "{kind} clear must fail closed"
            );

            let current = server.config.read().clone();
            let retained = match kind {
                "model" => current.model.api_key_ref,
                "embedding" => current.model.embedding_api_key_ref,
                "stt" => current.voice.stt.api_key_ref,
                "tts" => current.voice.tts.api_key_ref,
                _ => unreachable!(),
            };
            assert_eq!(
                retained,
                Some(secret_ref.clone()),
                "{kind} ref must remain live"
            );
            assert!(matches!(store.get(&secret_ref).await, Ok(Some(_))));

            let saved = server.db.get_settings().unwrap();
            let persisted_ref = match kind {
                "model" => saved.model.api_key_ref,
                "embedding" => saved.model.embedding_api_key_ref,
                "stt" => saved.voice.stt.api_key_ref,
                "tts" => saved.voice.tts.api_key_ref,
                _ => unreachable!(),
            };
            assert_eq!(persisted_ref, Some(secret_ref));

            drop(server);
            let _ = std::fs::remove_file(&path);
        }
    }

    #[tokio::test]
    async fn settings_get_projects_active_model_provider_readiness_without_secrets() {
        let path = std::env::temp_dir().join(format!(
            "yilian-settings-readiness-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store: Arc<dyn crate::secret::SecretStore> = Arc::new(InMemorySecretStore::new());
        let server = AppServer::new_with_control_session_and_store(
            &path,
            ".",
            ControlSession::generate(),
            Arc::clone(&store),
        )
        .unwrap();

        let active_ref = SecretRef::new("llm.readiness-active");
        store
            .put(&active_ref, SecretString::from("test-active-key"))
            .await
            .unwrap();
        server
            .db
            .create_llm_model(&crate::db::LlmModelInput {
                id: "readiness-active".into(),
                provider: "profile-provider".into(),
                label: "Active profile".into(),
                model: "profile-model".into(),
                base_url: "https://provider.example/v1".into(),
                api_format: "openai".into(),
                api_key_ref: active_ref.key.clone(),
                api_key_env: String::new(),
                temperature: 0.0,
                max_tokens: 128,
                invoke_timeout_ms: 5_000,
            })
            .unwrap();
        server.db.activate_llm_model("readiness-active").unwrap();

        let value = build_redacted_config(&server).await;
        let readiness = &value["provider_readiness"];
        assert_eq!(readiness["model"]["provider"], "profile-provider");
        assert_eq!(readiness["model"]["model"], "profile-model");
        assert_eq!(readiness["model"]["configured"], true);
        assert_eq!(readiness["model"]["available"], true);
        assert!(!value.to_string().contains("test-active-key"));

        drop(server);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn provider_readiness_fails_closed_for_unsupported_voice_provider() {
        let path = std::env::temp_dir().join(format!(
            "yilian-settings-unsupported-voice-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store: Arc<dyn crate::secret::SecretStore> = Arc::new(InMemorySecretStore::new());
        let server = AppServer::new_with_control_session_and_store(
            &path,
            ".",
            ControlSession::generate(),
            Arc::clone(&store),
        )
        .unwrap();
        let stt_ref = SecretRef::new(STT_KEY_REF);
        store
            .put(&stt_ref, SecretString::from("test-stt-key"))
            .await
            .unwrap();
        {
            let mut config = server.config.write();
            config.voice.stt.provider = "unsupported-provider".into();
            config.voice.stt.model = "configured-model".into();
            config.voice.stt.api_key_ref = Some(stt_ref);
        }

        let readiness = build_redacted_config(&server).await;
        assert_eq!(readiness["provider_readiness"]["stt"]["configured"], true);
        assert_eq!(readiness["provider_readiness"]["stt"]["available"], false);

        drop(server);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn voice_connection_errors_are_reduced_to_the_shared_safe_vocabulary() {
        assert_eq!(
            normalize_voice_connection_error(&VoiceProviderError::AuthFailed),
            "INVALID_CREDENTIAL"
        );
        assert_eq!(
            normalize_voice_connection_error(&VoiceProviderError::RequestFailed(404)),
            "MODEL_NOT_FOUND"
        );
        assert_eq!(
            normalize_voice_connection_error(&VoiceProviderError::RateLimited),
            "RATE_LIMITED"
        );
        assert_eq!(
            normalize_voice_connection_error(&VoiceProviderError::Timeout),
            "TIMEOUT"
        );
        assert_eq!(
            normalize_voice_connection_error(&VoiceProviderError::RequestFailed(503)),
            "PROVIDER_UNREACHABLE"
        );
        assert_eq!(
            normalize_voice_connection_error(&VoiceProviderError::InvalidText),
            "INVALID_CONFIGURATION"
        );
    }

    #[tokio::test]
    async fn provider_connection_leaf_rejects_an_unknown_kind_without_a_network_call() {
        let path = std::env::temp_dir().join(format!(
            "yilian-settings-provider-leaf-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store: Arc<dyn crate::secret::SecretStore> = Arc::new(InMemorySecretStore::new());
        let server = Arc::new(
            AppServer::new_with_control_session_and_store(
                &path,
                ".",
                ControlSession::generate(),
                store,
            )
            .unwrap(),
        );

        let error = verify_provider_handler(State(server), Path("unknown".to_string()))
            .await
            .unwrap_err();
        assert_eq!(error.0, StatusCode::BAD_REQUEST);
        assert_eq!(error.1 .0["error"], "INVALID_CONFIGURATION");

        let _ = std::fs::remove_file(&path);
    }

    #[derive(Clone, Default)]
    struct ProviderMockCalls(Arc<std::sync::Mutex<Vec<(String, String)>>>);

    async fn provider_mock_server(
        failure_statuses: Option<(StatusCode, StatusCode, StatusCode)>,
    ) -> (String, ProviderMockCalls, tokio::task::JoinHandle<()>) {
        use axum::{body::Bytes, http::header::CONTENT_TYPE, routing::post, Router};

        let calls = ProviderMockCalls::default();
        let model_calls = calls.clone();
        let stt_calls = calls.clone();
        let tts_calls = calls.clone();
        let (model_status, stt_status, tts_status) =
            failure_statuses.unwrap_or((StatusCode::OK, StatusCode::OK, StatusCode::OK));
        let app = Router::new()
            .route(
                "/chat/completions",
                post(move |Json(body): Json<serde_json::Value>| {
                    let calls = model_calls.clone();
                    async move {
                        calls
                            .0
                            .lock()
                            .unwrap()
                            .push(("model".to_string(), body.to_string()));
                        if model_status == StatusCode::OK {
                            (
                                StatusCode::OK,
                                Json(serde_json::json!({
                                    "id": "mock-model-response",
                                    "choices": [{
                                        "index": 0,
                                        "message": { "role": "assistant", "content": "ok" },
                                        "finish_reason": "stop"
                                    }]
                                })),
                            )
                        } else {
                            (
                                model_status,
                                Json(
                                    serde_json::json!({ "detail": "mock-provider-response-body" }),
                                ),
                            )
                        }
                    }
                }),
            )
            .route(
                "/audio/transcriptions",
                post(move |body: Bytes| {
                    let calls = stt_calls.clone();
                    async move {
                        calls.0.lock().unwrap().push((
                            "stt".to_string(),
                            String::from_utf8_lossy(&body).to_string(),
                        ));
                        if stt_status == StatusCode::OK {
                            (
                                StatusCode::OK,
                                Json(serde_json::json!({ "text": "mock transcript" })),
                            )
                        } else {
                            (
                                stt_status,
                                Json(
                                    serde_json::json!({ "detail": "mock-provider-response-body" }),
                                ),
                            )
                        }
                    }
                }),
            )
            .route(
                "/audio/speech",
                post(move |Json(body): Json<serde_json::Value>| {
                    let calls = tts_calls.clone();
                    async move {
                        calls
                            .0
                            .lock()
                            .unwrap()
                            .push(("tts".to_string(), body.to_string()));
                        if tts_status == StatusCode::OK {
                            (
                                StatusCode::OK,
                                [(CONTENT_TYPE, "audio/mpeg")],
                                vec![1u8, 2, 3],
                            )
                        } else {
                            (
                                tts_status,
                                [(CONTENT_TYPE, "application/json")],
                                serde_json::to_vec(
                                    &serde_json::json!({ "detail": "mock-provider-response-body" }),
                                )
                                .unwrap(),
                            )
                        }
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (base_url, calls, task)
    }

    async fn provider_test_server(base_url: String) -> Arc<AppServer> {
        let path = std::env::temp_dir().join(format!(
            "yilian-settings-provider-mock-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
        let server = Arc::new(
            AppServer::new_with_control_session_and_store(
                &path,
                ".",
                ControlSession::generate(),
                Arc::clone(&store),
            )
            .unwrap(),
        );
        let chat_ref = SecretRef::new("test.provider.model");
        let stt_ref = SecretRef::new("test.provider.stt");
        let tts_ref = SecretRef::new("test.provider.tts");
        for secret_ref in [&chat_ref, &stt_ref, &tts_ref] {
            store
                .put(secret_ref, SecretString::from("test-provider-key"))
                .await
                .unwrap();
        }
        let mut config = server.config.write();
        config.model.base_url = base_url.clone();
        config.model.name = "mock-model".to_string();
        config.model.api_key_ref = Some(chat_ref);
        config.voice.stt.provider = "openai-compatible".to_string();
        config.voice.stt.base_url = base_url.clone();
        config.voice.stt.model = "mock-stt".to_string();
        config.voice.stt.api_key_ref = Some(stt_ref);
        config.voice.tts.provider = "openai-compatible".to_string();
        config.voice.tts.base_url = base_url;
        config.voice.tts.model = "mock-tts".to_string();
        config.voice.tts.voice = "mock-voice".to_string();
        config.voice.tts.api_key_ref = Some(tts_ref);
        drop(config);
        server
    }

    #[tokio::test]
    async fn provider_connection_leaf_makes_minimal_model_stt_and_tts_mock_calls() {
        let (base_url, calls, task) = provider_mock_server(None).await;
        let server = provider_test_server(base_url).await;
        for kind in ["model", "stt", "tts"] {
            let response =
                verify_provider_handler(State(Arc::clone(&server)), Path(kind.to_string()))
                    .await
                    .unwrap();
            assert_eq!(response.0["status"], "ok");
        }
        let calls = calls.0.lock().unwrap();
        assert!(calls.iter().any(|(kind, body)| kind == "model"
            && body.contains("mock-model")
            && body.contains("ping")));
        assert!(calls
            .iter()
            .any(|(kind, body)| kind == "stt" && body.contains("provider-connection-test.wav")));
        assert!(calls.iter().any(|(kind, body)| kind == "tts"
            && body.contains("你好，我是小涟。")
            && body.contains("mock-tts")));
        task.abort();
    }

    #[tokio::test]
    async fn provider_connection_leaf_normalizes_mock_failures_without_response_bodies() {
        let (base_url, _calls, task) = provider_mock_server(Some((
            StatusCode::UNAUTHORIZED,
            StatusCode::NOT_FOUND,
            StatusCode::TOO_MANY_REQUESTS,
        )))
        .await;
        let server = provider_test_server(base_url).await;
        for (kind, expected) in [
            ("model", "INVALID_CREDENTIAL"),
            ("stt", "MODEL_NOT_FOUND"),
            ("tts", "RATE_LIMITED"),
        ] {
            let error = verify_provider_handler(State(Arc::clone(&server)), Path(kind.to_string()))
                .await
                .unwrap_err();
            assert_eq!(error.1 .0["error"], expected);
            assert!(!error
                .1
                 .0
                .to_string()
                .contains("mock-provider-response-body"));
        }
        task.abort();
    }

    #[tokio::test]
    async fn empty_key_preserves_secret_across_settings_changes_and_restart() {
        let path = std::env::temp_dir().join(format!(
            "yilian-settings-restart-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store: Arc<dyn crate::secret::SecretStore> = Arc::new(InMemorySecretStore::new());
        let server = Arc::new(
            AppServer::new_with_control_session_and_store(
                &path,
                ".",
                ControlSession::generate(),
                Arc::clone(&store),
            )
            .unwrap(),
        );

        let mut first = server.config.read().clone();
        first.model.api_key = "test-replace-key".to_string();
        first.model.base_url = "https://first.example/v1".to_string();
        assert_eq!(
            update_handler(State(Arc::clone(&server)), Json(first))
                .await
                .0["status"],
            "saved"
        );

        let mut changed_without_key = server.config.read().clone();
        changed_without_key.model.api_key = String::new();
        changed_without_key.model.base_url = "https://second.example/v1".to_string();
        assert_eq!(
            update_handler(State(Arc::clone(&server)), Json(changed_without_key))
                .await
                .0["status"],
            "saved"
        );
        assert_eq!(
            server.config.read().model.api_key_ref,
            Some(SecretRef::new(CHAT_KEY_REF))
        );

        drop(server);
        let restarted = AppServer::new_with_control_session_and_store(
            &path,
            ".",
            ControlSession::generate(),
            store,
        )
        .unwrap();
        let redacted = build_redacted_config(&restarted).await;
        assert_eq!(redacted["model"]["api_key"], "");
        assert_eq!(redacted["model"]["api_key_configured"], true);
        assert_eq!(redacted["model"]["base_url"], "https://second.example/v1");
        assert!(matches!(
            restarted
                .secret_resolver
                .resolve_ref(&SecretRef::new(CHAT_KEY_REF))
                .await,
            Ok(Some(_))
        ));

        drop(restarted);
        let _ = std::fs::remove_file(&path);
    }
}
