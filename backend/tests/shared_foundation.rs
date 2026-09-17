use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use futures::StreamExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use yilian_backend::{
    api,
    safety::{ControlSession, CONTROL_SESSION_HEADER},
    shared::event::YiEvent,
    AppServer,
};

struct TempFoundation {
    root: PathBuf,
    database: PathBuf,
    workspace: PathBuf,
}

impl TempFoundation {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("yilian-shared-foundation-{}", uuid::Uuid::new_v4()));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        Self {
            database: root.join("foundation.db"),
            root,
            workspace,
        }
    }
}

impl Drop for TempFoundation {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn request(
    method: Method,
    uri: &str,
    token: &str,
    content_type: &str,
    body: Body,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTROL_SESSION_HEADER, token)
        .header(CONTENT_TYPE, content_type)
        .body(body)
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert!(
        !bytes.is_empty(),
        "expected JSON response body for HTTP {status}"
    );
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn gate_zero_shared_foundation_routes_are_functional_and_honest() {
    let temp = TempFoundation::new();
    let token = "f".repeat(64);
    let server = Arc::new(
        AppServer::new_with_control_session(
            &temp.database,
            temp.workspace.to_str().unwrap(),
            ControlSession::new(token.clone()).unwrap(),
        )
        .unwrap(),
    );
    let app = api::build_router(Arc::clone(&server));

    let events = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/events",
            &token,
            "text/event-stream",
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(events.status(), StatusCode::OK);
    assert_eq!(
        events.headers()[CONTENT_TYPE].to_str().unwrap(),
        "text/event-stream"
    );
    let mut event_stream = events.into_body().into_data_stream();

    let command = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/commands",
            &token,
            "application/json",
            Body::from(
                json!({
                    "command": "core.echo",
                    "request_id": "gate0-command",
                    "source": "gate0",
                    "payload": {"message": "foundation"}
                })
                .to_string(),
            ),
        ))
        .await
        .unwrap();
    assert_eq!(command.status(), StatusCode::OK);
    let command = json_body(command).await;
    assert_eq!(command["status"], "succeeded");
    assert_eq!(command["result"]["message"], "foundation");

    let ingest = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/resources/ingest?name=gate-zero.txt",
            &token,
            "text/plain",
            Body::from("shared foundation"),
        ))
        .await
        .unwrap();
    assert_eq!(ingest.status(), StatusCode::CREATED);
    let resource = json_body(ingest).await;
    let resource_id = resource["id"].as_str().unwrap();
    assert_eq!(resource["name"], "gate-zero.txt");
    assert!(resource["storage_path"]
        .as_str()
        .unwrap()
        .contains("resources"));

    let event_bytes = tokio::time::timeout(Duration::from_secs(2), event_stream.next())
        .await
        .expect("resource event timed out")
        .expect("event stream ended")
        .expect("event body failed");
    let event_text = String::from_utf8(event_bytes.to_vec()).unwrap();
    assert!(event_text.contains("event: resource.created"));
    assert!(event_text.contains(resource_id));

    let resource_query = app
        .clone()
        .oneshot(request(
            Method::GET,
            &format!("/api/resources/{resource_id}"),
            &token,
            "application/json",
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(resource_query.status(), StatusCode::OK);
    assert_eq!(json_body(resource_query).await["id"], resource_id);

    let voice = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/voice/sessions/start",
            &token,
            "application/json",
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(voice.status(), StatusCode::OK);
    assert_eq!(json_body(voice).await["state"], "listening");

    let presence = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/presence",
            &token,
            "application/json",
            Body::empty(),
        ))
        .await
        .unwrap();
    let presence = json_body(presence).await;
    assert_eq!(presence["activity"], "working");
    assert_eq!(presence["interaction"], "listening");

    let tasks = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/projections/tasks?scope=workspace%3Agate0",
            &token,
            "application/json",
            Body::empty(),
        ))
        .await
        .unwrap();
    let tasks = json_body(tasks).await;
    assert!(tasks["tasks"].as_array().unwrap().is_empty());

    server
        .event_hub
        .publish(YiEvent::new(
            "core.gate0.completed",
            "gate0-test",
            json!({"passed": true}),
        ))
        .unwrap();
}
