use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
use tower::ServiceExt;
use yilian_backend::modules::task::artifact::{ArtifactService, ArtifactSource};
use yilian_backend::modules::task::execution::{
    NodeContext, NodeExecution, NodeExecutionId, NodeExecutionStatus, ValidationResult,
};
use yilian_backend::modules::task::model::ArtifactId;
use yilian_backend::modules::task::proofloop::*;
use yilian_backend::modules::task::{TaskGraphId, TaskNodeId};
use yilian_backend::safety::ControlSession;
use yilian_backend::workspace::{Workspace, WorkspaceId};
use yilian_backend::AppServer;

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn server() -> Arc<AppServer> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/proofloop-delivery-tests")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root).unwrap();
    Arc::new(
        AppServer::new_with_control_session(
            Path::new(":memory:"),
            root.to_str().unwrap(),
            ControlSession::new("d".repeat(64)).unwrap(),
        )
        .unwrap(),
    )
}

fn setup(status: NodeExecutionStatus) -> (Arc<AppServer>, GoalContract, TaskGraphId, String) {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/proofloop/declared-two-node.json")).unwrap();
    let mut contract: GoalContract = serde_json::from_value(fixture["contract"].clone()).unwrap();
    contract.workspace_id = "delivery-integrity-workspace".into();
    let snapshot: CapabilitySnapshot = serde_json::from_value(fixture["snapshot"].clone()).unwrap();
    let candidate: CandidatePlan = serde_json::from_value(fixture["candidate"].clone()).unwrap();
    let server = server();
    let graph = TaskGraphId::generate();
    let draft = ProofDraft {
        planning_review: None,
        draft_id: "delivery-integrity-draft".into(),
        revision: 1,
        graph_id: graph.to_string(),
        contract: contract.clone(),
        snapshot: snapshot.clone(),
        candidate: Some(candidate.clone()),
        binding_confirmations: vec![],
        compilation: compile_candidate(&contract, &candidate, &snapshot, &graph, &[]),
        status: DraftStatus::ReadyForConfirmation,
        confirmed_graph_revision: None,
        planning_usage: PlanningUsage::default(),
    };
    server.db.save_proof_draft(&draft, now()).unwrap();
    DraftService::new(server.db.clone())
        .confirm_draft(&server.task_world, &draft.draft_id, 1, &snapshot, now())
        .unwrap();
    let workspace = Workspace::new(
        WorkspaceId::new(&contract.workspace_id).unwrap(),
        "Deliverable integrity".into(),
        String::new(),
        Some(server.workspace_root.clone()),
        now(),
    )
    .unwrap();
    server.db.create_workspace(&workspace).unwrap();

    let mut execution = NodeExecution::new(
        NodeExecutionId::generate(),
        graph.clone(),
        TaskNodeId::new("compare").unwrap(),
        1,
        None,
        NodeContext::new(
            "deliverable integrity",
            "deliverable integrity",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .unwrap(),
        now(),
    )
    .unwrap();
    execution.status = status;
    execution.finished_at = Some(now());
    if status == NodeExecutionStatus::Succeeded {
        execution.output = Some(json!({"structured":{"summary":"verified output"}}));
        execution.validation = Some(ValidationResult::accepted());
    } else {
        execution.failure_code = Some("provider_unavailable".into());
    }
    server.db.save_node_execution(&execution).unwrap();

    (server, contract, graph, execution.id.to_string())
}

async fn call(server: &Arc<AppServer>, method: &str, uri: &str, body: Value) -> (u16, Value) {
    let response = yilian_backend::app::router::build_router(server.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("X-Yilian-Control-Session", "d".repeat(64))
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
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
async fn human_pass_and_failed_execution_without_report_stays_failed_after_refresh() {
    let (server, mut contract, _, execution_id) = setup(NodeExecutionStatus::Failed);
    let deliverable_id = contract.deliverables[0].id.clone();
    contract.criteria = vec![Criterion {
        id: "human_quality".into(),
        description: "人工质量复核".into(),
        required: true,
        check: CriterionCheck::HumanReview {
            deliverable_id,
            rubric: "报告质量合格".into(),
        },
    }];
    server.db.save_goal_version(&contract, now()).unwrap();

    let (status, report) = call(
        &server,
        "POST",
        &format!(
            "/api/task-world/proofloop/goals/{}/evaluations",
            contract.goal_id
        ),
        json!({"goal_version":contract.version,"graph_revision":1,"execution_ids":[execution_id]}),
    )
    .await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["status"], "fail");
    assert_eq!(report["deliverable_results"][0]["code"], "artifact_missing");

    let report_id = report["report_id"].as_str().unwrap();
    let (status, reviewed) = call(
        &server,
        "POST",
        &format!("/api/task-world/proofloop/evaluations/{report_id}/human-review"),
        json!({"criterion_id":"human_quality","decision":"pass","reason":"人工选择通过"}),
    )
    .await;
    assert_eq!(status, 200, "{reviewed}");
    assert_eq!(reviewed["human_reviews"][0]["decision"], "pass");
    assert_eq!(reviewed["results"][0]["status"], "fail");
    assert_eq!(reviewed["status"], "fail");

    let (status, detail) = call(
        &server,
        "GET",
        &format!("/api/task-world/proofloop/evaluations/{report_id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{detail}");
    assert_eq!(detail["report"]["status"], "fail");
}

#[tokio::test]
async fn exact_execution_finds_verified_deliverable_after_more_than_two_hundred_artifacts() {
    let (server, contract, graph, execution_id) = setup(NodeExecutionStatus::Succeeded);
    let artifacts = ArtifactService::new(server.db.clone_connection());
    let materialized = artifacts
        .materialize(
            &ArtifactSource::Node {
                graph_id: graph.to_string(),
                node_id: "compare".into(),
                execution_id: execution_id.clone(),
                workspace_id: contract.workspace_id.clone(),
            },
            &contract.deliverables[0].title,
            &server.workspace_root,
            now(),
        )
        .unwrap()
        .0;

    for index in 0..250 {
        let id = ArtifactId::generate();
        let created_at = now() + index;
        server
            .db
            .conn()
            .execute(
                "INSERT INTO artifacts
                 (id,workspace_id,task_id,task_execution_id,name,artifact_type,path,mime_type,size,summary,created_at,updated_at)
                 VALUES (?1,?2,?3,?4,?5,'report',NULL,NULL,NULL,'unrelated history',?6,?6)",
                rusqlite::params![
                    id.as_str(),
                    contract.workspace_id.as_str(),
                    materialized.task_id.as_str(),
                    materialized.task_execution_id.as_str(),
                    contract.deliverables[0].title.as_str(),
                    created_at,
                ],
            )
            .unwrap();
    }

    let (snapshot, contents) = collect_evidence(
        &server.db,
        &artifacts,
        &contract,
        graph.as_str(),
        1,
        &[execution_id],
        &server.workspace_root,
        now(),
    )
    .unwrap();
    assert_eq!(snapshot.deliverables.len(), 1);
    assert_eq!(
        snapshot.deliverables[0].artifact_id,
        materialized.id.to_string()
    );
    let report = evaluate_goal(&contract, &snapshot, &contents, &[], now()).unwrap();
    assert_eq!(report.deliverable_results[0].code, "verified");
    assert_eq!(report.status, GoalStatus::Pass);
}

#[test]
fn old_reports_without_deliverable_results_deserialize_as_unverified() {
    let report = json!({
        "report_id":"old", "goal_id":"goal", "goal_version":1,
        "graph_id":"graph", "graph_revision":1, "snapshot_id":"snapshot",
        "status":"pass", "results":[], "human_reviews":[], "checked_at_ms":1
    });
    let decoded: AcceptanceReport = serde_json::from_value(report).unwrap();
    assert!(decoded.deliverable_results.is_empty());
}
