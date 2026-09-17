use std::{path::PathBuf, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{
        header::{
            ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_REQUEST_HEADERS,
            ACCESS_CONTROL_REQUEST_METHOD, CONTENT_TYPE, ORIGIN,
        },
        Method, Request, StatusCode,
    },
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::{
    db::SecurityAuditQuery,
    safety::{AuditEventInput, AuditEventType, ControlSession, CONTROL_SESSION_HEADER},
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
    let health_body: Value =
        serde_json::from_slice(&to_bytes(health.into_body(), 64 * 1024).await.unwrap()).unwrap();
    assert_eq!(health_body["status"], "healthy");
    assert_eq!(health_body["service"], "yilian-backend");
    assert_eq!(health_body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(health_body["database"], "healthy");
    assert_eq!(health_body["policy_version"], crate::safety::POLICY_VERSION);

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

#[tokio::test]
async fn voice_transcribe_preflight_allows_runtime_headers() {
    let (_temp, server, _token) = test_server();
    let response = build_router(server)
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/voice/transcribe")
                .header(ORIGIN, "http://localhost:1420")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "POST")
                .header(
                    ACCESS_CONTROL_REQUEST_HEADERS,
                    "content-type,x-yilian-control-session,x-yilian-voice-session,x-yilian-voice-generation,x-yilian-voice-lease",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let allowed = response
        .headers()
        .get(ACCESS_CONTROL_ALLOW_HEADERS)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    for header in [
        "content-type",
        "x-yilian-control-session",
        "x-yilian-voice-session",
        "x-yilian-voice-generation",
        "x-yilian-voice-lease",
    ] {
        assert!(
            allowed.contains(header),
            "CORS did not allow {header}: {allowed}"
        );
    }
}

#[tokio::test]
async fn task_world_routes_use_live_projection_and_reject_stale_revision() {
    let (_temp, server, token) = test_server();
    let app = build_router(server);
    let graph = json!({
        "id": "api-task-graph",
        "nodes": [{
            "id": "root",
            "kind": "work",
            "title": "Root",
            "input": {},
            "retry_policy": { "max_attempts": 1 }
        }],
        "edges": []
    });

    let created = app
        .clone()
        .oneshot(auth_request(
            Method::POST,
            "/api/task-world/graphs",
            &token,
            Body::from(graph.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = json_body(created).await;
    assert_eq!(created["graph"]["revision"], 1);

    let projection = app
        .clone()
        .oneshot(auth_request(
            Method::GET,
            "/api/projections/tasks?scope=task-world&max_items=1",
            &token,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(projection.status(), StatusCode::OK);
    let projection = json_body(projection).await;
    assert_eq!(projection["tasks"][0]["id"], "api-task-graph");
    assert_eq!(projection["tasks"][0]["status"], "runnable");
    assert_eq!(projection["tasks"][0]["simulation"]["simulated"], false);
    assert!(projection["tasks"][0]["simulation"]["reason"]
        .as_str()
        .unwrap()
        .contains("no workflow or tool execution"));

    let started = app
        .clone()
        .oneshot(auth_request(
            Method::POST,
            "/api/task-world/graphs/api-task-graph/nodes/root/start",
            &token,
            Body::from(json!({ "expected_revision": 1 }).to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(started.status(), StatusCode::OK);

    let projection = app
        .clone()
        .oneshot(auth_request(
            Method::GET,
            "/api/projections/tasks?scope=task-world",
            &token,
            Body::empty(),
        ))
        .await
        .unwrap();
    let projection = json_body(projection).await;
    assert_eq!(projection["tasks"][0]["status"], "working");

    let presence = app
        .clone()
        .oneshot(auth_request(
            Method::GET,
            "/api/presence",
            &token,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(json_body(presence).await["activity"], "working");

    let add_node = json!({
        "expected_revision": 1,
        "node": {
            "id": "child",
            "kind": "work",
            "title": "Child",
            "input": {},
            "retry_policy": { "max_attempts": 1 }
        }
    });
    let updated = app
        .clone()
        .oneshot(auth_request(
            Method::POST,
            "/api/task-world/graphs/api-task-graph/nodes",
            &token,
            Body::from(add_node.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(json_body(updated).await["graph"]["revision"], 2);

    let stale = app
        .clone()
        .oneshot(auth_request(
            Method::POST,
            "/api/task-world/graphs/api-task-graph/nodes",
            &token,
            Body::from(
                json!({
                    "expected_revision": 1,
                    "node": {
                        "id": "other",
                        "kind": "work",
                        "title": "Other",
                        "input": {},
                        "retry_policy": { "max_attempts": 1 }
                    }
                })
                .to_string(),
            ),
        ))
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(json_body(stale).await["error"], "stale_revision");

    let missing_session = app
        .oneshot(
            Request::builder()
                .uri("/api/task-world/graphs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_session.status(), StatusCode::UNAUTHORIZED);
}

fn auth_request(method: Method, uri: &str, token: &str, body: Body) -> Request<Body> {
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

fn seed_policy_event(server: &AppServer, request_id: &str, created_for: &str) {
    server
        .audit_recorder
        .record(AuditEventInput {
            event_type: AuditEventType::PolicyDecided,
            correlation_id: "corr-api".to_string(),
            request_id: request_id.to_string(),
            subject_id: "local-user".to_string(),
            role_key: "owner".to_string(),
            conversation_id: Some(created_for.to_string()),
            tool_name: Some("write_file".to_string()),
            decision_status: Some("allow".to_string()),
            risk_level: Some("medium".to_string()),
            ..Default::default()
        })
        .unwrap();
}

#[tokio::test]
async fn missing_mcp_mutations_return_not_found() {
    let (_temp, server, token) = test_server();
    let app = build_router(server);

    let update = app
        .clone()
        .oneshot(auth_request(
            Method::PUT,
            "/api/plugins/mcp/missing",
            &token,
            Body::from(json!({"name": "Missing"}).to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(update.status(), StatusCode::NOT_FOUND);

    let delete = app
        .oneshot(auth_request(
            Method::DELETE,
            "/api/plugins/mcp/missing",
            &token,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn security_audit_endpoints_filter_validate_export_and_never_delete() {
    let (_temp, server, token) = test_server();
    seed_policy_event(&server, "req-api-1", "conv-api-1");
    seed_policy_event(&server, "req-api-2", "conv-api-2");
    let app = build_router(server.clone());

    let list = app
        .clone()
        .oneshot(auth_request(
            Method::GET,
            "/api/security/audit?event_type=policy_decided&limit=1",
            &token,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list_body: Value =
        serde_json::from_slice(&to_bytes(list.into_body(), 1024 * 1024).await.unwrap()).unwrap();
    assert_eq!(list_body["total"], 1);
    assert_eq!(list_body["events"][0]["event_type"], "policy_decided");

    for invalid_uri in [
        "/api/security/audit?limit=0",
        "/api/security/audit?limit=501",
        "/api/security/audit?start_at=20&end_at=10",
        "/api/security/audit?offset=1000001",
    ] {
        let response = app
            .clone()
            .oneshot(auth_request(
                Method::GET,
                invalid_uri,
                &token,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{invalid_uri}");
    }

    let health = app
        .clone()
        .oneshot(auth_request(
            Method::GET,
            "/api/security/health",
            &token,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    let health_body: Value =
        serde_json::from_slice(&to_bytes(health.into_body(), 1024 * 1024).await.unwrap()).unwrap();
    assert_eq!(health_body["audit"], "healthy");
    assert_eq!(health_body["policy_version"], crate::safety::POLICY_VERSION);

    let export = app
        .clone()
        .oneshot(auth_request(
            Method::POST,
            "/api/security/audit/export",
            &token,
            Body::from(json!({"limit": 50}).to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(export.status(), StatusCode::OK);
    let export_body: Value =
        serde_json::from_slice(&to_bytes(export.into_body(), 1024 * 1024).await.unwrap()).unwrap();
    assert_eq!(export_body["schema_version"], "security-audit-export-v1");
    assert!(export_body["events"].as_array().unwrap().len() >= 3);

    let export_events = server
        .audit_recorder
        .query(&SecurityAuditQuery {
            event_type: Some("audit_exported".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(export_events.len(), 1);

    let delete = app
        .oneshot(auth_request(
            Method::DELETE,
            "/api/security/audit",
            &token,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::METHOD_NOT_ALLOWED);
}
