//! Narrow API tests for generation-safe, final-only voice dispatch.

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

use yilian_backend::{
    api,
    config::types::{VoiceConfig, VoiceSttConfig, VoiceTtsConfig},
    safety::{ControlSession, CONTROL_SESSION_HEADER},
    secret::{InMemorySecretStore, SecretRef, SecretStore, VOICE_KEY_REF},
    shared::{
        interaction::{
            ContextAnchorSnapshot, InteractionIntent, InteractionSource, TargetResolution,
        },
        voice::{VoiceInputLease, VoiceInputOwner, VoiceTurn},
    },
    voice::{VoiceDispatchError, VoiceDispatchHook, VoiceDispatchOutcome, VoiceDispatchRequest},
    AppServer,
};

struct TestVoiceDispatchHook;

struct UnavailableVoiceDispatchHook;

impl VoiceDispatchHook for UnavailableVoiceDispatchHook {
    fn dispatch(
        &self,
        _request: VoiceDispatchRequest,
    ) -> Result<VoiceDispatchOutcome, VoiceDispatchError> {
        Err(VoiceDispatchError::Unavailable)
    }
}

impl VoiceDispatchHook for TestVoiceDispatchHook {
    fn dispatch(
        &self,
        request: VoiceDispatchRequest,
    ) -> Result<VoiceDispatchOutcome, VoiceDispatchError> {
        let lease = VoiceInputLease {
            lease_id: request.accepted.lease_id.clone(),
            voice_session_id: request.accepted.session_id.clone(),
            generation: request.accepted.generation,
            owner: VoiceInputOwner::BuiltinAsr,
            acquired_at: request.accepted.created_at,
        };
        VoiceTurn::new(
            &request.session,
            &lease,
            InteractionSource::Voice,
            request.accepted.text,
            ContextAnchorSnapshot {
                focused_surface: request.session.focused_surface,
                conversational_anchor: request.session.conversational_anchor.clone(),
                active_task: request.session.active_task.clone(),
            },
            TargetResolution::Missing {
                reason: "test hook leaves target resolution to the trusted router".to_string(),
            },
            InteractionIntent::ConversationTurn,
            request.accepted.created_at,
        )
        .map(VoiceDispatchOutcome::turn_only)
        .map_err(|error| VoiceDispatchError::Rejected(error.to_string()))
    }
}

struct TempServer {
    root: PathBuf,
    token: String,
    server: Arc<AppServer>,
}

impl TempServer {
    fn new() -> Self {
        Self::build(true)
    }

    fn without_dispatch_hook() -> Self {
        Self::build(false)
    }

