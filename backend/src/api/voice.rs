use axum::{
    body::Bytes,
    extract::State,
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use secrecy::ExposeSecret;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::server::AppServer;
use crate::shared::interaction::{ContextAnchorSnapshot, ConversationalAnchor, FocusedSurface};
use crate::shared::voice::{VoiceInputOwner, VoiceTurn};
use crate::voice::{
    AudioInput, MiniMaxSttProvider, VoiceDispatchError, VoiceDispatchOnceError, VoiceProviderError,
    VoiceRuntimeError,
};

pub async fn presence_handler(State(server): State<Arc<AppServer>>) -> impl IntoResponse {
    Json(server.presence())
}

pub async fn providers_handler(State(server): State<Arc<AppServer>>) -> impl IntoResponse {
    let voice = server.config.read().voice.clone();
    let stt_supported = voice.stt.provider.eq_ignore_ascii_case("openai-compatible")
        || voice.stt.provider.eq_ignore_ascii_case("minimax");
    let tts_supported = voice.tts.provider.eq_ignore_ascii_case("openai-compatible")
        || voice.tts.provider.eq_ignore_ascii_case("minimax");
    let stt_credential = server
        .secret_resolver
        .resolve_voice_stt_api_key(&voice.stt)
        .await
        .ok()
        .flatten()
        .filter(|key| !key.expose_secret().trim().is_empty())
        .is_some();
    let tts_credential = server
        .secret_resolver
        .resolve_voice_tts_api_key(&voice.tts)
        .await
        .ok()
        .flatten()
        .filter(|key| !key.expose_secret().trim().is_empty())
        .is_some();
    let stt_structural = if voice.stt.provider.eq_ignore_ascii_case("minimax") {
        MiniMaxSttProvider::from_config(&voice.stt, Arc::clone(&server.secret_resolver))
            .is_structurally_available()
    } else {
        voice.stt.structurally_configured()
    };
    let tts_structural = voice.tts.structurally_configured();
    Json(json!({
        "stt": {
            "provider": voice.stt.provider,
            "model": voice.stt.model,
            "configured": stt_credential,
            "credential_configured": stt_credential,
            "available": stt_supported && stt_structural && stt_credential,
            "unavailable_reason": provider_unavailable_reason(stt_supported, stt_structural, stt_credential),
        },
        "tts": {
            "provider": voice.tts.provider,
            "model": voice.tts.model,
            "voice": voice.tts.voice,
            "configured": tts_credential,
            "credential_configured": tts_credential,
            "available": tts_supported && tts_structural && tts_credential,
            "unavailable_reason": provider_unavailable_reason(tts_supported, tts_structural, tts_credential),
        }
    }))
}

fn provider_unavailable_reason(
    supported: bool,
    structural: bool,
    credential: bool,
) -> Option<&'static str> {
    if !supported {
        Some("unsupported_provider")
    } else if !structural {
        Some("incomplete_configuration")
    } else if !credential {
        Some("credential_unavailable")
    } else {
        None
    }
}

pub async fn session_handler(State(server): State<Arc<AppServer>>) -> impl IntoResponse {
    let snapshot = server.voice_runtime.snapshot();
    Json(json!({ "session": snapshot.session, "snapshot": snapshot }))
}

#[derive(Default, Deserialize)]
struct StartRequest {
    focused_surface: Option<FocusedSurface>,
}

pub async fn start_handler(State(server): State<Arc<AppServer>>, body: Bytes) -> Response {
    let request = match parse_optional_json::<StartRequest>(&body) {
        Ok(request) => request,
        Err(error) => return bad_request("invalid_voice_request", error),
    };
    runtime_json(
        server.voice_runtime.start(
            request
                .and_then(|request| request.focused_surface)
                .unwrap_or(FocusedSurface::Conversation),
        ),
    )
}

#[derive(Deserialize)]
pub struct SessionGenerationRequest {
    session_id: String,
    generation: u64,
}

