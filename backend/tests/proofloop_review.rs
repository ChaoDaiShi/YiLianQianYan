use axum::{
    body::{to_bytes, Body},
    http::Request,
    Json,
};
use serde_json::{json, Value};
use std::{collections::HashMap, path::Path, sync::Arc};
use tower::ServiceExt;
use yilian_backend::modules::task::artifact::{ArtifactService, ArtifactSource};
use yilian_backend::modules::task::execution::{
    NodeContext, NodeExecution, NodeExecutionId, NodeExecutionStatus, ValidationResult,
};
use yilian_backend::modules::task::proofloop::*;
use yilian_backend::modules::task::runtime::TaskWorldRuntime;
use yilian_backend::modules::task::{TaskGraphId, TaskNodeId};
use yilian_backend::safety::ControlSession;
use yilian_backend::shared::event::EventHub;
use yilian_backend::workspace::{Workspace, WorkspaceId};
use yilian_backend::{AppServer, Database};

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
fn bundle() -> (GoalContract, CapabilitySnapshot, CandidatePlan) {
    let raw: Value =
        serde_json::from_str(include_str!("fixtures/proofloop/declared-two-node.json")).unwrap();
    (
        serde_json::from_value(raw["contract"].clone()).unwrap(),
        serde_json::from_value(raw["snapshot"].clone()).unwrap(),
        serde_json::from_value(raw["candidate"].clone()).unwrap(),
    )
}
fn server() -> Arc<AppServer> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/proofloop-review-tests")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root).unwrap();
    Arc::new(
        AppServer::new_with_control_session(
            Path::new(":memory:"),
            root.to_str().unwrap(),
            ControlSession::new("r".repeat(64)).unwrap(),
        )
        .unwrap(),
    )
}
fn draft(
    id: &str,
    graph: &TaskGraphId,
    c: &GoalContract,
    s: &CapabilitySnapshot,
    p: &CandidatePlan,
) -> ProofDraft {
    ProofDraft {
        draft_id: id.into(),
        revision: 1,
        graph_id: graph.to_string(),
        contract: c.clone(),
        snapshot: s.clone(),
        candidate: Some(p.clone()),
        binding_confirmations: vec![],
        compilation: compile_candidate(c, p, s, graph, &[]),
        status: DraftStatus::ReadyForConfirmation,
        confirmed_graph_revision: None,
        planning_usage: PlanningUsage::default(),
    }
}
fn setup(srv: &AppServer) -> (GoalContract, TaskGraphId, String) {
    let (mut c, s, p) = bundle();
    c.workspace_id = "review-workspace".into();
    let graph = TaskGraphId::generate();
    let d = draft("review-draft", &graph, &c, &s, &p);
    srv.db.save_proof_draft(&d, now()).unwrap();
    DraftService::new(srv.db.clone())
        .confirm_draft(&srv.task_world, &d.draft_id, 1, &s, now())
        .unwrap();
    let ws = Workspace::new(
        WorkspaceId::new(&c.workspace_id).unwrap(),
        "Review".into(),
        String::new(),
        Some(srv.workspace_root.clone()),
        now(),
    )
    .unwrap();
    srv.db.create_workspace(&ws).unwrap();
    let mut e = NodeExecution::new(
        NodeExecutionId::generate(),
        graph.clone(),
        TaskNodeId::new("compare").unwrap(),
        1,
        None,
        NodeContext::new(
            "review",
            "review",
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
    e.status = NodeExecutionStatus::Succeeded;
    e.output = Some(json!({"structured":{"text":"actual isolated output"}}));
    e.validation = Some(ValidationResult::accepted());
    e.finished_at = Some(now());
    srv.db.save_node_execution(&e).unwrap();
    (c, graph, e.id.to_string())
}
async fn call(srv: &Arc<AppServer>, method: &str, uri: &str, body: Value) -> (u16, Value) {
    let response = yilian_backend::app::router::build_router(srv.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("X-Yilian-Control-Session", "r".repeat(64))
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

#[test]
fn compiler_rejects_invalid_fixed_type_and_enum() {
    let (c, mut s, mut p) = bundle();
    let id = TaskGraphId::generate();
    p.nodes[0].literal_input.insert("query".into(), json!(123));
    assert_eq!(
        compile_candidate(&c, &p, &s, &id, &[]).status,
        CompileStatus::Invalid
    );
    p.nodes[0]
        .literal_input
        .insert("query".into(), json!("not-allowed"));
    s.capabilities[0].input_schema.as_mut().unwrap()["properties"]["query"]["enum"] =
        json!(["frameworks"]);
    assert_eq!(
        compile_candidate(&c, &p, &s, &id, &[]).status,
        CompileStatus::Invalid
    );
}

#[test]
fn workflow_uses_existing_workflow_executor() {
    let (mut c, mut s, mut p) = bundle();
    s.capabilities[0].kind = CapabilityKindSpec::Workflow;
    s.capabilities[0].capability_id = "workflow.saved".into();
    s.capabilities[0].executor_ref = "workflow://saved".into();
    s.capabilities[0].input_schema = None;
    s.capabilities[0].output_schema = None;
    c.allowed_capability_ids = vec!["workflow.saved".into()];
    p.nodes.truncate(1);
    p.nodes[0].capability_id = "workflow.saved".into();
    p.nodes[0].literal_input.clear();
    p.edges.clear();
    p.deliverable_nodes[0].node_id = "collect".into();
    let result = compile_candidate(&c, &p, &s, &TaskGraphId::generate(), &[]);
    assert_eq!(result.status, CompileStatus::Ready);
    assert_eq!(
        result.graph.unwrap().nodes[0].input["executor_ref"],
        "workflow://saved"
    );
}

#[test]
fn nested_fixed_fields_can_share_an_object_with_a_binding() {
    let (c, mut s, mut p) = bundle();
    s.capabilities[1].input_schema = Some(
        json!({"type":"object","properties":{"options":{"type":"object","properties":{"title":{"type":"string"},"sources":{"type":"array","items":{"type":"string"}}},"required":["title","sources"]}},"required":["options"]}),
    );
    p.nodes[1].literal_input = json!({"options":{"title":"Report"}})
        .as_object()
        .unwrap()
        .clone();
    p.nodes[1].bindings[0].target_path = vec!["options".into(), "sources".into()];
    let result = compile_candidate(&c, &p, &s, &TaskGraphId::generate(), &[]);
    assert_eq!(
        result.status,
        CompileStatus::Ready,
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn forged_confirmation_cannot_satisfy_unknown_output() {
    let (c, mut s, p) = bundle();
    s.capabilities[0].output_schema = None;
    let b = &p.nodes[1].bindings[0];
    let confirmation = BindingConfirmation {
        node_id: "compare".into(),
        target_path: b.target_path.clone(),
        source_node_id: "collect".into(),
        source_capability_id: "wrong-capability".into(),
        source_execution_id: "missing".into(),
        source_path: b.source.path().to_vec(),
        observed_type: "array".into(),
        confirmed_at_ms: 0,
    };
    assert_ne!(
        compile_candidate(&c, &p, &s, &TaskGraphId::generate(), &[confirmation]).status,
        CompileStatus::Ready
    );
}

#[test]
fn failed_confirmation_rolls_back_graph_and_can_retry() {
    let db = Database::new(Path::new(":memory:")).unwrap();
    let runtime = TaskWorldRuntime::new(&db, EventHub::new(16)).unwrap();
    let (c, s, p) = bundle();
    let id = TaskGraphId::generate();
    let d = draft("fail-confirm", &id, &c, &s, &p);
    db.save_proof_draft(&d, now()).unwrap();
    db.conn().execute_batch("CREATE TRIGGER review_confirmation_failure BEFORE UPDATE ON proofloop_drafts WHEN NEW.status='confirmed' BEGIN SELECT RAISE(ABORT,'injected confirmation failure'); END;").unwrap();
    let service = DraftService::new(db.clone());
    assert!(service
        .confirm_draft(&runtime, &d.draft_id, 1, &s, now())
        .is_err());
    assert!(runtime.get_graph(&id).is_none());
    assert!(db
        .load_goal_version(&c.goal_id, c.version)
        .unwrap()
        .is_none());
    db.conn()
        .execute_batch("DROP TRIGGER review_confirmation_failure;")
        .unwrap();
    assert!(service
        .confirm_draft(&runtime, &d.draft_id, 1, &s, now())
        .is_ok());
}

#[tokio::test]
async fn distinct_goal_and_graph_are_resolved_by_http() {
    let srv = server();
    let (c, g, e) = setup(&srv);
    assert_ne!(c.goal_id, g.to_string());
    let (status, body) = call(
        &srv,
        "POST",
        &format!("/api/task-world/proofloop/goals/{}/evaluations", c.goal_id),
        json!({"goal_version":1,"graph_revision":1,"execution_ids":[e]}),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["graph_id"], g.to_string());
    assert_eq!(body["status"], "fail");
}

#[tokio::test]
async fn evidence_rejects_stale_graph_revision() {
    let srv = server();
    let (c, g, e) = setup(&srv);
    let artifacts = ArtifactService::new(srv.db.clone_connection());
    let result = collect_evidence(
        &srv.db,
        &artifacts,
        &c,
        g.as_str(),
        999,
        &[e],
        &srv.workspace_root,
        now(),
    );
    assert!(result.is_err());
}

#[tokio::test]
async fn evidence_never_uses_a_same_name_artifact_from_another_graph() {
    let srv = server();
    let (c, g, e) = setup(&srv);
    let artifacts = ArtifactService::new(srv.db.clone_connection());
    let source = ArtifactSource::Node {
        graph_id: g.to_string(),
        node_id: "compare".into(),
        execution_id: e.clone(),
        workspace_id: c.workspace_id.clone(),
    };
    artifacts
        .materialize(
            &source,
            &c.deliverables[0].title,
            &srv.workspace_root,
            now(),
        )
        .unwrap();
    let other = TaskGraphId::generate();
    let (mut c2, s, p) = bundle();
    c2.goal_id = "other-goal".into();
    c2.workspace_id = c.workspace_id.clone();
    let d = draft("other-draft", &other, &c2, &s, &p);
    srv.db.save_proof_draft(&d, now()).unwrap();
    DraftService::new(srv.db.clone())
        .confirm_draft(&srv.task_world, &d.draft_id, 1, &s, now())
        .unwrap();
    let mut current = srv
        .db
        .load_node_execution(&NodeExecutionId::new(&e).unwrap())
        .unwrap()
        .unwrap();
    current.id = NodeExecutionId::generate();
    current.graph_id = other.clone();
    srv.db.save_node_execution(&current).unwrap();
    let (snapshot, contents) = collect_evidence(
        &srv.db,
        &artifacts,
        &c2,
        other.as_str(),
        1,
        &[current.id.to_string()],
        &srv.workspace_root,
        now(),
    )
    .unwrap();
    assert!(snapshot.deliverables.is_empty());
    assert_eq!(
        evaluate_goal(&c2, &snapshot, &contents, &[], now())
            .unwrap()
            .status,
        GoalStatus::Fail
    );
}

#[tokio::test]
async fn human_review_keeps_real_automatic_evidence() {
    let srv = server();
    let (mut c, g, e) = setup(&srv);
    let artifacts = ArtifactService::new(srv.db.clone_connection());
    artifacts
        .materialize(
            &ArtifactSource::Node {
                graph_id: g.to_string(),
                node_id: "compare".into(),
                execution_id: e.clone(),
                workspace_id: c.workspace_id.clone(),
            },
            &c.deliverables[0].title,
            &srv.workspace_root,
            now(),
        )
        .unwrap();
    c.criteria.push(Criterion {
        id: "human".into(),
        description: "review".into(),
        required: true,
        check: CriterionCheck::HumanReview {
            deliverable_id: c.deliverables[0].id.clone(),
            rubric: "review output".into(),
        },
    });
    srv.db.save_goal_version(&c, now()).unwrap();
    let (snapshot, content) = collect_evidence(
        &srv.db,
        &artifacts,
        &c,
        g.as_str(),
        1,
        &[e],
        &srv.workspace_root,
        now(),
    )
    .unwrap();
    let report = evaluate_goal(&c, &snapshot, &content, &[], now()).unwrap();
    srv.db
        .save_acceptance_report(&report, &snapshot, now())
        .unwrap();
    let (status, body) = call(
        &srv,
        "POST",
        &format!(
            "/api/task-world/proofloop/evaluations/{}/human-review",
            report.report_id
        ),
        json!({"criterion_id":"human","decision":"pass","reason":"verified actual output"}),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "pass");
}

#[test]
fn invalid_input_requires_a_real_value_instead_of_a_placeholder() {
    let (c, s, _) = bundle();
    let mut e = NodeExecution::new(
        NodeExecutionId::generate(),
        TaskGraphId::generate(),
        TaskNodeId::new("collect").unwrap(),
        1,
        None,
        NodeContext::new(
            "review",
            "review",
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
    e.status = NodeExecutionStatus::Failed;
    e.failure_code = Some("invalid_argument".into());
    assert!(classify_failure(&e, None, &s, &c, 0, 1, "review-report".into()).is_err());
}

#[tokio::test]
async fn planner_accepts_its_documented_model_response() {
    let srv = server();
    let (_, s, p) = bundle();
    let mut candidate = serde_json::to_value(&p).unwrap();
    candidate.as_object_mut().unwrap().remove("schema_version");
    let response = json!({"deliverables":[{"id":"report","title":"report.md","mime_type":"text/markdown"}],"criteria":[],"candidate":candidate});
    let provider=axum::Router::new().route("/chat/completions",axum::routing::post(move || {let response=response.clone();async move{Json(json!({"id":"review","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":response.to_string()}}]}))}}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move { axum::serve(listener, provider).await.unwrap() });
    let mut config = srv.effective_model_config();
    config.base_url = format!("http://{addr}");
    config.api_key = "local-test-placeholder".into();
    config.invoke_timeout_ms = 2000;
    let client = yilian_backend::integrations::llm::client::LlmClient::new(
        &config,
        srv.secret_resolver.clone(),
    );
    let result = propose_candidate(
        "review-workspace",
        "review sources",
        &s,
        Some(&client),
        &tokio_util::sync::CancellationToken::new(),
    )
    .await;
    handle.abort();
    assert!(result.is_ok(), "{result:?}");
}

async fn failed_workflow_goal() -> (
    Arc<AppServer>,
    GoalContract,
    TaskGraphId,
    NodeExecution,
    AcceptanceReport,
) {
    let srv = server();
    srv.db.create_workflow_graph(&yilian_backend::db::WorkflowGraphRecord { id:"review-output".into(),name:"Review output".into(),description:String::new(),created_at:now(),updated_at:now(),definition:serde_json::from_value(json!({"schema_version":1,"entry_node_id":"out","nodes":[{"id":"out","kind":"output","config":{"type":"output","template":"actual repaired workflow result"}}],"edges":[]})).unwrap() }).unwrap();
    let (mut c, mut s, mut p) = bundle();
    c.allowed_capability_ids = vec!["workflow.review-output".into()];
    s.capabilities.truncate(1);
    s.capabilities[0].capability_id = "workflow.review-output".into();
    s.capabilities[0].kind = CapabilityKindSpec::Workflow;
    s.capabilities[0].executor_ref = "workflow://review-output".into();
    s.capabilities[0].input_schema = None;
    s.capabilities[0].output_schema = None;
    p.nodes.truncate(1);
    p.nodes[0].capability_id = "workflow.review-output".into();
    p.nodes[0].literal_input.clear();
    p.edges.clear();
    p.deliverable_nodes[0].node_id = "collect".into();
    let graph = TaskGraphId::generate();
    let d = draft("workflow-draft", &graph, &c, &s, &p);
    srv.db.save_proof_draft(&d, now()).unwrap();
    DraftService::new(srv.db.clone())
        .confirm_draft(&srv.task_world, &d.draft_id, 1, &s, now())
        .unwrap();
    let node = TaskNodeId::new("collect").unwrap();
    let resolver =
        yilian_backend::modules::task::application::execution_service::executor_resolver(
            &srv, &graph, &node,
        )
        .await
        .unwrap();
    let first = srv
        .task_world
        .start_execution_with_resolver(&graph, &node, 1, resolver, now())
        .unwrap();
    let failed = srv
        .task_world
        .fail_execution(
            &graph,
            &first.id,
            "provider_unavailable",
            "provider unavailable before dispatch",
            now(),
        )
        .unwrap();
    let snapshot = EvidenceSnapshot {
        snapshot_id: "failed-workflow-evidence".into(),
        goal_id: c.goal_id.clone(),
        goal_version: 1,
        graph_id: graph.to_string(),
        graph_revision: 1,
        executions: vec![ExecutionRef {
            graph_id: graph.to_string(),
            graph_revision: 1,
            node_id: "collect".into(),
            execution_id: failed.id.to_string(),
        }],
        evidence: vec![],
        deliverables: vec![],
        sources: vec![],
    };
    let report = evaluate_goal(&c, &snapshot, &HashMap::new(), &[], now()).unwrap();
    srv.db
        .save_acceptance_report(&report, &snapshot, now())
        .unwrap();
    (srv, c, graph, failed, report)
}

#[tokio::test]
async fn repair_requests_consume_the_two_round_budget() {
    let (srv, c, _, failed, report) = failed_workflow_goal().await;
    let uri = format!("/api/task-world/proofloop/goals/{}/repairs", c.goal_id);
    for round in [1, 2] {
        let (status, body) = call(
            &srv,
            "POST",
            &uri,
            json!({"report_id":report.report_id,"failure_execution_ids":[failed.id.to_string()]}),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["round"], round);
    }
    let (status, body) = call(
        &srv,
        "POST",
        &uri,
        json!({"report_id":report.report_id,"failure_execution_ids":[failed.id.to_string()]}),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["code"], "proofloop_budget_exhausted");
}

#[tokio::test]
async fn repair_checks_model_token_and_elapsed_budgets() {
    for kind in ["model", "tokens", "time"] {
        let (srv, mut c, _, failed, report) = failed_workflow_goal().await;
        let mut d = srv
            .db
            .confirmed_proof_draft_for_goal(&c.goal_id, c.version)
            .unwrap()
            .unwrap();
        match kind {
            "model" => c.budget.max_model_calls = 0,
            "tokens" => {
                c.budget.max_total_tokens = 10;
                d.planning_usage.total_tokens = Some(11);
            }
            _ => {
                c.budget.max_wall_time_ms = 10;
                d.planning_usage.started_at_ms = now() - 100;
            }
        }
        d.contract = c.clone();
        srv.db.save_goal_version(&c, now()).unwrap();
        srv.db.save_proof_draft(&d, now()).unwrap();
        let (status, body) = call(
            &srv,
            "POST",
            &format!("/api/task-world/proofloop/goals/{}/repairs", c.goal_id),
            json!({"report_id":report.report_id,"failure_execution_ids":[failed.id.to_string()]}),
        )
        .await;
        assert_eq!(status, 422, "{kind}: {body}");
        assert_eq!(body["code"], "proofloop_budget_exhausted", "{kind}");
    }
}

#[tokio::test]
async fn repair_application_reserves_and_dispatches_a_new_attempt_once() {
    let (srv, c, g, failed, report) = failed_workflow_goal().await;
    let (status, proposal) = call(
        &srv,
        "POST",
        &format!("/api/task-world/proofloop/goals/{}/repairs", c.goal_id),
        json!({"report_id":report.report_id,"failure_execution_ids":[failed.id.to_string()]}),
    )
    .await;
    assert_eq!(status, 200);
    let uri = format!(
        "/api/task-world/proofloop/repairs/{}/apply",
        proposal["repair_id"].as_str().unwrap()
    );
    let (status, body) = call(&srv, "POST", &uri, json!({"expected_graph_revision":1})).await;
    assert_eq!(status, 200, "{body}");
    let attempts = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let rows = srv
                .db
                .load_node_executions(&g, &TaskNodeId::new("collect").unwrap())
                .unwrap();
            if rows.len() == 2
                && rows
                    .iter()
                    .any(|row| row.attempt == 2 && row.status == NodeExecutionStatus::Succeeded)
            {
                break rows;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("repair must dispatch an actual second attempt");
    assert_eq!(
        attempts
            .iter()
            .find(|row| row.id == failed.id)
            .unwrap()
            .status,
        NodeExecutionStatus::Stale
    );
    let (status, again) = call(&srv, "POST", &uri, json!({"expected_graph_revision":1})).await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(
        srv.db
            .load_node_executions(&g, &TaskNodeId::new("collect").unwrap())
            .unwrap()
            .len(),
        2
    );
}
