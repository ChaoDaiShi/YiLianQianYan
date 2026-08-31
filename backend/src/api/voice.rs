use axum::{
    body::Bytes,
    extract::State,
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::server::AppServer;
use crate::shared::voice::VoiceError;

pub async fn presence_handler(State(server): State<Arc<AppServer>>) -> impl IntoResponse {
    Json(server.voice_core.presence())
}

pub async fn session_handler(State(server): State<Arc<AppServer>>) -> impl IntoResponse {
    Json(json!({ "session": server.voice_core.session() }))
}

pub async fn start_handler(State(server): State<Arc<AppServer>>) -> Response {
    voice_json(server.voice_core.start())
}

pub async fn stop_handler(State(server): State<Arc<AppServer>>) -> Response {
    voice_json(server.voice_core.stop())
}

pub async fn interrupt_handler(State(server): State<Arc<AppServer>>) -> Response {
    voice_json(server.voice_core.interrupt())
}

pub async fn cancel_handler(State(server): State<Arc<AppServer>>) -> Response {
    voice_json(server.voice_core.cancel())
}

pub async fn transcribe_handler(State(server): State<Arc<AppServer>>, body: Bytes) -> Response {
    voice_json(server.voice_core.transcribe(&body))
}

#[derive(Deserialize)]
pub struct SpeakRequest {
    text: String,
}

pub async fn speak_handler(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<SpeakRequest>,
) -> Response {
    match server.voice_core.speak(&request.text) {
        Ok(output) => {
            let mut headers = HeaderMap::new();
            if let Ok(value) = output.media_type.parse() {
                headers.insert(CONTENT_TYPE, value);
            }
            headers.insert(
                "x-yilian-voice-provider",
                output
                    .provider
                    .parse()
                    .unwrap_or_else(|_| "unknown".parse().unwrap()),
            );
            (StatusCode::OK, headers, output.audio).into_response()
        }
        Err(error) => voice_error(error),
    }
}

fn voice_json<T: serde::Serialize>(result: Result<T, VoiceError>) -> Response {
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => voice_error(error),
    }
}

fn voice_error(error: VoiceError) -> Response {
    let status = match error {
        VoiceError::NoActiveSession | VoiceError::InvalidState(_) => StatusCode::CONFLICT,
        VoiceError::InvalidInput(_) => StatusCode::BAD_REQUEST,
        VoiceError::Provider(_) => StatusCode::BAD_GATEWAY,
    };
    (
        status,
        Json(json!({
            "error": "voice_operation_failed",
            "message": error.to_string(),
        })),
    )
        .into_response()
}
