use axum::{
    body::{to_bytes, Body},
    http::Request,
    response::IntoResponse,
    routing::post,
    Router,
};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc, time::Duration};
use tower::ServiceExt;
use yilian_backend::{
    db::{ConversationGoalBinding, ConversationRunStatus},
    modules::task::{proofloop::*, TaskGraphId},
    safety::ControlSession,
    AppServer,
};

fn goal(server: &AppServer) -> (TaskGraphId, String) {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/proofloop/declared-two-node.json")).unwrap();
    let mut contract: GoalContract = serde_json::from_value(fixture["contract"].clone()).unwrap();
    // This fixture exercises the default chat registry, whose complete tool
    // schemas and system prompt exceed the two-capability planner fixture's
    // 16k conservative byte budget. Keep calls/time bounded, with room for the
    // actual default chat request; no remote paid model is used by this test.
    contract.budget.max_total_tokens = 1_000_000;
    let snapshot: CapabilitySnapshot = serde_json::from_value(fixture["snapshot"].clone()).unwrap();
    let candidate: CandidatePlan = serde_json::from_value(fixture["candidate"].clone()).unwrap();
    let graph = TaskGraphId::generate();
    let now = chrono::Utc::now().timestamp_millis();
    let draft = ProofDraft {
        source: None,
        planning_review: None,
        draft_id: "goal-chat-cancel-draft".into(),
        revision: 1,
        graph_id: graph.to_string(),
        contract: contract.clone(),
        snapshot: snapshot.clone(),
        candidate: Some(candidate.clone()),
        binding_confirmations: vec![],
        compilation: compile_candidate(&contract, &candidate, &snapshot, &graph, &[]),
        status: DraftStatus::ReadyForConfirmation,
        confirmed_graph_revision: None,
        planning_usage: PlanningUsage {
            model_calls: 0,
            total_tokens: Some(0),
            started_at_ms: now,
            elapsed_ms: Some(0),
        },
    };
    server.db.save_proof_draft(&draft, now).unwrap();
    DraftService::new(server.db.clone())
        .confirm_draft(&server.task_world, &draft.draft_id, 1, &snapshot, now)
        .unwrap();
    let conversation = server
        .db
        .create_conversation("取消共享目标对话")
        .unwrap()
        .id;
    server
        .db
        .bind_conversation_goal(&ConversationGoalBinding {
            conversation_id: conversation.clone(),
            goal_id: contract.goal_id,
            goal_version: contract.version,
        })
        .unwrap();
    (graph, conversation)
}

fn request(conversation: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/chat")
        .header("X-Yilian-Control-Session", "h".repeat(64))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"conversation_id":conversation,"message":"继续当前目标"}).to_string(),
        ))
        .unwrap()
}

async fn graph_goal(server: &Arc<AppServer>, graph: &TaskGraphId) -> Value {
    let response = yilian_backend::app::router::build_router(Arc::clone(server))
        .oneshot(
            Request::builder()
                .uri(format!("/api/task-world/proofloop/graphs/{graph}/goal"))
                .header("X-Yilian-Control-Session", "h".repeat(64))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}

#[tokio::test]
async fn bound_conversation_controls_the_same_graph_without_creating_chat_turns() {
    let server = Arc::new(
        AppServer::new_with_control_session(
            Path::new(":memory:"),
            ".",
            ControlSession::new("h".repeat(64)).unwrap(),
        )
        .unwrap(),
    );
    let (graph, conversation) = goal(&server);
    let app = yilian_backend::app::router::build_router(server.clone());
    use yilian_backend::modules::task::TaskExecutionControlState::{Cancelled, Paused, Running};
    for (message, expected) in [
        ("暂停当前任务", Paused),
        ("继续当前任务", Running),
        ("取消当前任务", Cancelled),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat/task-command")
                    .header("X-Yilian-Control-Session", "h".repeat(64))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        json!({"conversation_id":conversation,"message":message}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
        assert!(serde_json::from_slice::<Value>(&body).unwrap()["message"].is_string());
        assert_eq!(
            server.task_world.control_state(&graph).unwrap().state,
            expected
        );
    }
    assert!(server
        .db
        .conversation_turns(&conversation)
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn graph_cancel_interrupts_a_real_goal_chat_stream_before_model_returns() {
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let provider = Router::new().route(
        "/chat/completions",
        post({
            let entered = Arc::clone(&entered);
            let release = Arc::clone(&release);
            move || {
                let entered = Arc::clone(&entered);
                let release = Arc::clone(&release);
                async move {
                    entered.notify_one();
                    release.notified().await;
                    (
                        [("Content-Type", "text/event-stream")],
                        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"late answer\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
                    ).into_response()
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let provider_task = tokio::spawn(async move { axum::serve(listener, provider).await.unwrap() });
    let server = Arc::new(
        AppServer::new_with_control_session(
            Path::new(":memory:"),
            ".",
            ControlSession::new("h".repeat(64)).unwrap(),
        )
        .unwrap(),
    );
    {
        let mut config = server.config.write();
        config.model.base_url = format!("http://{address}");
        config.model.api_key = "local-test-placeholder".into();
        config.model.api_key_env.clear();
        config.model.invoke_timeout_ms = 15_000;
    }
    let (graph, conversation) = goal(&server);
    let response = yilian_backend::app::router::build_router(Arc::clone(&server))
        .oneshot(request(&conversation))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        server
            .db
            .conversation_workspace(&conversation)
            .unwrap()
            .as_deref(),
        Some("ws-fixture"),
        "starting an existing goal chat restores its goal workspace binding"
    );
    let mut consumer =
        tokio::spawn(async move { to_bytes(response.into_body(), 1024 * 1024).await.unwrap() });
    tokio::select! {
        _ = entered.notified() => {},
        result = &mut consumer => panic!("chat ended before the model request: {}", String::from_utf8_lossy(&result.unwrap())),
        _ = tokio::time::sleep(Duration::from_secs(5)) => panic!("model request did not start; persisted conversation: {:?}", server.db.get_conversation(&conversation).unwrap()),
    }
    let active = graph_goal(&server, &graph).await;
    assert_eq!(active["conversations"][0]["conversation_id"], conversation);
    assert_eq!(active["conversations"][0]["run_status"], "running");
    server
        .task_world
        .cancel_task(&graph, chrono::Utc::now().timestamp_millis())
        .unwrap();
    let bytes = tokio::time::timeout(Duration::from_secs(2), consumer)
        .await
        .unwrap()
        .unwrap();
    let stream = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!stream.contains("late answer"));
    assert!(!stream.contains("\"type\":\"tool_start\""));
    assert_eq!(
        server
            .db
            .get_conversation(&conversation)
            .unwrap()
            .run_status,
        ConversationRunStatus::Cancelled
    );
    assert_eq!(
        graph_goal(&server, &graph).await["conversations"][0]["run_status"],
        "cancelled"
    );

    // The same gate is reused by approvals and question-answer resumes.
    let response = yilian_backend::app::router::build_router(Arc::clone(&server))
        .oneshot(request(&conversation))
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    let rejection: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(rejection["code"], "task_cancelled");
    release.notify_one();
    provider_task.abort();
}
