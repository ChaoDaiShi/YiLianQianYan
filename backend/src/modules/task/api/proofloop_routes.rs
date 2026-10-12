//! HTTP routes for ProofLoop drafts and compilations.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/00-contracts-and-ownership.md §5

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::app::state::AppServer;
use crate::integrations::llm::client::LlmClient;
use crate::modules::task::proofloop::{
    acceptance::evaluate_goal,
    capabilities::build_capability_snapshot,
    contracts::{
        ApplyRepairRequest, ApplyRepairResponse, ConfirmProofDraftRequest, CreateProofDraftRequest,
        EvaluateGoalRequest, HumanReview, ProofError, ProposeRepairRequest,
        SubmitHumanReviewRequest, UpdateProofDraftRequest,
    },
    drafts::DraftService,
    evidence::collect_evidence,
    repair::{apply_repair, classify_failure, validate_repair},
};

#[derive(Debug, Deserialize)]
pub struct ListDraftsQuery {
    pub workspace_id: Option<String>,
}

fn proof_error_response(error: ProofError) -> Response {
    let status = match error.code.as_str() {
        "proofloop_stale_revision" => StatusCode::CONFLICT,
        "draft_not_found" => StatusCode::NOT_FOUND,
        "aborted" => StatusCode::BAD_REQUEST,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    };
    (
        status,
        Json(json!({
            "code": error.code,
            "error": error.code,
            "message": error.message,
            "node_id": error.node_id,
            "target_path": error.target_path,
        })),
    )
        .into_response()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub async fn create_draft_handler(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<CreateProofDraftRequest>,
) -> Response {
    let registry = server.build_capability_registry().await;
    let descriptors = registry.list();
    let snapshot = build_capability_snapshot(
        &descriptors,
        request.allowed_capability_ids.as_deref(),
        now(),
    );

    let model_config = server.effective_model_config();
    let llm_client = LlmClient::new(&model_config, Arc::clone(&server.secret_resolver));
    let cancel = CancellationToken::new();

    let service = DraftService::new(server.db.clone());
    match service
        .create_draft(request, snapshot, Some(&llm_client), &cancel, now())
        .await
    {
        Ok(draft) => (StatusCode::CREATED, Json(draft)).into_response(),
        Err(err) => proof_error_response(err),
    }
}

pub async fn list_drafts_handler(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<ListDraftsQuery>,
) -> Response {
    let service = DraftService::new(server.db.clone());
    match service.list_drafts(query.workspace_id.as_deref()) {
        Ok(drafts) => (StatusCode::OK, Json(json!({ "drafts": drafts }))).into_response(),
        Err(err) => proof_error_response(err),
    }
}

pub async fn get_draft_handler(
    State(server): State<Arc<AppServer>>,
    Path(draft_id): Path<String>,
) -> Response {
    let service = DraftService::new(server.db.clone());
    match service.get_draft(&draft_id) {
        Ok(Some(draft)) => (StatusCode::OK, Json(draft)).into_response(),
        Ok(None) => {
            proof_error_response(ProofError::new("draft_not_found", "指定的任务草稿不存在"))
        }
        Err(err) => proof_error_response(err),
    }
}

pub async fn update_draft_handler(
    State(server): State<Arc<AppServer>>,
    Path(draft_id): Path<String>,
    Json(request): Json<UpdateProofDraftRequest>,
) -> Response {
    let registry = server.build_capability_registry().await;
    let descriptors = registry.list();
    let snapshot = build_capability_snapshot(
        &descriptors,
        Some(&request.contract.allowed_capability_ids),
        now(),
    );

    let service = DraftService::new(server.db.clone());
    match service.update_draft(&draft_id, request, snapshot, now()) {
        Ok(draft) => (StatusCode::OK, Json(draft)).into_response(),
        Err(err) => proof_error_response(err),
    }
}

pub async fn confirm_draft_handler(
    State(server): State<Arc<AppServer>>,
    Path(draft_id): Path<String>,
    Json(request): Json<ConfirmProofDraftRequest>,
) -> Response {
    let registry = server.build_capability_registry().await;
    let descriptors = registry.list();
    let snapshot = build_capability_snapshot(&descriptors, None, now());

    let service = DraftService::new(server.db.clone());
    match service.confirm_draft(
        &server.task_world,
        &draft_id,
        request.expected_revision,
        &snapshot,
        now(),
    ) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => proof_error_response(err),
    }
}

