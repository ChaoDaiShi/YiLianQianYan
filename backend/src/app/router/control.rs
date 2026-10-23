use crate::{app::state::AppServer, safety::CONTROL_SESSION_HEADER};
use axum::{
    extract::{Request, State},
    http::{header::CONTENT_TYPE, HeaderName, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};
pub(super) async fn require_control_session(
    State(server): State<Arc<AppServer>>,
    request: Request,
    next: Next,
) -> Response {
    let candidate = request
        .headers()
        .get(CONTROL_SESSION_HEADER)
        .and_then(|value| value.to_str().ok());

    if server.control_session.verify(candidate).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "invalid_control_session",
                "message": "有效的本地控制会话凭据是必需的"
            })),
        )
            .into_response();
    }

    next.run(request).await
}

pub(super) fn control_plane_cors() -> CorsLayer {
    let mut origins = [
        "http://tauri.localhost",
        "tauri://localhost",
        "http://localhost:1420",
        "http://127.0.0.1:1420",
    ]
    .into_iter()
    .filter_map(|origin| HeaderValue::from_str(origin).ok())
    .collect::<Vec<_>>();

    if let Ok(configured) = std::env::var("YILIAN_ALLOWED_ORIGINS") {
        origins.extend(
            configured
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .filter_map(|origin| HeaderValue::from_str(origin).ok()),
        );
    }
    origins.sort_unstable_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    origins.dedup();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            CONTENT_TYPE,
            HeaderName::from_static(CONTROL_SESSION_HEADER),
            HeaderName::from_static("x-yilian-voice-session"),
            HeaderName::from_static("x-yilian-voice-generation"),
            HeaderName::from_static("x-yilian-voice-lease"),
        ])
}