pub async fn reinitialize_handler(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<SessionGenerationRequest>,
) -> Response {
    runtime_json(
        server
            .voice_runtime
            .reinitialize_input(&request.session_id, request.generation),
    )
}

pub async fn stop_handler(State(server): State<Arc<AppServer>>, body: Bytes) -> Response {
    end_from_body(&server, &body)
}

pub async fn cancel_handler(State(server): State<Arc<AppServer>>, body: Bytes) -> Response {
    end_from_body(&server, &body)
}

fn end_from_body(server: &AppServer, body: &Bytes) -> Response {
    let request = match parse_optional_json::<SessionGenerationRequest>(body) {
        Ok(request) => request,
        Err(error) => return bad_request("invalid_voice_request", error),
    };
    let Some((session_id, generation)) = request
        .map(|request| (request.session_id, request.generation))
        .or_else(|| {
            server
                .voice_runtime
                .session()
                .map(|session| (session.voice_session_id, session.generation))
        })
    else {
        return runtime_json::<crate::shared::voice::GlobalVoiceSession>(Err(
            VoiceRuntimeError::NoActiveSession,
        ));
    };
    runtime_json(server.voice_runtime.end(&session_id, generation))
}

pub async fn interrupt_handler(State(server): State<Arc<AppServer>>, body: Bytes) -> Response {
    let request = match parse_optional_json::<SessionGenerationRequest>(&body) {
        Ok(request) => request,
        Err(error) => return bad_request("invalid_voice_request", error),
    };
    let Some((session_id, generation)) = request
        .map(|request| (request.session_id, request.generation))
        .or_else(|| {
            server
                .voice_runtime
                .session()
                .map(|session| (session.voice_session_id, session.generation))
        })
    else {
        return runtime_json::<crate::shared::voice::GlobalVoiceSession>(Err(
            VoiceRuntimeError::NoActiveSession,
        ));
    };
    runtime_json(server.voice_runtime.interrupt(&session_id, generation))
}

#[derive(Deserialize)]
pub struct LeaseRequest {
    session_id: String,
    generation: u64,
    owner: VoiceInputOwner,
}

pub async fn lease_handler(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<LeaseRequest>,
) -> Response {
    runtime_json(server.voice_runtime.acquire_lease(
        &request.session_id,
        request.generation,
        request.owner,
    ))
}

#[derive(Deserialize)]
pub struct ContextUpdateRequest {
    session_id: String,
    generation: u64,
    focused_surface: FocusedSurface,
    #[serde(default)]
    conversational_anchor: Option<ConversationalAnchor>,
    #[serde(default)]
    active_task: Option<String>,
}

pub async fn context_handler(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<ContextUpdateRequest>,
) -> Response {
    runtime_json(server.voice_runtime.update_context(
        &request.session_id,
        request.generation,
        ContextAnchorSnapshot {
            focused_surface: request.focused_surface,
            conversational_anchor: request.conversational_anchor,
            active_task: request.active_task,
        },
    ))
}

#[derive(Deserialize)]
pub struct PartialTranscriptRequest {
    session_id: String,
    generation: u64,
    lease_id: String,
    text: String,
}

pub async fn partial_handler(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<PartialTranscriptRequest>,
) -> Response {
    runtime_json(server.voice_runtime.update_partial(
        &request.session_id,
        request.generation,
        &request.lease_id,
        request.text,
    ))
}

