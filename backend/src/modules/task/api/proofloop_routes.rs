//! HTTP routes for ProofLoop drafts and compilations.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/00-contracts-and-ownership.md §5

mod checkpoints;
mod evaluations;
mod records;
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
        "proofloop_invalid_goal_requirements"
        | "aborted"
        | "proofloop_invalid_planning_options"
        | "proofloop_candidate_not_found"
        | "proofloop_candidate_selection_required" => StatusCode::BAD_REQUEST,
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
    use crate::modules::task::proofloop::{
        candidate_planner::candidate_planning_context_chars,
        enhancement_contracts::PlanningStrategy,
        routing::{build_ranked_snapshot, rank_capabilities},
    };
    if let Err(error) = request.planning.validate() {
        return proof_error_response(error);
    }
    let planning_goal =
        match crate::modules::task::proofloop::enhancement_contracts::planning_goal_text(
            &request.user_goal,
            request.requirements.as_ref(),
        ) {
            Ok(v) => v,
            Err(e) => return proof_error_response(e),
        };
    let recommendations = if request.planning.strategy == PlanningStrategy::Experience {
        match server.managed_skill_store() {
            Some(store) => match crate::modules::memory_skill::MemorySkillService::new(
                server.db.clone_connection(),
                store,
            )
            .planning_recommendations(&request.user_goal, 3)
            {
                Ok(value) => value,
                Err(e) => {
                    return proof_error_response(ProofError::new(
                        "proofloop_skill_recommendation_unavailable",
                        e,
                    ))
                }
            },
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let registry = server.build_capability_registry().await;
    let descriptors = registry.list();
    let routes = if request.planning.strategy == PlanningStrategy::Existing {
        Vec::new()
    } else {
        rank_capabilities(
            &request.user_goal,
            &descriptors,
            request.allowed_capability_ids.as_deref(),
            &std::collections::HashMap::new(),
        )
    };
    let snapshot = if request.planning.strategy == PlanningStrategy::Existing {
        build_capability_snapshot(
            &descriptors,
            request.allowed_capability_ids.as_deref(),
            now(),
        )
    } else {
        let overhead =
            candidate_planning_context_chars(&planning_goal, &recommendations, &request.planning);
        let remaining = crate::modules::task::proofloop::planner::MAX_PLANNING_PROMPT_CHARS
            .saturating_sub(overhead);
        match build_ranked_snapshot(
            &descriptors,
            &routes,
            request.allowed_capability_ids.as_deref(),
            now(),
            remaining,
        ) {
            Ok(value) => value,
            Err(error) => return proof_error_response(error),
        }
    };
    let mut model_config = server.effective_model_config();
    let budget = request
        .requirements
        .as_ref()
        .and_then(|requirements| requirements.budget)
        .unwrap_or_default();
    model_config.max_tokens = model_config.max_tokens.min(budget.max_total_tokens as u32);
    model_config.invoke_timeout_ms = model_config.invoke_timeout_ms.min(budget.max_wall_time_ms);
    let usage = Arc::new(crate::integrations::llm::usage::DatabaseUsageRecorder::new(
        server.db.clone_connection(),
        model_config.name.clone(),
        model_config.provider.clone(),
        model_config.name.clone(),
        "proofloop-planning",
    ));
    let client = LlmClient::new_with_usage_recorder(
        &model_config,
        Arc::clone(&server.secret_resolver),
        Some(usage),
    );
    let cancel = CancellationToken::new();
    let service = DraftService::new(server.db.clone());
    match service
        .create_draft_with_context(
            request,
            snapshot,
            Some(&client),
            &cancel,
            now(),
            routes,
            recommendations.clone(),
        )
        .await
    {
        Ok(draft) => {
            if !recommendations.is_empty() {
                if let Some(store) = server.managed_skill_store() {
                    if let Err(e) = crate::modules::memory_skill::MemorySkillService::new(
                        server.db.clone_connection(),
                        store,
                    )
                    .record_planning_skill_usage(
                        &draft.contract.goal_id,
                        draft.contract.version,
                        &draft.draft_id,
                        &recommendations,
                        now(),
                    ) {
                        return proof_error_response(ProofError::new(
                            "proofloop_skill_feedback_unavailable",
                            e,
                        ));
                    }
                }
            }
            (StatusCode::CREATED, Json(draft)).into_response()
        }
        Err(error) => proof_error_response(error),
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

pub async fn select_candidate_handler(
    State(server): State<Arc<AppServer>>,
    Path(draft_id): Path<String>,
    Json(request): Json<
        crate::modules::task::proofloop::enhancement_contracts::SelectCandidateRequest,
    >,
) -> Response {
    let service = DraftService::new(server.db.clone());
    let draft = match service.get_draft(&draft_id) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return proof_error_response(ProofError::new("draft_not_found", "任务草稿不存在"))
        }
        Err(e) => return proof_error_response(e),
    };
    let registry = server.build_capability_registry().await;
    let snapshot = build_capability_snapshot(
        &registry.list(),
        Some(&draft.contract.allowed_capability_ids),
        now(),
    );
    match service.select_candidate(&draft_id, request, snapshot, now()) {
        Ok(draft) => Json(draft).into_response(),
        Err(error) => proof_error_response(error),
    }
}

pub async fn confirm_draft_handler(
    State(server): State<Arc<AppServer>>,
    Path(draft_id): Path<String>,
    Json(request): Json<ConfirmProofDraftRequest>,
) -> Response {
    let service = DraftService::new(server.db.clone());
    let draft = match service.get_draft(&draft_id) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return proof_error_response(ProofError::new("draft_not_found", "任务草稿不存在"))
        }
        Err(e) => return proof_error_response(e),
    };
    let registry = server.build_capability_registry().await;
    let snapshot = build_capability_snapshot(
        &registry.list(),
        Some(&draft.contract.allowed_capability_ids),
        now(),
    );

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
            let model_usage = server
                .db
                .combined_goal_model_usage(&draft.contract, &draft.planning_usage)
                .ok();
            (StatusCode::OK, Json(json!({"goal_id":draft.contract.goal_id,"goal_version":draft.contract.version,"graph_id":draft.graph_id,"contract":draft.contract,
                "model_usage":model_usage,"time_usage":usage.ok(),"remaining_ms":remaining.ok().map(|duration|duration.as_millis() as u64),"budget_error":budget_error}))).into_response()
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
        .route(
            "/api/task-world/proofloop/graphs/:id/execution-record",
            get(records::execution_record_handler),
        )
        .route(
            "/api/task-world/proofloop/graphs/:id/file-checkpoints",
            post(checkpoints::capture_files_handler),
        )
        .route(
            "/api/task-world/proofloop/graphs/:id/file-checkpoints/:checkpoint/preview",
            get(checkpoints::preview_files_handler),
        )
        .route(
            "/api/task-world/proofloop/graphs/:id/file-checkpoints/:checkpoint/restore",
            post(checkpoints::restore_files_handler),
        )
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
            "/api/task-world/proofloop/drafts/:id/select-candidate",
            post(select_candidate_handler),
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
