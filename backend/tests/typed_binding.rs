//! Typed binding graph integrity and snapshot contracts (real Task Harness).
use serde_json::json;
use yilian_backend::modules::task::validation::ValidationPolicy;
use yilian_backend::modules::task::*;

fn node(id: &str, bound: bool) -> TaskNode {
    TaskNode::new(TaskNodeId::new(id).unwrap(), TaskNodeKind::Work, id,
        if bound { json!({"executor_ref":"capability://mcp.fixture_b","capability_input":{},"capability_bindings":[{"target_path":["id"],"source":{"kind":"dependency_output","node_id":"a","path":["structured","id"]}}]}) }
        else { json!({"executor_ref":"capability://mcp.fixture_a","capability_input":{}}) }).unwrap()
}

#[test]
fn u31_graph_rejects_unconnected_binding() {
    assert!(TaskGraph::new(
        TaskGraphId::new("u31-red").unwrap(),
        GraphRevision::initial(),
        vec![node("a", false), node("b", true)],
        vec![]
    )
    .is_err());
}

#[test]
fn u31_pending_binding_keeps_snapshot_when_upstream_reruns() {
    let a = TaskNodeId::new("a").unwrap();
    let b = TaskNodeId::new("b").unwrap();
    let graph = TaskGraph::new(
        TaskGraphId::new("u31-snapshot").unwrap(),
        GraphRevision::initial(),
        vec![node("a", false), node("b", true)],
        vec![TaskEdge::new(a.clone(), b.clone())],
    )
    .unwrap();
    let mut resolver = ExecutorResolver::new();
    resolver.register_capability("mcp.fixture_a");
    resolver.register_capability("mcp.fixture_b");
    let mut harness = TaskHarness::new(graph, resolver).unwrap();
    let first = harness.start_node(&a, 1).unwrap();
    harness
        .record_output(&first, json!({"structured":{"id":"first"}}))
        .unwrap();
    harness
        .validate_execution(&first, ValidationPolicy::StructuredResult, 2)
        .unwrap();
    let waiting = harness.start_node(&b, 3).unwrap();
    harness
        .mark_waiting_approval(&waiting, "approval-frozen", 4)
        .unwrap();
    let frozen = harness.execution(&waiting).unwrap().clone();
    harness
        .rerun_from_node(&a, 5)
        .expect("bound pending descendant must preserve its snapshot");
    assert_eq!(harness.execution(&waiting).unwrap(), &frozen);
    let second = harness.start_node(&a, 6).unwrap();
    harness
        .record_output(&second, json!({"structured":{"id":"second"}}))
        .unwrap();
    harness
        .validate_execution(&second, ValidationPolicy::StructuredResult, 7)
        .unwrap();
    assert_eq!(
        harness
            .execution(&waiting)
            .unwrap()
            .context
            .dependency_outputs["a"]["structured"]["id"],
        "first"
    );
    assert!(
        harness.rerun_from_node(&b, 8).is_err(),
        "an active selected root remains protected"
    );
}

#[tokio::test]
async fn u31_http_edge_and_node_delete_preserve_graph_and_bindings() {
    use axum::{
        body::{to_bytes, Body},
        http::{Method, Request, StatusCode},
    };
    use tower::ServiceExt;
    use yilian_backend::{
        api,
        safety::{ControlSession, CONTROL_SESSION_HEADER},
        AppServer,
    };
    let token = uuid::Uuid::new_v4().to_string().repeat(2);
    let server = std::sync::Arc::new(
        AppServer::new_with_control_session(
            std::path::Path::new(":memory:"),
            ".",
            ControlSession::new(token.clone()).unwrap(),
        )
        .unwrap(),
    );
    let a = TaskNodeId::new("a").unwrap();
    let b = TaskNodeId::new("b").unwrap();
    let id = TaskGraphId::new("u31-guards").unwrap();
    let before = server
        .task_world
        .create_graph(
            id.clone(),
            vec![node("a", false), node("b", true)],
            vec![TaskEdge::new(a, b)],
            1,
        )
        .unwrap();
    let app = api::build_router(server.clone());
    for path in [
        "/api/task-world/graphs/u31-guards/edges/a/b",
        "/api/task-world/graphs/u31-guards/nodes/a",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(path)
                    .header(CONTROL_SESSION_HEADER, &token)
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"expected_revision":1}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
        assert!(body["message"]
            .as_str()
            .unwrap()
            .contains("input_binding_source_not_dependency"));
        assert_eq!(server.task_world.get_graph(&id).unwrap(), before);
    }
}