pub async fn transcribe_handler(
    State(server): State<Arc<AppServer>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(session_id) = header_string(&headers, "x-yilian-voice-session") else {
        return bad_request(
            "missing_voice_headers",
            "voice session, generation and lease headers are required",
        );
    };
    let Some(generation) = header_string(&headers, "x-yilian-voice-generation")
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return bad_request(
            "missing_voice_headers",
            "voice session, generation and lease headers are required",
        );
    };
    let Some(lease_id) = header_string(&headers, "x-yilian-voice-lease") else {
        return bad_request(
            "missing_voice_headers",
            "voice session, generation and lease headers are required",
        );
    };
    let Some(media_type) = header_string(&headers, CONTENT_TYPE.as_str()) else {
        return bad_request("invalid_audio", "audio media type is required");
    };
    if body.is_empty() || body.len() > crate::voice::provider::MAX_AUDIO_BYTES {
        return bad_request("invalid_audio", "audio size is outside the accepted range");
    }
    if let Err(error) = server
        .voice_runtime
        .validate_input(&session_id, generation, &lease_id)
    {
        return runtime_json::<crate::voice::runtime::VoiceRuntimeSnapshot>(Err(error));
    }
    let provider = match server.stt_provider() {
        Ok(provider) => provider,
        Err(error) => return provider_error(error),
    };
    let filename = match transcription_filename(provider.provider_name(), &media_type, false) {
        Ok(filename) => filename,
        Err(error) => return provider_error(error),
    };
    let transcript = match provider
        .transcribe(AudioInput {
            bytes: body.to_vec(),
            media_type,
            filename,
            language: None,
        })
        .await
    {
        Ok(transcript) => transcript,
        Err(error) => return provider_error(error),
    };
    let accepted = match server.voice_runtime.commit_final(
        &session_id,
        generation,
        &lease_id,
        &transcript.text,
    ) {
        Ok(accepted) => accepted,
        Err(error) => {
            return runtime_json::<crate::voice::runtime::AcceptedFinalTranscript>(Err(error))
        }
    };
    (
        StatusCode::OK,
        Json(json!({ "transcript": transcript, "accepted": accepted })),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct TurnDispatchRequest {
    session_id: String,
    generation: u64,
    lease_id: String,
    final_transcript: String,
}

pub async fn dispatch_handler(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<TurnDispatchRequest>,
) -> Response {
    let accepted = match server.voice_runtime.commit_final(
        &request.session_id,
        request.generation,
        &request.lease_id,
        &request.final_transcript,
    ) {
        Ok(accepted) => accepted,
        Err(VoiceRuntimeError::DuplicateFinal) => match server.voice_runtime.accepted_final(
            &request.session_id,
            request.generation,
            &request.lease_id,
            &request.final_transcript,
        ) {
            Ok(accepted) => accepted,
            Err(error) => {
                return runtime_json::<crate::voice::runtime::AcceptedFinalTranscript>(Err(error))
            }
        },
        Err(error) => {
            return runtime_json::<crate::voice::runtime::AcceptedFinalTranscript>(Err(error))
        }
    };
    let Some(hook) = server.voice_dispatch_hook() else {
        return dispatch_error(VoiceDispatchError::Unavailable);
    };
    let outcome = match server.voice_runtime.dispatch_once(accepted, hook.as_ref()) {
        Ok(outcome) => outcome,
        Err(VoiceDispatchOnceError::Runtime(error)) => {
            return runtime_json::<VoiceTurn>(Err(error))
        }
        Err(VoiceDispatchOnceError::Dispatch(error)) => return dispatch_error(error),
    };
    (
        StatusCode::OK,
        Json(json!({
            "turn": outcome.turn,
            "command_result": outcome.command_result,
            "narration": outcome.narration,
            "continuation": outcome.continuation,
        })),
    )
        .into_response()
}

/// Provider-backed interim transcription. This validates the active input
/// lease both before and after the provider call, then updates display-only
/// partial state. It never commits a final transcript or creates a VoiceTurn.
pub async fn transcribe_partial_handler(
    State(server): State<Arc<AppServer>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(session_id) = header_string(&headers, "x-yilian-voice-session") else {
        return bad_request(
            "missing_voice_headers",
            "voice session, generation and lease headers are required",
        );
    };
    let Some(generation) = header_string(&headers, "x-yilian-voice-generation")
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return bad_request(
            "missing_voice_headers",
            "voice session, generation and lease headers are required",
        );
    };
    let Some(lease_id) = header_string(&headers, "x-yilian-voice-lease") else {
        return bad_request(
            "missing_voice_headers",
            "voice session, generation and lease headers are required",
        );
    };
    let Some(media_type) = header_string(&headers, CONTENT_TYPE.as_str()) else {
        return bad_request("invalid_audio", "audio media type is required");
    };
    if body.is_empty() || body.len() > crate::voice::provider::MAX_AUDIO_BYTES {
        return bad_request("invalid_audio", "audio size is outside the accepted range");
    }
    if let Err(error) = server
        .voice_runtime
        .validate_input(&session_id, generation, &lease_id)
    {
        return runtime_json::<crate::voice::runtime::VoiceRuntimeSnapshot>(Err(error));
    }
    let provider = match server.stt_provider() {
        Ok(provider) => provider,
        Err(error) => return provider_error(error),
    };
    // MiniMax's public API supports SSE deltas only after a complete file
    // upload; it is not a microphone stream.  Keep the final-first policy so
    // the browser never resubmits an ever-growing cumulative recording.
    if provider.provider_name().eq_ignore_ascii_case("minimax") {
        return (
            StatusCode::OK,
            Json(json!({
                "transcript": Value::Null,
                "snapshot": server.voice_runtime.snapshot(),
                "partial_supported": false,
            })),
        )
            .into_response();
    }
    let filename = match transcription_filename(provider.provider_name(), &media_type, true) {
        Ok(filename) => filename,
        Err(error) => return provider_error(error),
    };
    let transcript = match provider
        .transcribe(AudioInput {
            bytes: body.to_vec(),
            media_type,
            filename,
            language: None,
        })
        .await
    {
        Ok(transcript) => transcript,
        Err(error) => return provider_error(error),
    };
    let snapshot = match server.voice_runtime.update_partial(
        &session_id,
        generation,
        &lease_id,
        &transcript.text,
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return runtime_json::<crate::voice::runtime::VoiceRuntimeSnapshot>(Err(error))
        }
    };
    (
        StatusCode::OK,
        Json(json!({ "transcript": transcript, "snapshot": snapshot })),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct SpeakRequest {
    pub text: String,
    pub session_id: String,
    pub generation: u64,
    #[serde(default)]
    pub voice: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
}

pub async fn speak_handler(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<SpeakRequest>,
) -> Response {
    let session_id = request.session_id;
    let generation = request.generation;
    if let Err(error) = server
        .voice_runtime
        .validate_generation(&session_id, generation)
    {
        return runtime_json::<crate::shared::voice::GlobalVoiceSession>(Err(error));
    }
    let provider = match server.tts_provider() {
        Ok(provider) => provider,
        Err(error) => return provider_error(error),
    };
    let audio = match provider
        .synthesize(&crate::voice::provider::SpeechRequest {
            text: request.text,
            voice: request.voice.unwrap_or_default(),
            language: request.language,
        })
        .await
    {
        Ok(audio) => audio,
        Err(error) => return provider_error(error),
    };
    let mut response = Response::new(axum::body::Body::from(audio.bytes));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        audio
            .media_type
            .parse()
            .unwrap_or_else(|_| "application/octet-stream".parse().unwrap()),
    );
    response.headers_mut().insert(
        "x-yilian-voice-provider",
        audio
            .provider
            .parse()
            .unwrap_or_else(|_| "unknown".parse().unwrap()),
    );
    response.headers_mut().insert(
        "x-yilian-voice-generation",
        generation
            .to_string()
            .parse()
            .expect("generation is a valid header value"),
    );
    response
}

