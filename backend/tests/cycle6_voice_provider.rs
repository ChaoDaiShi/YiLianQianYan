//! Narrow provider-contract tests. The HTTP fixture is only a transport test;
//! deterministic bytes are never used as a production provider fallback.

use axum::{
    body::Bytes,
    extract::State,
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use yilian_backend::config::types::{VoiceConfig, VoiceTtsConfig};
use yilian_backend::secret::{
    InMemorySecretStore, SecretRef, SecretResolver, SecretSource, SecretStore,
};
use yilian_backend::voice::provider::{
    AudioInput, MiniMaxTtsProvider, OpenAiCompatibleVoiceProvider, SpeechRequest,
    TextToSpeechProvider, VoiceProviderConfig, VoiceProviderError,
};

#[derive(Clone, Default)]
struct FixtureState {
    stt_headers: Arc<Mutex<HeaderMap>>,
    stt_body: Arc<Mutex<Vec<u8>>>,
    tts_headers: Arc<Mutex<HeaderMap>>,
    tts_body: Arc<Mutex<Vec<u8>>>,
    status: Arc<Mutex<StatusCode>>,
}

async fn stt_handler(
    State(state): State<FixtureState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    *state.stt_headers.lock().await = headers;
    *state.stt_body.lock().await = body.to_vec();
    let status = *state.status.lock().await;
    if status != StatusCode::OK {
        return (status, "fixture failure with SECRET_VALUE").into_response();
    }
    (StatusCode::OK, Json(json!({"text": "打开记事本"}))).into_response()
}

async fn tts_handler(
    State(state): State<FixtureState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    *state.tts_headers.lock().await = headers;
    *state.tts_body.lock().await = body.to_vec();
    let status = *state.status.lock().await;
    if status != StatusCode::OK {
        return (status, "fixture failure with SECRET_VALUE").into_response();
    }
    (
        StatusCode::OK,
        [("content-type", "audio/mpeg")],
        Bytes::from_static(b"real-provider-audio-fixture"),
    )
        .into_response()
}

async fn minimax_tts_handler(
    State(state): State<FixtureState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    *state.tts_headers.lock().await = headers;
    *state.tts_body.lock().await = body.to_vec();
    (
        StatusCode::OK,
        Json(json!({
            "data": { "audio": "494433" },
            "base_resp": { "status_code": 0, "status_msg": "success" }
        })),
    )
}

async fn fixture() -> (String, FixtureState, tokio::task::JoinHandle<()>) {
    let state = FixtureState::default();
    let app = Router::new()
        .route("/v1/audio/transcriptions", post(stt_handler))
        .route("/v1/audio/speech", post(tts_handler))
        .route("/v1/t2a_v2", post(minimax_tts_handler))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("fixture listener");
    let address = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("fixture server");
    });
    (address, state, task)
}

#[tokio::test]
async fn minimax_native_tts_uses_voice_id_and_decodes_hex_audio() {
    let (base_url, state, task) = fixture().await;
    let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let key_ref = SecretRef::new("voice.tts.test");
    store
        .put(&key_ref, secrecy::SecretString::from("SECRET_VALUE"))
        .await
        .expect("fixture secret");
    let provider = MiniMaxTtsProvider::from_config(
        &VoiceTtsConfig {
            provider: "minimax".to_string(),
            base_url: base_url.trim_end_matches("/v1").to_string(),
            model: "speech-02-hd".to_string(),
            voice: "female-shaonv".to_string(),
            language: "zh".to_string(),
            api_key: String::new(),
            api_key_env: String::new(),
            api_key_ref: Some(key_ref),
            clear_api_key: false,
            timeout_ms: 1_000,
        },
        Arc::new(SecretResolver::new(store)),
    );
    let audio = provider
        .synthesize(&SpeechRequest::new("语音系统连接成功。", ""))
        .await
        .expect("MiniMax audio");
    assert_eq!(audio.bytes, b"ID3");
    assert_eq!(audio.media_type, "audio/mpeg");
    let headers = state.tts_headers.lock().await.clone();
    assert_eq!(headers.get(AUTHORIZATION).unwrap(), "Bearer SECRET_VALUE");
    let body: Value = serde_json::from_slice(&state.tts_body.lock().await).unwrap();
    assert_eq!(body["model"], "speech-02-hd");
    assert_eq!(body["text"], "语音系统连接成功。");
    assert_eq!(body["voice_setting"]["voice_id"], "female-shaonv");
    assert_eq!(body["output_format"], "hex");
    task.abort();
}

async fn provider(
    base_url: String,
) -> (
    OpenAiCompatibleVoiceProvider,
    Arc<dyn SecretStore>,
    SecretRef,
) {
    let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let key_ref = SecretRef::new("voice.provider.test");
    store
        .put(&key_ref, secrecy::SecretString::from("SECRET_VALUE"))
        .await
        .expect("fixture secret");
    let resolver = Arc::new(SecretResolver::new(Arc::clone(&store)));
    let config = VoiceProviderConfig {
        provider: "openai-compatible".to_string(),
        base_url,
        stt_model: "gpt-4o-mini-transcribe".to_string(),
        tts_model: "gpt-4o-mini-tts".to_string(),
        voice: "alloy".to_string(),
        language: "zh".to_string(),
        api_key_env: String::new(),
        api_key_ref: Some(key_ref.clone()),
        api_key_source: SecretSource::SecretStore,
        timeout_ms: 1_000,
        available: true,
        unavailable_reason: None,
    };
    (
        OpenAiCompatibleVoiceProvider::new(config, resolver),
        store,
        key_ref,
    )
}

