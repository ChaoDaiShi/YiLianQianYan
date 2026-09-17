//! Focused contract tests for the official MiniMax Speech-to-Text API.
//!
//! These tests exercise the provider boundary with a local HTTP fixture.  No
//! real credential, microphone, or MiniMax request is used here.

use axum::{
    body::{to_bytes, Body, Bytes},
    extract::State,
    http::{header::AUTHORIZATION, HeaderMap, Method, Request, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::{
    sync::Mutex,
    time::{sleep, Duration},
};
use tower::ServiceExt;

use yilian_backend::config::types::{VoiceConfig, VoiceSttConfig};
use yilian_backend::safety::{ControlSession, CONTROL_SESSION_HEADER};
use yilian_backend::secret::{InMemorySecretStore, SecretRef, SecretResolver, SecretStore};
use yilian_backend::shared::{interaction::FocusedSurface, voice::VoiceInputOwner};
use yilian_backend::voice::provider::{
    AudioInput, MiniMaxSttProvider, SpeechToTextProvider, VoiceProviderError,
};
use yilian_backend::{api, AppServer};

#[derive(Clone)]
struct FixtureState {
    requests: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    headers: Arc<Mutex<HeaderMap>>,
    body: Arc<Mutex<Vec<u8>>>,
    statuses: Arc<Mutex<Vec<StatusCode>>>,
    response_body: Arc<Mutex<Value>>,
    delay_ms: u64,
}

impl FixtureState {
    fn with_statuses(statuses: Vec<StatusCode>) -> Self {
        Self {
            requests: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
            headers: Arc::new(Mutex::new(HeaderMap::new())),
            body: Arc::new(Mutex::new(Vec::new())),
            statuses: Arc::new(Mutex::new(statuses)),
            response_body: Arc::new(Mutex::new(json!({
                "text": "打开记事本",
                "duration": 1.25,
                "trace_id": "fixture-trace"
            }))),
            delay_ms: 0,
        }
    }
}

async fn stt_handler(
    State(state): State<FixtureState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    state.requests.fetch_add(1, Ordering::SeqCst);
    *state.headers.lock().await = headers;
    *state.body.lock().await = body.to_vec();

    let active = state.active.fetch_add(1, Ordering::SeqCst) + 1;
    state.max_active.fetch_max(active, Ordering::SeqCst);
    if state.delay_ms > 0 {
        sleep(Duration::from_millis(state.delay_ms)).await;
    }
    let status = state
        .statuses
        .lock()
        .await
        .first()
        .copied()
        .unwrap_or(StatusCode::OK);
    if state.statuses.lock().await.len() > 1 {
        state.statuses.lock().await.remove(0);
    }
    state.active.fetch_sub(1, Ordering::SeqCst);

    if status != StatusCode::OK {
        return (
            status,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "rate_limit_error",
                    "message": "fixture failure with SECRET_VALUE",
                    "http_code": status.as_u16().to_string()
                },
                "request_id": "fixture-request"
            })),
        )
            .into_response();
    }

    let response = state.response_body.lock().await.clone();
    (StatusCode::OK, Json(response)).into_response()
}

async fn fixture(state: FixtureState) -> (String, FixtureState, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route("/v1/speech_to_text", post(stt_handler))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("fixture listener");
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("fixture server");
    });
    (base_url, state, task)
}

async fn provider(
    base_url: String,
    timeout_ms: u64,
) -> (MiniMaxSttProvider, Arc<dyn SecretStore>, SecretRef) {
    provider_with_language(base_url, timeout_ms, "zh").await
}

async fn provider_with_language(
    base_url: String,
    timeout_ms: u64,
    language: &str,
) -> (MiniMaxSttProvider, Arc<dyn SecretStore>, SecretRef) {
    let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let key_ref = SecretRef::new("voice.stt.test");
    store
        .put(&key_ref, secrecy::SecretString::from("SECRET_VALUE"))
        .await
        .expect("fixture secret");
    let config = VoiceSttConfig {
        provider: "minimax".to_string(),
        base_url,
        model: "asr-1.0".to_string(),
        language: language.to_string(),
        api_key: String::new(),
        api_key_env: String::new(),
        api_key_ref: Some(key_ref.clone()),
        clear_api_key: false,
        timeout_ms,
    };
    (
        MiniMaxSttProvider::from_config(&config, Arc::new(SecretResolver::new(Arc::clone(&store)))),
        store,
        key_ref,
    )
}