pub async fn speech_start_handler(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<SessionGenerationRequest>,
) -> Response {
    runtime_json(
        server
            .voice_runtime
            .begin_speaking(&request.session_id, request.generation),
    )
}

pub async fn speech_finished_handler(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<SessionGenerationRequest>,
) -> Response {
    runtime_json(
        server
            .voice_runtime
            .finish_speaking(&request.session_id, request.generation),
    )
}

fn parse_optional_json<T: DeserializeOwned>(body: &Bytes) -> Result<Option<T>, String> {
    if body.is_empty() {
        return Ok(None);
    }
    serde_json::from_slice(body)
        .map(Some)
        .map_err(|_| "voice request JSON is invalid".to_string())
}

fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn runtime_json<T: serde::Serialize>(result: Result<T, VoiceRuntimeError>) -> Response {
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => {
            let status = match error {
                VoiceRuntimeError::InvalidTranscript | VoiceRuntimeError::InvalidTurn => {
                    StatusCode::BAD_REQUEST
                }
                _ => StatusCode::CONFLICT,
            };
            (
                status,
                Json(json!({
                    "error": "voice_operation_failed",
                    "code": error.code(),
                    "message": error.to_string(),
                })),
            )
                .into_response()
        }
    }
}

fn transcription_filename(
    provider: &str,
    media_type: &str,
    partial: bool,
) -> Result<String, VoiceProviderError> {
    let stem = if partial { "voice-partial" } else { "voice" };
    if !provider.eq_ignore_ascii_case("minimax") {
        return Ok(format!("{stem}.webm"));
    }
    let media_type = media_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let extension = match media_type.as_str() {
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/aiff" | "audio/x-aiff" => "aiff",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => "m4a",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/aac" | "audio/x-aac" => "aac",
        "audio/opus" => "opus",
        "audio/ogg" => "ogg",
        _ => return Err(VoiceProviderError::UnsupportedAudio),
    };
    Ok(format!("{stem}.{extension}"))
}