pub async fn list_goal_evaluations_handler(
    State(server): State<Arc<AppServer>>,
    Path(goal_id): Path<String>,
) -> Response {
    match server.db.list_acceptance_reports(&goal_id) {
        Ok(reports) => (StatusCode::OK, Json(json!({ "reports": reports }))).into_response(),
        Err(err) => proof_error_response(ProofError::new("db_error", err.to_string())),
    }
}

pub async fn evaluate_goal_handler(
    State(server): State<Arc<AppServer>>,
    Path(goal_id): Path<String>,
    Json(request): Json<EvaluateGoalRequest>,
) -> Response {
    let now_ms = now();
    let contract = match server.db.load_goal_version(&goal_id, request.goal_version) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return proof_error_response(ProofError::new(
                "goal_not_found",
                format!("未找到目标 '{}' 版本 {}", goal_id, request.goal_version),
            ));
        }
        Err(err) => return proof_error_response(ProofError::new("db_error", err.to_string())),
    };

    let allowed_root = server.workspace_root.clone();
    let artifact_service =
        crate::modules::task::artifact::ArtifactService::new(server.db.clone_connection());
    let (snapshot, evidence_content) = match collect_evidence(
        &server.db,
        &artifact_service,
        &contract,
        &goal_id,
        request.graph_revision,
        &request.execution_ids,
        &allowed_root,
        now_ms,
    ) {
        Ok(res) => res,
        Err(err) => return proof_error_response(err),
    };

    let existing_reviews = Vec::new();
    let report = match evaluate_goal(
        &contract,
        &snapshot,
        &evidence_content,
        &existing_reviews,
        now_ms,
    ) {
        Ok(r) => r,
        Err(err) => return proof_error_response(err),
    };

    if let Err(err) = server.db.save_acceptance_report(&report, &snapshot, now_ms) {
        return proof_error_response(ProofError::new("db_error", err.to_string()));
    }

    (StatusCode::OK, Json(report)).into_response()
}

pub async fn submit_human_review_handler(
    State(server): State<Arc<AppServer>>,
    Path(evaluation_id): Path<String>,
    Json(request): Json<SubmitHumanReviewRequest>,
) -> Response {
    let now_ms = now();
    let (mut report, snapshot) = match server.db.get_acceptance_report(&evaluation_id) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return proof_error_response(ProofError::new(
                "report_not_found",
                format!("未找到验收报告 '{}'", evaluation_id),
            ));
        }
        Err(err) => return proof_error_response(ProofError::new("db_error", err.to_string())),
    };

    let contract = match server
        .db
        .load_goal_version(&report.goal_id, report.goal_version)
    {
        Ok(Some(c)) => c,
        _ => {
            return proof_error_response(ProofError::new("goal_not_found", "未找到对应的目标契约"));
        }
    };

    let human_review = HumanReview {
        criterion_id: request.criterion_id.clone(),
        decision: request.decision,
        reason: request.reason,
        reviewer_subject: "operator".to_string(),
        reviewed_at_ms: now_ms,
    };

    report
        .human_reviews
        .retain(|r| r.criterion_id != request.criterion_id);
    report.human_reviews.push(human_review);

    let dummy_content = std::collections::HashMap::new();
    let updated_report = match evaluate_goal(
        &contract,
        &snapshot,
        &dummy_content,
        &report.human_reviews,
        now_ms,
    ) {
        Ok(r) => r,
        Err(err) => return proof_error_response(err),
    };

    if let Err(err) = server
        .db
        .save_acceptance_report(&updated_report, &snapshot, now_ms)
    {
        return proof_error_response(ProofError::new("db_error", err.to_string()));
    }

    (StatusCode::OK, Json(updated_report)).into_response()
}

