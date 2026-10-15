//! HTTP routes for ProofLoop drafts and compilations.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/00-contracts-and-ownership.md §5

mod evaluations;
mod checkpoints;
mod repairs;
pub use evaluations::*;
pub use repairs::*;

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
    evidence::{collect_evidence, reload_snapshot_contents},
    repair::{classify_failure, validate_repair},
};

#[derive(Debug, Deserialize)]
pub struct ListDraftsQuery {
    pub workspace_id: Option<String>,
}

fn proof_error_response(error: ProofError) -> Response {
    let status = match error.code.as_str() {
        "proofloop_stale_revision" | "draft_already_confirmed" => StatusCode::CONFLICT,
        "draft_not_found" => StatusCode::NOT_FOUND,
        "aborted" | "proofloop_invalid_planning_options" | "proofloop_candidate_not_found" | "proofloop_candidate_selection_required" => StatusCode::BAD_REQUEST,
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
            "planning_usage": error.planning_usage,
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

    let mut model_config = server.effective_model_config();
    let budget = crate::modules::task::proofloop::Budget::default();
    model_config.max_tokens = model_config.max_tokens.min(budget.max_total_tokens as u32);
    model_config.invoke_timeout_ms = model_config.invoke_timeout_ms.min(budget.max_wall_time_ms);
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

pub async fn graph_goal_handler(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
) -> Response {
    match server.db.confirmed_proof_draft_for_graph(&graph_id) {
        Ok(Some(draft)) => {
            let usage = server.proofloop_budget.usage(&draft.contract);
            let remaining = server.proofloop_budget.check(&draft.contract, now());
            let budget_error = usage
                .as_ref()
                .err()
                .or_else(|| remaining.as_ref().err())
                .cloned();
            (StatusCode::OK, Json(json!({"goal_id":draft.contract.goal_id,"goal_version":draft.contract.version,"graph_id":draft.graph_id,"contract":draft.contract,
                "time_usage":usage.ok(),"remaining_ms":remaining.ok().map(|duration|duration.as_millis() as u64),"budget_error":budget_error}))).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"code":"goal_not_found","message":"此任务图尚未关联证行目标"})),
        )
            .into_response(),
        Err(error) => proof_error_response(ProofError::new("db_error", error.to_string())),
    }
}

pub async fn evaluation_detail_handler(
    State(server): State<Arc<AppServer>>,
    Path(report_id): Path<String>,
) -> Response {
    match server.db.get_acceptance_report(&report_id) {
        Ok(Some((report, snapshot))) => (
            StatusCode::OK,
            Json(json!({"report":report,"snapshot":snapshot})),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"code":"report_not_found","message":"验收报告不存在"})),
        )
            .into_response(),
        Err(error) => proof_error_response(ProofError::new("db_error", error.to_string())),
    }
}

pub fn proofloop_router() -> axum::Router<Arc<AppServer>> {
    use axum::routing::{get, post};
    axum::Router::new()
        // ProofLoop engine routes (I1)
        .route("/api/task-world/proofloop/graphs/:id/file-checkpoints", post(checkpoints::capture_files_handler))
        .route("/api/task-world/proofloop/graphs/:id/file-checkpoints/:checkpoint/preview", get(checkpoints::preview_files_handler))
        .route("/api/task-world/proofloop/graphs/:id/file-checkpoints/:checkpoint/restore", post(checkpoints::restore_files_handler))
        .route(
            "/api/task-world/proofloop/graphs/:id/goal",
            get(graph_goal_handler),
        )
        .route(
            "/api/task-world/proofloop/evaluations/:id",
            get(evaluation_detail_handler),
        )
        .route(
            "/api/task-world/proofloop/drafts",
            get(list_drafts_handler).post(create_draft_handler),
        )
        .route(
            "/api/task-world/proofloop/drafts/:id",
            get(get_draft_handler).put(update_draft_handler),
        )
        .route(
            "/api/task-world/proofloop/drafts/:id/confirm",
            post(confirm_draft_handler),
        )
        .route(
            "/api/task-world/proofloop/goals/:id/evaluations",
            get(list_goal_evaluations_handler).post(evaluate_goal_handler),
        )
        .route(
            "/api/task-world/proofloop/evaluations/:id/human-review",
            post(submit_human_review_handler),
        )
        .route(
            "/api/task-world/proofloop/goals/:id/repairs",
            post(propose_repair_handler),
        )
        .route(
            "/api/task-world/proofloop/repairs/:id/apply",
            post(apply_repair_handler),
        )
}
