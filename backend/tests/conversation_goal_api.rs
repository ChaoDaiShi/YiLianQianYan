use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
use tower::ServiceExt;
use yilian_backend::{
    modules::task::{proofloop::*, TaskGraphId},
    safety::ControlSession,
    workspace::{Workspace, WorkspaceId},
    AppServer,
};

fn server(path: &Path) -> Arc<AppServer> {
    Arc::new(
        AppServer::new_with_control_session(
            path,
            ".",
            ControlSession::new("g".repeat(64)).unwrap(),
        )
        .unwrap(),
    )
}

fn confirm(server: &Arc<AppServer>, suffix: &str) -> String {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/proofloop/declared-two-node.json")).unwrap();
    let mut contract: GoalContract = serde_json::from_value(fixture["contract"].clone()).unwrap();
    contract.goal_id = format!("shared-goal-{suffix}");
    contract.workspace_id = "shared-workspace".into();
    let snapshot: CapabilitySnapshot = serde_json::from_value(fixture["snapshot"].clone()).unwrap();
    let candidate: CandidatePlan = serde_json::from_value(fixture["candidate"].clone()).unwrap();
    let graph_id = TaskGraphId::generate();
    let draft = ProofDraft {
        source: None,
        planning_review: None,
        draft_id: format!("shared-draft-{suffix}"),
        revision: 1,
        graph_id: graph_id.to_string(),
        contract: contract.clone(),
        snapshot: snapshot.clone(),
        candidate: Some(candidate.clone()),
        binding_confirmations: vec![],
        compilation: compile_candidate(&contract, &candidate, &snapshot, &graph_id, &[]),
        status: DraftStatus::ReadyForConfirmation,
        confirmed_graph_revision: None,
        planning_usage: PlanningUsage {
            total_tokens: Some(0),
            ..PlanningUsage::default()
        },
    };
    let now = chrono::Utc::now().timestamp_millis();
    server.db.save_proof_draft(&draft, now).unwrap();
    DraftService::new(server.db.clone())
        .confirm_draft(&server.task_world, &draft.draft_id, 1, &snapshot, now)
        .unwrap();
    graph_id.to_string()
}