pub async fn propose_repair_handler(
    State(server): State<Arc<AppServer>>,
    Path(_goal_id): Path<String>,
    Json(request): Json<ProposeRepairRequest>,
) -> Response {
    let now_ms = now();
    let (report, _snapshot) = match server.db.get_acceptance_report(&request.report_id) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return proof_error_response(ProofError::new(
                "report_not_found",
                format!("未找到验收报告 '{}'", request.report_id),
            ));
        }
        Err(err) => return proof_error_response(ProofError::new("db_error", err.to_string())),
    };

    let contract = match server
        .db
        .load_goal_version(&report.goal_id, report.goal_version)
    {
        Ok(Some(c)) => c,
        _ => return proof_error_response(ProofError::new("goal_not_found", "未找到目标契约")),
    };

    let failed_exec_id = match request.failure_execution_ids.first() {
        Some(id) => id,
        None => {
            return proof_error_response(ProofError::new(
                "no_failed_executions",
                "缺少失败执行记录",
            ));
        }
    };

    let exec_id = match crate::modules::task::execution::NodeExecutionId::new(failed_exec_id) {
        Ok(id) => id,
        Err(err) => {
            return proof_error_response(ProofError::new("invalid_execution_id", err.to_string()))
        }
    };

    let failed_exec = match server.db.load_node_execution(&exec_id) {
        Ok(Some(e)) => e,
        _ => return proof_error_response(ProofError::new("execution_not_found", "未找到执行记录")),
    };

    let registry = server.build_capability_registry().await;
    let descriptors = registry.list();
    let cap_snapshot = build_capability_snapshot(&descriptors, None, now_ms);

    let failed_criterion = report
        .results
        .iter()
        .find(|r| r.status == crate::modules::task::proofloop::contracts::CriterionStatus::Fail);

    let proposal = match classify_failure(
        &failed_exec,
        failed_criterion,
        &cap_snapshot,
        &contract,
        0,
        report.graph_revision,
        report.report_id.clone(),
    ) {
        Ok(p) => p,
        Err(err) => return proof_error_response(err),
    };

    if let Err(err) = server
        .db
        .save_repair_proposal(&proposal, "proposed", now_ms)
    {
        return proof_error_response(ProofError::new("db_error", err.to_string()));
    }

    (StatusCode::OK, Json(proposal)).into_response()
}

pub async fn apply_repair_handler(
    State(server): State<Arc<AppServer>>,
    Path(repair_id): Path<String>,
    Json(request): Json<ApplyRepairRequest>,
) -> Response {
    let proposal = match server.db.get_repair_proposal(&repair_id) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return proof_error_response(ProofError::new("repair_not_found", "未找到修复提案"))
        }
        Err(err) => return proof_error_response(ProofError::new("db_error", err.to_string())),
    };

    let contract = match server
        .db
        .load_goal_version(&proposal.goal_id, proposal.goal_version)
    {
        Ok(Some(c)) => c,
        _ => return proof_error_response(ProofError::new("goal_not_found", "未找到目标契约")),
    };

    let graph_id = match crate::modules::task::TaskGraphId::new(&proposal.goal_id) {
        Ok(id) => id,
        Err(err) => {
            return proof_error_response(ProofError::new("invalid_graph_id", err.to_string()))
        }
    };

    let graph = match server.task_world.get_graph(&graph_id) {
        Some(g) => g,
        None => return proof_error_response(ProofError::new("graph_not_found", "未找到任务图")),
    };

    let registry = server.build_capability_registry().await;
    let descriptors = registry.list();
    let snapshot = build_capability_snapshot(&descriptors, None, now());

    let patch = match validate_repair(
        &proposal,
        &graph,
        request.expected_graph_revision,
        &snapshot,
        &contract,
    ) {
        Ok(p) => p,
        Err(err) => return proof_error_response(err),
    };

    let (updated_graph, _new_revision, affected_node_ids) =
        match apply_repair(patch, request.expected_graph_revision) {
            Ok(res) => res,
            Err(err) => return proof_error_response(err),
        };

    let mut current_rev = request.expected_graph_revision;
    let now_ms = now();
    for node in updated_graph.nodes {
        if affected_node_ids.contains(&node.id.as_str().to_string()) {
            match server
                .task_world
                .update_node(&graph_id, node, current_rev, now_ms)
            {
                Ok(next_g) => {
                    current_rev = next_g.revision.value();
                }
                Err(err) => {
                    return proof_error_response(ProofError::new(
                        "update_node_failed",
                        err.to_string(),
                    ))
                }
            }
        }
    }

    let _ = server
        .db
        .update_repair_status(&repair_id, "applied", Some(current_rev));

    (
        StatusCode::OK,
        Json(ApplyRepairResponse {
            graph_revision: current_rev,
            affected_node_ids,
        }),
    )
        .into_response()
}