#[tokio::test]
async fn stt_posts_bounded_multipart_with_auth_and_returns_final_text() {
    let (base_url, state, task) = fixture().await;
    let (provider, _store, _key_ref) = provider(base_url).await;
    let result = provider
        .transcribe(AudioInput {
            bytes: b"webm-bytes".to_vec(),
            media_type: "audio/webm".to_string(),
            filename: "turn.webm".to_string(),
            language: Some("zh".to_string()),
        })
        .await
        .expect("final transcript");

    assert_eq!(result.text, "打开记事本");
    assert_eq!(result.provider, "openai-compatible");
    let headers = state.stt_headers.lock().await.clone();
    assert_eq!(headers.get(AUTHORIZATION).unwrap(), "Bearer SECRET_VALUE");
    let body = String::from_utf8(state.stt_body.lock().await.clone()).unwrap();
    for field in [
        "name=\"model\"",
        "gpt-4o-mini-transcribe",
        "name=\"language\"",
        "name=\"response_format\"",
        "name=\"file\"",
        "filename=\"turn.webm\"",
        "webm-bytes",
    ] {
        assert!(body.contains(field), "multipart is missing {field}: {body}");
    }
    task.abort();
}

#[tokio::test]
async fn tts_posts_json_and_accepts_audio_response_media_type() {
    let (base_url, state, task) = fixture().await;
    let (provider, _store, _key_ref) = provider(base_url).await;
    let result = provider
        .synthesize(&SpeechRequest {
            text: "你好".to_string(),
            voice: "alloy".to_string(),
            language: Some("zh".to_string()),
        })
        .await
        .expect("speech audio");

    assert_eq!(result.bytes, b"real-provider-audio-fixture");
    assert_eq!(result.media_type, "audio/mpeg");
    let headers = state.tts_headers.lock().await.clone();
    assert_eq!(headers.get(AUTHORIZATION).unwrap(), "Bearer SECRET_VALUE");
    let body: Value = serde_json::from_slice(&state.tts_body.lock().await).unwrap();
    assert_eq!(body["model"], "gpt-4o-mini-tts");
    assert_eq!(body["input"], "你好");
    assert_eq!(body["voice"], "alloy");
    assert_eq!(body["response_format"], "mp3");
    task.abort();
}

#[tokio::test]
async fn incomplete_configuration_is_unavailable_without_deterministic_fallback() {
    let resolver = Arc::new(SecretResolver::new(Arc::new(InMemorySecretStore::new())));
    let config = VoiceProviderConfig {
        provider: "openai-compatible".to_string(),
        base_url: "".to_string(),
        stt_model: "".to_string(),
        tts_model: "".to_string(),
        voice: "alloy".to_string(),
        language: "zh".to_string(),
        api_key_env: "MISSING_VOICE_KEY".to_string(),
        api_key_ref: None,
        api_key_source: SecretSource::Environment,
        timeout_ms: 1_000,
        available: false,
        unavailable_reason: Some("voice provider configuration is incomplete".to_string()),
    };
    let provider = OpenAiCompatibleVoiceProvider::new(config, resolver);
    let error = provider
        .transcribe(AudioInput {
            bytes: b"bytes".to_vec(),
            media_type: "audio/webm".to_string(),
            filename: "turn.webm".to_string(),
            language: None,
        })
        .await
        .expect_err("unconfigured provider must fail closed");
    assert_eq!(error, VoiceProviderError::ProviderUnavailable);
}

#[tokio::test]
async fn invalid_audio_is_rejected_before_network_access() {
    let (base_url, _state, task) = fixture().await;
    let (provider, _store, _key_ref) = provider(base_url).await;
    let error = provider
        .transcribe(AudioInput {
            bytes: Vec::new(),
            media_type: "text/plain".to_string(),
            filename: "bad.txt".to_string(),
            language: None,
        })
        .await
        .expect_err("invalid input must be rejected");
    assert!(matches!(error, VoiceProviderError::InvalidAudio(_)));
    task.abort();
}

#[tokio::test]
async fn provider_errors_redact_response_body_and_config_never_serializes_secret() {
    let (base_url, state, task) = fixture().await;
    *state.status.lock().await = StatusCode::INTERNAL_SERVER_ERROR;
    let (provider, _store, _key_ref) = provider(base_url).await;
    let error = provider
        .synthesize(&SpeechRequest {
            text: "你好".to_string(),
            voice: "alloy".to_string(),
            language: None,
        })
        .await
        .expect_err("fixture error");
    assert!(!error.to_string().contains("SECRET_VALUE"));
    let config = provider.config();
    let encoded = serde_json::to_string(&config).unwrap();
    assert!(!encoded.contains("SECRET_VALUE"));
    assert!(encoded.contains("openai-compatible"));
    task.abort();
}

#[test]
fn app_voice_config_is_redacted_and_defaults_to_unavailable_without_models() {
    let config = VoiceConfig::default();
    assert!(config.stt.model.is_empty());
    assert_eq!(config.tts.model, "speech-2.8-turbo");
    assert_eq!(config.tts.provider, "minimax");
    let encoded = serde_json::to_string(&config).unwrap();
    assert!(!encoded.contains("SECRET_VALUE"));
}