async fn call(server: &Arc<AppServer>, method: &str, uri: &str, value: Value) -> (u16, Value) {
    let response = yilian_backend::app::router::build_router(server.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("X-Yilian-Control-Session", "g".repeat(64))
                .header("Content-Type", "application/json")
                .body(Body::from(value.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn goal_binding_is_shared_persistent_and_immutable() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/shared-goal-tests")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("state.sqlite3");
    let app = server(&path);
    let workspace = Workspace::new(
        WorkspaceId::new("shared-workspace").unwrap(),
        "Shared".into(),
        String::new(),
        None,
        1,
    )
    .unwrap();
    app.db.create_workspace(&workspace).unwrap();
    let graph = confirm(&app, "one");
    let other_graph = confirm(&app, "two");
    let conversation = app.db.create_conversation("Shared task").unwrap().id;
    let uri = format!("/api/conversations/{conversation}/goal");
    assert_eq!(
        call(&app, "GET", &uri, Value::Null).await,
        (200, json!({"binding":null}))
    );
    let (status, binding) = call(&app, "PUT", &uri, json!({"graph_id":graph})).await;
    assert_eq!(status, 200);
    assert_eq!(binding["binding"]["graph_id"], graph);
    assert_eq!(binding["binding"]["workspace_id"], "shared-workspace");
    assert_eq!(
        app.db
            .conversation_workspace(&conversation)
            .unwrap()
            .as_deref(),
        Some("shared-workspace")
    );
    assert_eq!(
        call(&app, "PUT", &uri, json!({"graph_id":graph})).await,
        (200, binding.clone())
    );
    assert_eq!(
        call(&app, "PUT", &uri, json!({"graph_id":other_graph}))
            .await
            .0,
        409
    );
    assert_eq!(
        call(
            &app,
            "PUT",
            &format!("/api/conversations/{conversation}/workspace"),
            json!({"workspace_id":null})
        )
        .await
        .0,
        409
    );
    let (_, goal) = call(
        &app,
        "GET",
        &format!("/api/task-world/proofloop/graphs/{graph}/goal"),
        Value::Null,
    )
    .await;
    assert_eq!(goal["conversation_ids"], json!([conversation]));
    // Keep the same identity after a restart, with the unfinished operation
    // exposed as an interrupted failure rather than still running.
    let mut interrupted = yilian_backend::modules::task::NodeExecution::new(
        yilian_backend::modules::task::NodeExecutionId::generate(),
        TaskGraphId::new(&graph).unwrap(),
        yilian_backend::modules::task::TaskNodeId::new("compare").unwrap(),
        1,
        None,
        yilian_backend::modules::task::NodeContext::new(
            "Shared goal",
            "Interrupted operation",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .unwrap(),
        3,
    )
    .unwrap();
    interrupted.status = yilian_backend::modules::task::NodeExecutionStatus::Running;
    app.db.save_node_execution(&interrupted).unwrap();
    drop(app);
    let app = server(&path);
    assert_eq!(call(&app, "GET", &uri, Value::Null).await, (200, binding));
    let (_, detail) = call(
        &app,
        "GET",
        &format!("/api/task-world/graphs/{graph}/detail"),
        Value::Null,
    )
    .await;
    let failed = detail["detail"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == "compare")
        .unwrap();
    assert_eq!(failed["latest_execution"]["status"], "failed");
    assert_eq!(
        failed["latest_execution"]["failure_code"],
        "execution_interrupted"
    );
    assert!(app
        .task_world
        .get_graph(&TaskGraphId::new(graph).unwrap())
        .is_some());
}

#[tokio::test]
async fn goal_binding_rejects_workspace_conflicts_and_unknown_entities() {
    let app = server(Path::new(":memory:"));
    for id in ["shared-workspace", "other-workspace"] {
        app.db
            .create_workspace(
                &Workspace::new(
                    WorkspaceId::new(id).unwrap(),
                    id.into(),
                    String::new(),
                    None,
                    1,
                )
                .unwrap(),
            )
            .unwrap();
    }
    let graph = confirm(&app, "one");
    let conversation = app.db.create_conversation("Other workspace").unwrap().id;
    app.db
        .set_conversation_workspace(&conversation, Some("other-workspace"))
        .unwrap();
    let uri = format!("/api/conversations/{conversation}/goal");
    assert_eq!(
        call(&app, "PUT", &uri, json!({"graph_id":graph})).await.0,
        409
    );
    assert_eq!(
        call(&app, "GET", &uri, Value::Null).await.1,
        json!({"binding":null})
    );
    assert_eq!(
        app.db
            .conversation_workspace(&conversation)
            .unwrap()
            .as_deref(),
        Some("other-workspace")
    );
    assert_eq!(
        call(&app, "PUT", &uri, json!({"graph_id":"missing"}))
            .await
            .0,
        404
    );
    assert_eq!(
        call(&app, "GET", "/api/conversations/missing/goal", Value::Null)
            .await
            .0,
        404
    );
    assert_eq!(
        call(
            &app,
            "PUT",
            "/api/conversations/missing/goal",
            json!({"graph_id":graph})
        )
        .await
        .0,
        404
    );
}

#[tokio::test]
async fn goal_binding_cannot_relabel_an_active_or_preparing_chat_turn() {
    let app = server(Path::new(":memory:"));
    app.db
        .create_workspace(
            &Workspace::new(
                WorkspaceId::new("shared-workspace").unwrap(),
                "Shared".into(),
                String::new(),
                None,
                1,
            )
            .unwrap(),
        )
        .unwrap();
    let graph = confirm(&app, "active");
    for status in [
        yilian_backend::db::ConversationRunStatus::Running,
        yilian_backend::db::ConversationRunStatus::WaitingInput,
        yilian_backend::db::ConversationRunStatus::WaitingApproval,
    ] {
        let conversation = app.db.create_conversation("Existing turn").unwrap().id;
        app.db
            .set_conversation_run_status(&conversation, status, None)
            .unwrap();
        assert_eq!(
            call(
                &app,
                "PUT",
                &format!("/api/conversations/{conversation}/goal"),
                json!({"graph_id":graph})
            )
            .await
            .0,
            409
        );
        assert!(app
            .db
            .load_conversation_goal_binding(&conversation)
            .unwrap()
            .is_none());
    }
    // The turn owns its gate during preparation, before run_status is written.
    let conversation = app.db.create_conversation("Preparing turn").unwrap().id;
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    app.chat_turn_gates
        .lock()
        .insert(conversation.clone(), gate.clone());
    let held = gate.lock_owned().await;
    assert_eq!(
        call(
            &app,
            "PUT",
            &format!("/api/conversations/{conversation}/goal"),
            json!({"graph_id":graph})
        )
        .await
        .0,
        409
    );
    assert!(app
        .db
        .load_conversation_goal_binding(&conversation)
        .unwrap()
        .is_none());
    drop(held);
    assert_eq!(
        call(
            &app,
            "PUT",
            &format!("/api/conversations/{conversation}/goal"),
            json!({"graph_id":graph})
        )
        .await
        .0,
        200
    );
}

#[tokio::test]
async fn first_chat_goal_reference_uses_the_same_workspace_binding() {
    let app = server(Path::new(":memory:"));
    {
        let mut config = app.config.write();
        config.model.api_key = "local-test-placeholder".into();
        config.model.api_key_env.clear();
        config.model.base_url = "http://127.0.0.1:9".into();
    }
    for id in ["shared-workspace", "other-workspace"] {
        app.db
            .create_workspace(
                &Workspace::new(
                    WorkspaceId::new(id).unwrap(),
                    id.into(),
                    String::new(),
                    None,
                    1,
                )
                .unwrap(),
            )
            .unwrap();
    }
    let graph = confirm(&app, "legacy-chat");
    let draft = app
        .db
        .confirmed_proof_draft_for_graph(&graph)
        .unwrap()
        .unwrap();
    let conversation = app
        .db
        .create_conversation("Legacy goal request")
        .unwrap()
        .id;
    app.db
        .set_conversation_workspace(&conversation, Some("other-workspace"))
        .unwrap();
    let (status, body) = call(&app, "POST", "/api/chat", json!({"conversation_id":conversation,"message":"Continue the goal","goal_ref":{"goal_id":draft.contract.goal_id,"goal_version":draft.contract.version}})).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["code"], "conversation_goal_workspace_conflict");
    assert!(app
        .db
        .load_conversation_goal_binding(&conversation)
        .unwrap()
        .is_none());
    assert!(app
        .db
        .get_conversation(&conversation)
        .unwrap()
        .messages
        .is_empty());
}
