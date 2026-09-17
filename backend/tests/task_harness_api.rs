use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use yilian_backend::{
    api,
    safety::{ControlSession, CONTROL_SESSION_HEADER},
    AppServer,
};

struct TempRuntime {
    root: PathBuf,
    database: PathBuf,
    workspace: PathBuf,
}

impl TempRuntime {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yilian-cycle5-task-harness-api-{}",
            uuid::Uuid::new_v4()
        ));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        Self {
            database: root.join("runtime.db"),
            workspace,
            root,
        }
    }
}

impl Drop for TempRuntime {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn request(method: Method, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTROL_SESSION_HEADER, token)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}

#[tokio::test]
async fn protected_execution_routes_preserve_attempt_history_and_partial_rerun() {
    let temp = TempRuntime::new();
    let token = "h".repeat(64);
    let server = Arc::new(
        AppServer::new_with_control_session(
            &temp.database,
            temp.workspace.to_str().unwrap(),
            ControlSession::new(token.clone()).unwrap(),
        )
        .unwrap(),
    );
    let app = api::build_router(server);

    let unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/task-world/graphs/harness-api/nodes/node/executions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let created = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/task-world/graphs",
            &token,
            json!({
                "id": "harness-api",
                "nodes": [{
                    "id": "node",
                    "kind": "work",
                    "title": "Harness node",
                    "input": {},
                    "retry_policy": {"max_attempts": 1}
                }],
                "edges": []
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);

    let started = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/task-world/graphs/harness-api/nodes/node/executions",
            &token,
            json!({"expected_revision": 1}),
        ))
        .await
        .unwrap();
    assert_eq!(started.status(), StatusCode::CREATED);
    let started = json_body(started).await;
    assert_eq!(started["execution"]["attempt"], 1);
    assert_eq!(started["execution"]["status"], "dispatching");
    let execution_id = started["execution"]["execution_id"]
        .as_str()
        .unwrap()
        .to_string();

    let history = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/task-world/graphs/harness-api/nodes/node/executions",
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(history.status(), StatusCode::OK);
    assert_eq!(
        json_body(history).await["executions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let cancelled = app
        .clone()
        .oneshot(request(
            Method::POST,
            &format!("/api/task-world/graphs/harness-api/executions/{execution_id}/cancel"),
            &token,
            json!({"expected_revision": 1}),
        ))
        .await
        .unwrap();
    assert_eq!(cancelled.status(), StatusCode::OK);
    assert_eq!(
        json_body(cancelled).await["execution"]["status"],
        "cancelled"
    );

    let rerun = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/task-world/graphs/harness-api/rerun",
            &token,
            json!({"expected_revision": 1, "node_id": "node"}),
        ))
        .await
        .unwrap();
    assert_eq!(rerun.status(), StatusCode::OK);
    assert_eq!(json_body(rerun).await["affected_nodes"], json!(["node"]));
}