fn provider_error(error: VoiceProviderError) -> Response {
    let (status, code) = match error {
        VoiceProviderError::ProviderUnavailable => {
            (StatusCode::SERVICE_UNAVAILABLE, "provider_unavailable")
        }
        VoiceProviderError::InvalidAudio(_) => (StatusCode::BAD_REQUEST, "invalid_audio"),
        VoiceProviderError::InvalidText => (StatusCode::BAD_REQUEST, "invalid_text"),
        VoiceProviderError::UnsupportedMediaType => {
            (StatusCode::BAD_GATEWAY, "invalid_audio_response")
        }
        VoiceProviderError::Timeout => (StatusCode::GATEWAY_TIMEOUT, "provider_timeout"),
        VoiceProviderError::SttTimeout => (StatusCode::GATEWAY_TIMEOUT, "STT_TIMEOUT"),
        VoiceProviderError::AuthFailed => (StatusCode::UNAUTHORIZED, "AUTH_FAILED"),
        VoiceProviderError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMITED"),
        VoiceProviderError::AudioTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "AUDIO_TOO_LARGE"),
        VoiceProviderError::UnsupportedAudio => (StatusCode::BAD_REQUEST, "UNSUPPORTED_AUDIO"),
        VoiceProviderError::TranscriptionFailed => {
            (StatusCode::BAD_GATEWAY, "TRANSCRIPTION_FAILED")
        }
        VoiceProviderError::RequestFailed(_)
        | VoiceProviderError::ProviderRejected(_)
        | VoiceProviderError::InvalidResponse => {
            (StatusCode::BAD_GATEWAY, "provider_request_failed")
        }
    };
    (
        status,
        Json(json!({
            "error": "voice_provider_failed",
            "code": code,
            "message": error.to_string(),
        })),
    )
        .into_response()
}

fn dispatch_error(error: VoiceDispatchError) -> Response {
    let message = error.to_string();
    let (status, code) = match &error {
        VoiceDispatchError::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "voice_dispatch_unavailable",
        ),
        VoiceDispatchError::Rejected(_) => (StatusCode::CONFLICT, "voice_dispatch_rejected"),
    };
    (
        status,
        Json(json!({
            "error": "voice_dispatch_failed",
            "code": code,
            "message": message,
        })),
    )
        .into_response()
}

fn bad_request(code: &str, message: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": "voice_request_failed",
            "code": code,
            "message": message.into()
        })),
    )
        .into_response()
}
