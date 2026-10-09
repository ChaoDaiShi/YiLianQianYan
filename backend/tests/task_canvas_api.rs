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
        let root =
            std::env::temp_dir().join(format!("yilian-cycle3-canvas-api-{}", uuid::Uuid::new_v4()));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        Self {
            database: root.join("runtime.db"),
            root,
            workspace,
        }
    }
}

impl Drop for TempRuntime {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn request(method: Method, uri: &str, token: &str, body: Body) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTROL_SESSION_HEADER, token)
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}

#[tokio::test]
async fn protected_detail_and_canvas_view_routes_keep_revisions_independent() {
    let temp = TempRuntime::new();
    let token = "c".repeat(64);
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
                .uri("/api/task-world/graphs/api-canvas/detail")
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
            Body::from(
                json!({
                    "id": "api-canvas",
                    "nodes": [{
                        "id": "root",
                        "kind": "work",
                        "title": "Root",
                        "input": {
                            "executor_ref": "workflow://review",
                            "instruction": "Review the local task",
                            "acceptance_criteria": ["A concise result exists"],
                            "resources": [{"id": "resource-1", "name": "Brief"}],
                            "validation": {"status": "not_checked", "issues": []},
                            "private_payload": "must not cross projection"
                        },
                        "retry_policy": {"max_attempts": 1}
                    }],
                    "edges": []
                })
                .to_string(),
            ),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(json_body(created).await["graph"]["revision"], 1);

    let detail = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/task-world/graphs/api-canvas/detail",
            &token,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let detail = json_body(detail).await;
    assert_eq!(detail["detail"]["graph"]["revision"], 1);
    assert_eq!(
        detail["detail"]["nodes"][0]["executor_ref"],
        "workflow://review"
    );
    assert_eq!(
        detail["detail"]["nodes"][0]["acceptance_criteria"][0],
        "A concise result exists"
    );
    assert!(detail["detail"]["nodes"][0]
        .get("private_payload")
        .is_none());
    assert_eq!(detail["detail"]["revisions"][0]["change"], "created");

    let view = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/task-world/graphs/api-canvas/canvas-view",
            &token,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(view.status(), StatusCode::OK);
    let view = json_body(view).await;
    assert_eq!(view["view"]["view_revision"], 1);
    assert_eq!(view["view"]["graph_revision_seen"], 1);

    let updated_view = app
        .clone()
        .oneshot(request(
            Method::PUT,
            "/api/task-world/graphs/api-canvas/canvas-view",
            &token,
            Body::from(
                json!({
                    "expected_view_revision": 1,
                    "view_revision": 1,
                    "graph_revision_seen": 1,
                    "viewport": {"x": 40.0, "y": 20.0, "zoom": 1.2},
                    "node_layouts": [{"node_id": "root", "x": 901.0, "y": 88.0}],
                    "selection": ["root"]
                })
                .to_string(),
            ),
        ))
        .await
        .unwrap();
    assert_eq!(updated_view.status(), StatusCode::OK);
    let updated_view = json_body(updated_view).await;
    assert_eq!(updated_view["view"]["view_revision"], 2);
    assert_eq!(updated_view["view"]["node_layouts"][0]["x"], 901.0);

    let graph = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/task-world/graphs/api-canvas",
            &token,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(json_body(graph).await["graph"]["revision"], 1);

    let started = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/task-world/graphs/api-canvas/nodes/root/start",
            &token,
            Body::from(json!({"expected_revision": 1}).to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(started.status(), StatusCode::OK);

    let checkpoint = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/task-world/graphs/api-canvas/checkpoint",
            &token,
            Body::from(json!({"expected_revision": 1}).to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(checkpoint.status(), StatusCode::CREATED);
    let checkpoint = json_body(checkpoint).await;
    let checkpoint_id = checkpoint["checkpoint"]["id"].as_str().unwrap();

    let added = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/task-world/graphs/api-canvas/nodes",
            &token,
            Body::from(
                json!({
                    "expected_revision": 1,
                    "node": {
                        "id": "child",
                        "kind": "work",
                        "title": "Child",
                        "input": {},
                        "retry_policy": {"max_attempts": 1}
                    }
                })
                .to_string(),
            ),
        ))
        .await
        .unwrap();
    assert_eq!(added.status(), StatusCode::OK);
    assert_eq!(json_body(added).await["graph"]["revision"], 2);

    let detail = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/task-world/graphs/api-canvas/detail",
            &token,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let detail = json_body(detail).await;
    assert_eq!(detail["detail"]["revision"], 2);
    assert_eq!(detail["detail"]["nodes"][0]["status"], "running");
    assert_eq!(detail["detail"]["revisions"].as_array().unwrap().len(), 2);
    assert_eq!(detail["detail"]["revisions"][1]["change"], "node_added");
    assert_eq!(detail["detail"]["checkpoints"].as_array().unwrap().len(), 1);
    assert_eq!(
        detail["detail"]["checkpoints"][0]["checkpoint_id"],
        checkpoint_id
    );
    assert_eq!(detail["detail"]["checkpoints"][0]["graph_revision"], 1);

    let reconciled_view = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/task-world/graphs/api-canvas/canvas-view",
            &token,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(reconciled_view.status(), StatusCode::OK);
    let reconciled_view = json_body(reconciled_view).await;
    assert_eq!(reconciled_view["view"]["view_revision"], 2);
    assert_eq!(reconciled_view["view"]["graph_revision_seen"], 2);
    assert_eq!(
        reconciled_view["view"]["node_layouts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let next_view = app
        .clone()
        .oneshot(request(
            Method::PUT,
            "/api/task-world/graphs/api-canvas/canvas-view",
            &token,
            Body::from(
                json!({
                    "expected_view_revision": 2,
                    "graph_revision_seen": 2,
                    "viewport": {"x": 40.0, "y": 20.0, "zoom": 1.2},
                    "node_layouts": [{"node_id": "root", "x": 902.0, "y": 88.0}],
                    "selection": ["root"]
                })
                .to_string(),
            ),
        ))
        .await
        .unwrap();
    assert_eq!(next_view.status(), StatusCode::OK);
    assert_eq!(json_body(next_view).await["view"]["view_revision"], 3);

    let stale = app
        .oneshot(request(
            Method::PUT,
            "/api/task-world/graphs/api-canvas/canvas-view",
            &token,
            Body::from(
                json!({
                    "expected_view_revision": 1,
                    "view_revision": 1,
                    "graph_revision_seen": 1,
                    "viewport": {"x": 0.0, "y": 0.0, "zoom": 1.0},
                    "node_layouts": [{"node_id": "root", "x": 10.0, "y": 10.0}],
                    "selection": []
                })
                .to_string(),
            ),
        ))
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(json_body(stale).await["error"], "stale_view_revision");
}