    fn build(with_dispatch_hook: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("yilian-cycle6-voice-api-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let token = "v".repeat(64);
        let server = Arc::new(
            AppServer::new_with_control_session(
                &root.join("voice.db"),
                root.to_str().unwrap(),
                ControlSession::new(token.clone()).unwrap(),
            )
            .unwrap(),
        );
        if with_dispatch_hook {
            server.set_voice_dispatch_hook(Arc::new(TestVoiceDispatchHook));
        } else {
            server.set_voice_dispatch_hook(Arc::new(UnavailableVoiceDispatchHook));
        }
        Self {
            root,
            token,
            server,
        }
    }

    async fn with_voice_provider(base_url: String) -> Self {
        let root =
            std::env::temp_dir().join(format!("yilian-cycle6-voice-api-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let token = "v".repeat(64);
        let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
        let key_ref = SecretRef::new(VOICE_KEY_REF);
        store
            .put(&key_ref, secrecy::SecretString::from("VOICE_TEST_SECRET"))
            .await
            .expect("store fixture voice key");
        let server = Arc::new(
            AppServer::new_with_control_session_and_store(
                &root.join("voice.db"),
                root.to_str().unwrap(),
                ControlSession::new(token.clone()).unwrap(),
                store,
            )
            .unwrap(),
        );
        server.config.write().voice = VoiceConfig {
            stt: VoiceSttConfig {
                provider: "openai-compatible".to_string(),
                base_url: base_url.clone(),
                model: "fixture-stt".to_string(),
                language: "zh".to_string(),
                api_key: String::new(),
                api_key_env: String::new(),
                api_key_ref: Some(key_ref.clone()),
                clear_api_key: false,
                timeout_ms: 1_000,
            },
            tts: VoiceTtsConfig {
                provider: "openai-compatible".to_string(),
                base_url,
                model: "fixture-tts".to_string(),
                voice: "fixture-voice".to_string(),
                language: "zh".to_string(),
                api_key: String::new(),
                api_key_env: String::new(),
                api_key_ref: Some(key_ref),
                clear_api_key: false,
                timeout_ms: 1_000,
            },
        };
        server.set_voice_dispatch_hook(Arc::new(TestVoiceDispatchHook));
        Self {
            root,
            token,
            server,
        }
    }

    fn app(&self) -> axum::Router {
        api::build_router(Arc::clone(&self.server))
    }
}

async fn stt_fixture() -> (String, tokio::task::JoinHandle<()>) {
    async fn transcribe(body: axum::body::Bytes) -> axum::response::Response {
        assert!(!body.is_empty(), "provider receives multipart audio");
        (StatusCode::OK, axum::Json(json!({"text": "现在做到哪了"}))).into_response()
    }

    async fn synthesize() -> axum::response::Response {
        (
            StatusCode::OK,
            [(CONTENT_TYPE, "audio/mpeg")],
            Body::from("provider-audio-bytes"),
        )
            .into_response()
    }

    let app = Router::new()
        .route("/v1/audio/transcriptions", post(transcribe))
        .route("/v1/audio/speech", post(synthesize));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("fixture listener");
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("fixture server");
    });
    (base_url, task)
}

impl Drop for TempServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn json_request(token: &str, method: Method, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTROL_SESSION_HEADER, token)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

async fn json_response(response: axum::response::Response) -> (StatusCode, Value) {
    let status = response.status();
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({"raw": body.len()}));
    (status, value)
}

