use std::{path::PathBuf, sync::Arc};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

use crate::{
    safety::{ControlSession, CONTROL_SESSION_HEADER},
    server::AppServer,
};

use super::build_router;

struct TempDatabase(PathBuf);

impl TempDatabase {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("yilian-api-auth-{}.db", uuid::Uuid::new_v4())))
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn test_server() -> (TempDatabase, Arc<AppServer>, String) {
    let temp = TempDatabase::new();
    let token = "a".repeat(64);
    let server = AppServer::new_with_control_session(
        &temp.0,
        ".",
        ControlSession::new(token.clone()).unwrap(),
    )
    .unwrap();
    (temp, Arc::new(server), token)
}

#[tokio::test]
async fn health_is_public_but_tools_require_the_exact_control_session() {
    let (_temp, server, token) = test_server();
    let app = build_router(server);

    let health = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);

    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tools")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    let wrong = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tools")
                .header(CONTROL_SESSION_HEADER, "b".repeat(64))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    let accepted = app
        .oneshot(
            Request::builder()
                .uri("/api/tools")
                .header(CONTROL_SESSION_HEADER, token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
}
