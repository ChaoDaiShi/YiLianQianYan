use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
use tower::ServiceExt;
use yilian_backend::{app::router::build_router, safety::ControlSession, AppServer};

fn server() -> Arc<AppServer> {
    let workspace =
        std::env::temp_dir().join(format!("yilian-v11-conflict-http-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&workspace).unwrap();
    Arc::new(
        AppServer::new_with_control_session(
            Path::new(":memory:"),
            workspace.to_str().unwrap(),
            ControlSession::new("r".repeat(64)).unwrap(),
        )
        .unwrap(),
    )
}

async fn post_resolution(server: &Arc<AppServer>, payload: Value) -> (u16, Value) {
    let response = build_router(server.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cloud/sync/conflicts/conflict-missing/resolve")
                .header("X-Yilian-Control-Session", "r".repeat(64))
                .header("Content-Type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn three_way_choice_requires_versions_request_id_and_no_legacy_field() {
    let server = server();
    let invalid_payloads = [
        json!({"choice":"keep_local_as_copy","expected_remote_revision":12,"request_id":"choice-1"}),
        json!({"choice":"keep_local_as_copy","expected_conflict_version":4,"request_id":"choice-1"}),
        json!({"choice":"keep_local_as_copy","expected_conflict_version":4,"expected_remote_revision":12}),
        json!({"choice":"keep_local_as_copy","expected_conflict_version":4,"expected_remote_revision":12,"request_id":" "}),
        json!({"choice":"keep_local_as_copy","expected_conflict_version":4,"expected_remote_revision":12,"request_id":"choice-1","resolution":"local"}),
        json!({"expected_conflict_version":4,"expected_remote_revision":12,"request_id":"choice-1"}),
    ];

    for payload in invalid_payloads {
        let (status, body) = post_resolution(&server, payload).await;
        assert_eq!(status, 400, "{body}");
        assert_eq!(body["code"], "invalid_conflict_request", "{body}");
    }

    assert!(server.db.list_conflict_records().unwrap().is_empty());
}