async fn start_and_lease(temp: &TempServer) -> (String, u64, String) {
    let response = temp
        .app()
        .oneshot(json_request(
            &temp.token,
            Method::POST,
            "/api/voice/sessions/start",
            json!({"focused_surface": "conversation"}),
        ))
        .await
        .unwrap();
    let (status, session) = json_response(response).await;
    assert_eq!(status, StatusCode::OK);
    let session_id = session["voice_session_id"].as_str().unwrap().to_string();
    let generation = session["generation"].as_u64().unwrap();
    let response = temp
        .app()
        .oneshot(json_request(
            &temp.token,
            Method::POST,
            "/api/voice/leases",
            json!({
                "session_id": session_id,
                "generation": generation,
                "owner": "builtin_asr"
            }),
        ))
        .await
        .unwrap();
    let (status, lease) = json_response(response).await;
    assert_eq!(status, StatusCode::OK);
    (
        session_id,
        generation,
        lease["lease_id"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn partial_transcript_never_dispatches_and_current_final_commits_once() {
    let temp = TempServer::new();
    let mut events = temp.server.event_hub.subscribe();
    let (session_id, generation, lease_id) = start_and_lease(&temp).await;

    let response = temp
        .app()
        .oneshot(json_request(
            &temp.token,
            Method::POST,
            "/api/voice/transcript/partial",
            json!({
                "session_id": session_id,
                "generation": generation,
                "lease_id": lease_id,
                "text": "暂停"
            }),
        ))
        .await
        .unwrap();
    let (status, partial) = json_response(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(partial["partial_transcript"], "暂停");
    assert!(partial["final_transcript"].is_null());

    let dispatch = json!({
        "session_id": session_id,
        "generation": generation,
        "lease_id": lease_id,
        "final_transcript": "暂停这个任务",
        "focused_surface": "task_canvas",
        "resolved_target": {"status": "resolved", "target": {"kind": "task", "graph_id": "task-1"}},
        "intent": {"kind": "command", "name": "task.pause"}
    });
    let response = temp
        .app()
        .oneshot(json_request(
            &temp.token,
            Method::POST,
            "/api/voice/turns/dispatch",
            dispatch.clone(),
        ))
        .await
        .unwrap();
    let (status, accepted) = json_response(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accepted["turn"]["final_transcript"], "暂停这个任务");
    assert_eq!(accepted["turn"]["focused_surface"], "conversation");
    assert_eq!(accepted["turn"]["intent"]["kind"], "conversation_turn");
    assert_eq!(accepted["turn"]["resolved_target"]["status"], "missing");

    let response = temp
        .app()
        .oneshot(json_request(
            &temp.token,
            Method::POST,
            "/api/voice/turns/dispatch",
            dispatch,
        ))
        .await
        .unwrap();
    let (status, duplicate) = json_response(response).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(duplicate["code"], "duplicate_final");

    let snapshot = temp.server.voice_runtime.snapshot();
    assert_eq!(snapshot.turns.len(), 1);
    assert_eq!(snapshot.final_transcript.as_deref(), Some("暂停这个任务"));
    while let Ok(event) = events.try_recv() {
        assert!(event.payload.get("audio").is_none());
        assert!(!event.event_type.contains("dispatch"));
    }
}

#[tokio::test]
async fn dispatch_without_trusted_router_is_unavailable_and_does_not_accept_client_routing() {
    let temp = TempServer::without_dispatch_hook();
    let (session_id, generation, lease_id) = start_and_lease(&temp).await;
    let response = temp
        .app()
        .oneshot(json_request(
            &temp.token,
            Method::POST,
            "/api/voice/turns/dispatch",
            json!({
                "session_id": session_id,
                "generation": generation,
                "lease_id": lease_id,
                "final_transcript": "打开一个任务",
                "focused_surface": "task_canvas",
                "resolved_target": {"status": "resolved", "target": {"kind": "task", "graph_id": "spoofed"}},
                "intent": {"kind": "command", "name": "task.pause"}
            }),
        ))
        .await
        .unwrap();
    let (status, error) = json_response(response).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error["code"], "voice_dispatch_unavailable");
    assert_eq!(temp.server.voice_runtime.snapshot().turns.len(), 0);
    assert_eq!(
        temp.server
            .voice_runtime
            .snapshot()
            .final_transcript
            .as_deref(),
        Some("打开一个任务")
    );
}

#[tokio::test]
async fn stale_generation_drops_old_turn_and_old_speech_cannot_enter_speaking() {
    let temp = TempServer::new();
    let (session_id, generation, lease_id) = start_and_lease(&temp).await;
    let response = temp
        .app()
        .oneshot(json_request(
            &temp.token,
            Method::POST,
            "/api/voice/sessions/reinitialize",
            json!({"session_id": session_id, "generation": generation}),
        ))
        .await
        .unwrap();
    let (status, current) = json_response(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current["generation"], generation + 1);

    let response = temp
        .app()
        .oneshot(json_request(
            &temp.token,
            Method::POST,
            "/api/voice/turns/dispatch",
            json!({
                "session_id": session_id,
                "generation": generation,
                "lease_id": lease_id,
                "final_transcript": "迟到文本",
                "focused_surface": "conversation",
                "resolved_target": {"status": "missing", "reason": "stale"},
                "intent": {"kind": "conversation_turn"}
            }),
        ))
        .await
        .unwrap();
    let (status, stale) = json_response(response).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(stale["code"], "stale_generation");

    let error = temp
        .server
        .voice_runtime
        .begin_speaking(&session_id, generation)
        .expect_err("old generation must not transition to speaking");
    assert_eq!(error.code(), "stale_generation");
    assert_eq!(
        temp.server.voice_runtime.snapshot().session.unwrap().state,
        yilian_backend::shared::voice::VoiceSessionState::Listening
    );
}

#[tokio::test]
async fn transcribe_requires_generation_and_lease_headers_and_does_not_store_audio() {
    let temp = TempServer::new();
    let (session_id, generation, lease_id) = start_and_lease(&temp).await;
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/voice/transcribe")
        .header(CONTROL_SESSION_HEADER, &temp.token)
        .header(CONTENT_TYPE, "audio/webm")
        .body(Body::from("audio-bytes"))
        .unwrap();
    let response = temp.app().oneshot(request).await.unwrap();
    let (status, error) = json_response(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["code"], "missing_voice_headers");

    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/voice/transcribe")
        .header(CONTROL_SESSION_HEADER, &temp.token)
        .header(CONTENT_TYPE, "audio/webm")
        .header("x-yilian-voice-session", session_id)
        .header("x-yilian-voice-generation", generation.to_string())
        .header("x-yilian-voice-lease", lease_id)
        .body(Body::from("audio-bytes"))
        .unwrap();
    let response = temp.app().oneshot(request).await.unwrap();
    let (status, error) = json_response(response).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error["code"], "provider_unavailable");
    assert_eq!(temp.server.voice_runtime.snapshot().final_transcript, None);
}

#[tokio::test]
async fn provider_partial_transcript_updates_only_partial_and_keeps_the_lease() {
    let (base_url, fixture) = stt_fixture().await;
    let temp = TempServer::with_voice_provider(base_url).await;
    let (session_id, generation, lease_id) = start_and_lease(&temp).await;
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/voice/transcribe/partial")
        .header(CONTROL_SESSION_HEADER, &temp.token)
        .header(CONTENT_TYPE, "audio/webm")
        .header("x-yilian-voice-session", &session_id)
        .header("x-yilian-voice-generation", generation.to_string())
        .header("x-yilian-voice-lease", &lease_id)
        .body(Body::from("partial-audio-bytes"))
        .unwrap();

    let response = temp.app().oneshot(request).await.unwrap();
    let (status, partial) = json_response(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(partial["transcript"]["text"], "现在做到哪了");
    assert_eq!(partial["snapshot"]["partial_transcript"], "现在做到哪了");
    assert!(partial["snapshot"]["final_transcript"].is_null());
    assert_eq!(partial["snapshot"]["turns"], json!([]));
    temp.server
        .voice_runtime
        .validate_input(&session_id, generation, &lease_id)
        .expect("partial STT must not consume the input lease");
    fixture.abort();
}

#[tokio::test]
async fn synthesis_waits_for_explicit_browser_playback_lifecycle() {
    let (base_url, fixture) = stt_fixture().await;
    let temp = TempServer::with_voice_provider(base_url).await;
    let (session_id, generation, _lease_id) = start_and_lease(&temp).await;
    let response = temp
        .app()
        .oneshot(json_request(
            &temp.token,
            Method::POST,
            "/api/voice/speak",
            json!({
                "text": "继续完成真实播放验证",
                "session_id": session_id,
                "generation": generation,
                "voice": "fixture-voice",
                "language": "zh"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_ne!(
        temp.server.voice_runtime.session().unwrap().state,
        yilian_backend::shared::voice::VoiceSessionState::Speaking,
        "synthesis alone cannot claim browser playback"
    );

    let response = temp
        .app()
        .oneshot(json_request(
            &temp.token,
            Method::POST,
            "/api/voice/speech/start",
            json!({"session_id": session_id, "generation": generation}),
        ))
        .await
        .unwrap();
    let (status, started) = json_response(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(started["state"], "speaking");

    let response = temp
        .app()
        .oneshot(json_request(
            &temp.token,
            Method::POST,
            "/api/voice/speech/finished",
            json!({"session_id": session_id, "generation": generation}),
        ))
        .await
        .unwrap();
    let (status, finished) = json_response(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(finished["state"], "listening");
    fixture.abort();
}

#[tokio::test]
async fn dispatch_can_follow_a_provider_final_commit_but_remains_single_use() {
    let temp = TempServer::new();
    let (session_id, generation, lease_id) = start_and_lease(&temp).await;
    temp.server
        .voice_runtime
        .commit_final(&session_id, generation, &lease_id, "继续这个任务")
        .expect("provider final is committed before routing");

    let dispatch = json!({
        "session_id": session_id,
        "generation": generation,
        "lease_id": lease_id,
        "final_transcript": "继续这个任务",
        "focused_surface": "conversation",
        "resolved_target": {"status": "resolved", "target": {"kind": "conversation", "conversation_id": "conversation-1"}},
        "intent": {"kind": "conversation_turn"}
    });
    let response = temp
        .app()
        .oneshot(json_request(
            &temp.token,
            Method::POST,
            "/api/voice/turns/dispatch",
            dispatch.clone(),
        ))
        .await
        .unwrap();
    let (status, accepted) = json_response(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accepted["turn"]["final_transcript"], "继续这个任务");

    let response = temp
        .app()
        .oneshot(json_request(
            &temp.token,
            Method::POST,
            "/api/voice/turns/dispatch",
            dispatch,
        ))
        .await
        .unwrap();
    let (status, duplicate) = json_response(response).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(duplicate["code"], "duplicate_final");
}
