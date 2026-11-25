use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
use tower::ServiceExt;
use yilian_backend::{
    db::MessageRow,
    modules::task::{proofloop::*, TaskGraphId},
    safety::ControlSession,
    workspace::{Workspace, WorkspaceId},
    AppServer,
};

pub fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn setup() -> (Arc<AppServer>, GoalContract, TaskGraphId) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/shared-goal-activity-tests")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root).unwrap();
    let server = Arc::new(
        AppServer::new_with_control_session(
            Path::new(":memory:"),
            root.to_str().unwrap(),
            ControlSession::new("z".repeat(64)).unwrap(),
        )
        .unwrap(),
    );
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/proofloop/declared-two-node.json")).unwrap();
    let mut contract: GoalContract = serde_json::from_value(fixture["contract"].clone()).unwrap();
    contract.workspace_id = "shared-activity-workspace".into();
    contract.criteria.push(Criterion {
        id: "human".into(),
        description: "Review the result".into(),
        required: true,
        check: CriterionCheck::HumanReview {
            deliverable_id: contract.deliverables[0].id.clone(),
            rubric: "Review the report".into(),
        },
    });
    let snapshot: CapabilitySnapshot = serde_json::from_value(fixture["snapshot"].clone()).unwrap();
    let candidate: CandidatePlan = serde_json::from_value(fixture["candidate"].clone()).unwrap();
    let graph = TaskGraphId::generate();
    let draft = ProofDraft {
        source: None,
        planning_review: None,
        draft_id: "activity-draft".into(),
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
            total_tokens: Some(0),
            ..PlanningUsage::default()
        },
    };
    server.db.save_proof_draft(&draft, now()).unwrap();
    DraftService::new(server.db.clone())
        .confirm_draft(&server.task_world, &draft.draft_id, 1, &snapshot, now())
        .unwrap();
    server
        .db
        .create_workspace(
            &Workspace::new(
                WorkspaceId::new(&contract.workspace_id).unwrap(),
                "Activity".into(),
                String::new(),
                Some(server.workspace_root.clone()),
                now(),
            )
            .unwrap(),
        )
        .unwrap();
    (server, contract, graph)
}

pub async fn call(server: &Arc<AppServer>, method: &str, uri: &str, value: Value) -> (u16, Value) {
    let response = yilian_backend::app::router::build_router(server.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("X-Yilian-Control-Session", "z".repeat(64))
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

pub async fn bound_conversation(server: &Arc<AppServer>, graph: &TaskGraphId) -> String {
    let id = server.db.create_conversation("Shared activity").unwrap().id;
    assert_eq!(
        call(
            server,
            "PUT",
            &format!("/api/conversations/{id}/goal"),
            json!({"graph_id":graph})
        )
        .await
        .0,
        200
    );
    id
}

pub fn start_turn(server: &AppServer, conversation: &str) {
    server
        .db
        .accept_conversation_turn(&MessageRow {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: conversation.into(),
            role: "user".into(),
            content: "Continue the goal".into(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at: now(),
        })
        .unwrap();
}