fn audio(media_type: &str, filename: &str) -> AudioInput {
    AudioInput {
        bytes: b"audio-fixture".to_vec(),
        media_type: media_type.to_string(),
        filename: filename.to_string(),
        language: None,
    }
}

#[tokio::test]
async fn minimax_stt_uses_official_multipart_wire_and_normalizes_final_response() {
    let state = FixtureState::with_statuses(vec![StatusCode::OK]);
    let (base_url, state, task) = fixture(state).await;
    let (provider, _store, _key_ref) = provider(base_url, 1_000).await;

    let result = provider
        .transcribe(audio("audio/mpeg", "turn.mp3"))
        .await
        .expect("final transcript");

    assert_eq!(result.text, "打开记事本");
    assert_eq!(result.provider, "minimax");
    assert_eq!(result.model, "asr-1.0");
    assert!(result.is_final);
    assert_eq!(result.duration, Some(1.25));
    assert_eq!(result.language.as_deref(), Some("zh"));

    let headers = state.headers.lock().await.clone();
    assert_eq!(headers.get(AUTHORIZATION).unwrap(), "Bearer SECRET_VALUE");
    assert_eq!(headers.get("language").unwrap(), "zh");
    let body = String::from_utf8(state.body.lock().await.clone()).unwrap();
    for field in [
        "name=\"model\"",
        "asr-1.0",
        "name=\"response_format\"",
        "json",
        "name=\"stream\"",
        "false",
        "name=\"file\"",
        "filename=\"turn.mp3\"",
        "audio-fixture",
    ] {
        assert!(body.contains(field), "multipart is missing {field}: {body}");
    }
    task.abort();
}

#[tokio::test]
async fn empty_language_uses_official_mixed_language_mode_without_header() {
    let state = FixtureState::with_statuses(vec![StatusCode::OK]);
    let (base_url, state, task) = fixture(state).await;
    let (provider, _store, _key_ref) = provider_with_language(base_url, 1_000, "").await;

    let mut input = audio("audio/ogg", "turn.ogg");
    input.language = None;
    let result = provider.transcribe(input).await.expect("mixed transcript");
    assert!(result.is_final);
    assert!(state.headers.lock().await.get("language").is_none());
    task.abort();
}

#[tokio::test]
async fn auto_language_maps_to_the_official_omitted_mixed_language_header() {
    let state = FixtureState::with_statuses(vec![StatusCode::OK]);
    let (base_url, state, task) = fixture(state).await;
    let (provider, _store, _key_ref) = provider_with_language(base_url, 1_000, "auto").await;

    let result = provider
        .transcribe(audio("audio/opus", "turn.opus"))
        .await
        .expect("auto-language transcript");
    assert!(result.is_final);
    assert!(result.language.is_none());
    assert!(state.headers.lock().await.get("language").is_none());
    task.abort();
}

#[tokio::test]
async fn minimax_rejects_unsupported_audio_before_network_access() {
    let state = FixtureState::with_statuses(vec![StatusCode::OK]);
    let (base_url, state, task) = fixture(state).await;
    let (provider, _store, _key_ref) = provider(base_url, 1_000).await;

    let error = provider
        .transcribe(audio("audio/webm", "turn.webm"))
        .await
        .expect_err("official ASR does not accept WebM");
    assert_eq!(error, VoiceProviderError::UnsupportedAudio);
    assert_eq!(state.requests.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn minimax_maps_auth_rate_limit_and_size_errors_without_body_leakage() {
    for (status, expected) in [
        (StatusCode::UNAUTHORIZED, VoiceProviderError::AuthFailed),
        (
            StatusCode::TOO_MANY_REQUESTS,
            VoiceProviderError::RateLimited,
        ),
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            VoiceProviderError::AudioTooLarge,
        ),
    ] {
        let state = FixtureState::with_statuses(vec![status]);
        let (base_url, _state, task) = fixture(state).await;
        let (provider, _store, _key_ref) = provider(base_url, 1_000).await;
        let error = provider
            .transcribe(audio("audio/wav", "turn.wav"))
            .await
            .expect_err("fixture error");
        assert_eq!(error, expected);
        assert!(!error.to_string().contains("SECRET_VALUE"));
        task.abort();
    }
}

#[tokio::test]
async fn minimax_retries_one_time_only_for_server_errors() {
    let state =
        FixtureState::with_statuses(vec![StatusCode::INTERNAL_SERVER_ERROR, StatusCode::OK]);
    let (base_url, state, task) = fixture(state).await;
    let (retry_provider, _store, _key_ref) = provider(base_url, 1_000).await;
    let result = retry_provider
        .transcribe(audio("audio/aac", "turn.aac"))
        .await
        .expect("retry should recover");
    assert_eq!(result.text, "打开记事本");
    assert_eq!(state.requests.load(Ordering::SeqCst), 2);
    task.abort();

    let state = FixtureState::with_statuses(vec![
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::OK,
    ]);
    let (base_url, state, task) = fixture(state).await;
    let (provider, _store, _key_ref) = provider(base_url, 1_000).await;
    let error = provider
        .transcribe(audio("audio/aac", "turn.aac"))
        .await
        .expect_err("retry must be bounded");
    assert_eq!(error, VoiceProviderError::ProviderUnavailable);
    assert_eq!(state.requests.load(Ordering::SeqCst), 2);
    task.abort();
}

#[tokio::test]
async fn minimax_stt_serializes_requests_with_a_bounded_single_permit() {
    let mut state = FixtureState::with_statuses(vec![StatusCode::OK, StatusCode::OK]);
    state.delay_ms = 50;
    let (base_url, state, task) = fixture(state).await;
    let (provider, _store, _key_ref) = provider(base_url, 2_000).await;
    let first = provider.clone();
    let second = provider.clone();
    let (first, second) = tokio::join!(
        first.transcribe(audio("audio/flac", "one.flac")),
        second.transcribe(audio("audio/flac", "two.flac")),
    );
    assert!(first.is_ok());
    assert!(second.is_ok());
    assert_eq!(state.max_active.load(Ordering::SeqCst), 1);
    task.abort();
}

async fn configured_server(base_url: String) -> (Arc<AppServer>, String, SecretRef) {
    let root = std::env::temp_dir().join(format!(
        "yilian-cycle6-minimax-api-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("server root");
    let token = "v".repeat(64);
    let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let key_ref = SecretRef::new("voice.stt.test");
    store
        .put(&key_ref, secrecy::SecretString::from("SECRET_VALUE"))
        .await
        .expect("server fixture secret");
    let server = Arc::new(
        AppServer::new_with_control_session_and_store(
            &root.join("voice.db"),
            root.to_str().unwrap(),
            ControlSession::new(token.clone()).unwrap(),
            store,
        )
        .expect("test server"),
    );
    server.config.write().voice = VoiceConfig {
        stt: VoiceSttConfig {
            provider: "minimax".to_string(),
            base_url,
            model: "asr-1.0".to_string(),
            language: "zh".to_string(),
            api_key: String::new(),
            api_key_env: String::new(),
            api_key_ref: Some(key_ref.clone()),
            clear_api_key: false,
            timeout_ms: 2_000,
        },
        tts: Default::default(),
    };
    (server, token, key_ref)
}

#[tokio::test]
async fn app_api_maps_content_type_to_an_official_filename_and_keeps_minimax_final_only() {
    let state = FixtureState::with_statuses(vec![StatusCode::OK]);
    let (base_url, state, task) = fixture(state).await;
    let (server, token, _key_ref) = configured_server(base_url).await;
    let session = server
        .voice_runtime
        .start(FocusedSurface::Conversation)
        .expect("voice session");
    let lease = server
        .voice_runtime
        .acquire_lease(
            &session.voice_session_id,
            session.generation,
            VoiceInputOwner::BuiltinAsr,
        )
        .expect("voice lease");

    let response = api::build_router(Arc::clone(&server))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/voice/transcribe")
                .header(CONTROL_SESSION_HEADER, &token)
                .header("content-type", "audio/ogg; codecs=opus")
                .header("x-yilian-voice-session", &session.voice_session_id)
                .header("x-yilian-voice-generation", session.generation.to_string())
                .header("x-yilian-voice-lease", &lease.lease_id)
                .body(Body::from("audio-fixture"))
                .unwrap(),
        )
        .await
        .expect("API response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .expect("API body");
    let payload: Value = serde_json::from_slice(&body).expect("JSON response");
    assert_eq!(payload["transcript"]["text"], "打开记事本");
    assert!(payload["transcript"]["is_final"].as_bool().unwrap());
    let multipart = String::from_utf8(state.body.lock().await.clone()).unwrap();
    assert!(multipart.contains("filename=\"voice.ogg\""));

    // The public MiniMax API has no microphone-duplex partial endpoint.  The
    // partial route must not send the cumulative browser blob again.
    let partial_lease = server
        .voice_runtime
        .acquire_lease(
            &session.voice_session_id,
            session.generation,
            VoiceInputOwner::BuiltinAsr,
        )
        .expect("partial voice lease");
    let response = api::build_router(Arc::clone(&server))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/voice/transcribe/partial")
                .header(CONTROL_SESSION_HEADER, &token)
                .header("content-type", "audio/webm")
                .header("x-yilian-voice-session", &session.voice_session_id)
                .header("x-yilian-voice-generation", session.generation.to_string())
                .header("x-yilian-voice-lease", &partial_lease.lease_id)
                .body(Body::from("cumulative-audio-fixture"))
                .unwrap(),
        )
        .await
        .expect("partial API response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .expect("partial API body");
    let payload: Value = serde_json::from_slice(&body).expect("partial JSON response");
    assert_eq!(payload["partial_supported"], false);
    assert!(payload["transcript"].is_null());
    assert_eq!(state.requests.load(Ordering::SeqCst), 1);

    task.abort();
}

#[tokio::test]
async fn app_server_factory_shares_single_minimax_stt_gate_across_provider_instances() {
    let mut state = FixtureState::with_statuses(vec![StatusCode::OK, StatusCode::OK]);
    state.delay_ms = 50;
    let (base_url, state, task) = fixture(state).await;
    let (server, _token, _key_ref) = configured_server(base_url).await;
    let first = server.stt_provider().expect("MiniMax STT provider");
    let second = server.stt_provider().expect("MiniMax STT provider");
    let (first, second) = tokio::join!(
        first.transcribe(audio("audio/flac", "one.flac")),
        second.transcribe(audio("audio/flac", "two.flac")),
    );
    assert!(first.is_ok());
    assert!(second.is_ok());
    assert_eq!(state.max_active.load(Ordering::SeqCst), 1);
    task.abort();
}

#[tokio::test]
async fn provider_status_reports_minimax_stt_independently_from_tts() {
    let state = FixtureState::with_statuses(vec![StatusCode::OK]);
    let (base_url, _state, task) = fixture(state).await;
    let (server, token, _key_ref) = configured_server(base_url).await;
    let response = api::build_router(server)
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/voice/providers")
                .header(CONTROL_SESSION_HEADER, token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("provider status response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .expect("provider status body");
    let payload: Value = serde_json::from_slice(&body).expect("provider status JSON");
    assert_eq!(payload["stt"]["provider"], "minimax");
    assert_eq!(payload["stt"]["model"], "asr-1.0");
    assert_eq!(payload["stt"]["configured"], true);
    assert_eq!(payload["stt"]["available"], true);
    assert_eq!(payload["tts"]["configured"], false);
    assert_eq!(payload["tts"]["available"], false);
    task.abort();
}
