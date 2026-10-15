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

fn prompt_input_upper_bound(
    strategy: crate::modules::task::proofloop::enhancement_contracts::PlanningStrategy,
    goal: &str,
    snapshot: &crate::modules::task::proofloop::CapabilitySnapshot,
    recommendations: &[crate::modules::task::proofloop::enhancement_contracts::SkillRecommendation],
    options: &crate::modules::task::proofloop::enhancement_contracts::PlanningOptions,
) -> Result<u64, ProofError> {
    let messages = match strategy {
        crate::modules::task::proofloop::enhancement_contracts::PlanningStrategy::Existing => {
            crate::modules::task::proofloop::planner::build_planning_messages(goal, snapshot)
        }
        crate::modules::task::proofloop::enhancement_contracts::PlanningStrategy::Ranked
        | crate::modules::task::proofloop::enhancement_contracts::PlanningStrategy::Experience => {
            crate::modules::task::proofloop::candidate_planner::build_candidate_messages(
                goal,
                snapshot,
                recommendations,
                options,
            )
        }
    };
    crate::integrations::llm::client::conservative_input_token_upper_bound(&messages, &[])
        .ok_or_else(|| {
            ProofError::new(
                "proofloop_usage_unavailable",
                "无法保守估算规划输入用量，未调用模型",
            )
        })
}

fn prompt_fits_budget(input_tokens: u64, total_budget: u64, output_reserve: u32) -> bool {
    input_tokens
        .checked_add(u64::from(output_reserve))
        .is_some_and(|needed| needed <= total_budget)
}

fn fit_ranked_snapshot_to_budget(
    descriptors: &[crate::modules::capability::CapabilityDescriptor],
    routes: &[crate::modules::task::proofloop::enhancement_contracts::RouteScore],
    explicit_ids: Option<&[String]>,
    now_ms: i64,
    goal: &str,
    recommendations: &[crate::modules::task::proofloop::enhancement_contracts::SkillRecommendation],
    options: &crate::modules::task::proofloop::enhancement_contracts::PlanningOptions,
    total_budget: u64,
    output_reserve: u32,
) -> Result<crate::modules::task::proofloop::CapabilitySnapshot, ProofError> {
    use crate::modules::task::proofloop::{
        candidate_planner::candidate_planning_context_chars, planner::MAX_PLANNING_PROMPT_CHARS,
        routing::build_ranked_snapshot,
    };
    let max_snapshot_chars = MAX_PLANNING_PROMPT_CHARS.saturating_sub(
        candidate_planning_context_chars(goal, recommendations, options),
    );
    if explicit_ids.is_some() {
        let snapshot = build_ranked_snapshot(
            descriptors,
            routes,
            explicit_ids,
            now_ms,
            max_snapshot_chars,
        )?;
        let input =
            prompt_input_upper_bound(options.strategy, goal, &snapshot, recommendations, options)?;
        if !prompt_fits_budget(input, total_budget, output_reserve) {
            return Err(ProofError::new(
                "proofloop_capability_selection_incomplete",
                "显式选择的能力无法同时满足规划输入和输出 token 预算",
            ));
        }
        return Ok(snapshot);
    }

    let (mut low, mut high) = (0usize, max_snapshot_chars);
    let mut best = None;
    while low <= high {
        let middle = low + (high - low) / 2;
        let snapshot = build_ranked_snapshot(descriptors, routes, None, now_ms, middle)?;
        let input =
            prompt_input_upper_bound(options.strategy, goal, &snapshot, recommendations, options)?;
        if !snapshot.capabilities.is_empty()
            && prompt_fits_budget(input, total_budget, output_reserve)
        {
            best = Some(snapshot);
            low = middle.saturating_add(1);
        } else if middle == 0 {
            break;
        } else {
            high = middle - 1;
        }
    }
    best.ok_or_else(|| {
        ProofError::new(
            "proofloop_budget_exhausted",
            "规划目标和至少一项能力无法在保留输出额度后装入 token 预算",
        )
    })
}

fn fit_existing_snapshot_to_budget(
    descriptors: &[crate::modules::capability::CapabilityDescriptor],
    explicit_ids: Option<&[String]>,
    now_ms: i64,
    goal: &str,
    options: &crate::modules::task::proofloop::enhancement_contracts::PlanningOptions,
    total_budget: u64,
    output_reserve: u32,
) -> Result<crate::modules::task::proofloop::CapabilitySnapshot, ProofError> {
    use crate::modules::task::proofloop::{
        capabilities::{build_capability_snapshot_with_char_limit, MAX_SNAPSHOT_CONTEXT_CHARS},
        planner::MAX_PLANNING_PROMPT_CHARS,
    };
    if explicit_ids.is_some() {
        let snapshot = build_capability_snapshot_with_char_limit(
            descriptors,
            explicit_ids,
            now_ms,
            MAX_SNAPSHOT_CONTEXT_CHARS,
        );
        if explicit_ids.is_some_and(|ids| {
            ids.iter().any(|id| {
                !snapshot
                    .capabilities
                    .iter()
                    .any(|capability| capability.capability_id == *id)
            })
        }) {
            return Err(ProofError::new(
                "proofloop_capability_selection_incomplete",
                "显式选择的能力不能完整装入规划输入",
            ));
        }
        let input = prompt_input_upper_bound(options.strategy, goal, &snapshot, &[], options)?;
        if !prompt_fits_budget(input, total_budget, output_reserve) {
            return Err(ProofError::new(
                "proofloop_capability_selection_incomplete",
                "显式选择的能力无法同时满足规划输入和输出 token 预算",
            ));
        }
        return Ok(snapshot);
    }

    let (mut low, mut high) = (0usize, MAX_SNAPSHOT_CONTEXT_CHARS);
    let mut best = None;
    while low <= high {
        let middle = low + (high - low) / 2;
        let snapshot = build_capability_snapshot_with_char_limit(descriptors, None, now_ms, middle);
        let chars = crate::modules::task::proofloop::planner::planning_prompt_char_count(
            &crate::modules::task::proofloop::planner::build_planning_messages(goal, &snapshot),
        );
        let input = prompt_input_upper_bound(options.strategy, goal, &snapshot, &[], options)?;
        if !snapshot.capabilities.is_empty()
            && chars <= MAX_PLANNING_PROMPT_CHARS
            && prompt_fits_budget(input, total_budget, output_reserve)
        {
            best = Some(snapshot);
            low = middle.saturating_add(1);
        } else if middle == 0 {
            break;
        } else {
            high = middle - 1;
        }
    }
    best.ok_or_else(|| {
        ProofError::new(
            "proofloop_budget_exhausted",
            "规划目标和至少一项能力无法在保留输出额度后装入 token 预算",
        )
    })
}

pub async fn create_draft_handler(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<CreateProofDraftRequest>,
) -> Response {
    use crate::modules::task::proofloop::{
        enhancement_contracts::PlanningStrategy, routing::rank_capabilities,
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
    let budget = request
        .requirements
        .as_ref()
        .and_then(|requirements| requirements.budget)
        .unwrap_or_default();
    let mut model_config = server.effective_model_config();
    let output_reserve = model_config.max_tokens.min(
        (budget.max_total_tokens / 2)
            .max(1)
            .min(u64::from(u32::MAX)) as u32,
    );
    model_config.invoke_timeout_ms = model_config.invoke_timeout_ms.min(budget.max_wall_time_ms);
    let routes = if request.planning.strategy == PlanningStrategy::Existing {
        Vec::new()
    } else {
        rank_capabilities(
            &planning_goal,
            &descriptors,
            request.allowed_capability_ids.as_deref(),
            &std::collections::HashMap::new(),
        )
    };
    let snapshot = if request.planning.strategy == PlanningStrategy::Existing {
        match fit_existing_snapshot_to_budget(
            &descriptors,
            request.allowed_capability_ids.as_deref(),
            now(),
            &planning_goal,
            &request.planning,
            budget.max_total_tokens,
            output_reserve,
        ) {
            Ok(value) => value,
            Err(error) => return proof_error_response(error),
        }
    } else {
        match fit_ranked_snapshot_to_budget(
            &descriptors,
            &routes,
            request.allowed_capability_ids.as_deref(),
            now(),
            &planning_goal,
            &recommendations,
            &request.planning,
            budget.max_total_tokens,
            output_reserve,
        ) {
            Ok(value) => value,
            Err(error) => return proof_error_response(error),
        }
    };
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
        Ok(Some((report, snapshot))) => {
            let contract = match server
                .db
                .load_goal_version(&report.goal_id, report.goal_version)
            {
                Ok(value) => value,
                Err(error) => {
                    return proof_error_response(ProofError::new("db_error", error.to_string()))
                }
            };
            let artifacts =
                crate::modules::task::artifact::ArtifactService::new(server.db.clone_connection());
            let current_evidence = contract
                .as_ref()
                .map(|contract| {
                    current_deliverable_evidence(
                        &server.db,
                        &artifacts,
                        contract,
                        &snapshot,
                        &server.workspace_root,
                    )
                })
                .unwrap_or_default();
            (
                StatusCode::OK,
                Json(json!({
                    "report": report,
                    "snapshot": snapshot,
                    "current_evidence": current_evidence,
                })),
            )
                .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"code":"report_not_found","message":"验收报告不存在"})),
        )
            .into_response(),
        Err(error) => proof_error_response(ProofError::new("db_error", error.to_string())),
    }
}

#[derive(serde::Serialize)]
struct CurrentDeliverableEvidence {
    deliverable_id: String,
    artifact_id: Option<String>,
    status: &'static str,
    reason: &'static str,
}

fn current_deliverable_evidence(
    db: &crate::db::Database,
    artifacts: &crate::modules::task::artifact::ArtifactService,
    contract: &crate::modules::task::proofloop::GoalContract,
    snapshot: &crate::modules::task::proofloop::EvidenceSnapshot,
    allowed_root: &str,
) -> Vec<CurrentDeliverableEvidence> {
    contract
        .deliverables
        .iter()
        .map(|deliverable| {
            let mapping = snapshot
                .deliverables
                .iter()
                .find(|mapping| mapping.deliverable_id == deliverable.id);
            let Some(mapping) = mapping else {
                return CurrentDeliverableEvidence {
                    deliverable_id: deliverable.id.clone(),
                    artifact_id: None,
                    status: "stale",
                    reason: "此交付物没有当前验收文件映射。",
                };
            };
            let artifact_id = mapping.artifact_id.clone();
            let parsed_id = match crate::modules::task::model::ArtifactId::new(&artifact_id) {
                Ok(id) => id,
                Err(_) => {
                    return CurrentDeliverableEvidence {
                        deliverable_id: deliverable.id.clone(),
                        artifact_id: Some(artifact_id),
                        status: "stale",
                        reason: "当前交付物标识无效。",
                    }
                }
            };
            let read = artifacts.read_bytes(&parsed_id, allowed_root);
            let provenance = db.artifact_provenance(&artifact_id);
            let (Ok((artifact, _bytes)), Ok(Some(provenance))) = (read, provenance) else {
                return CurrentDeliverableEvidence {
                    deliverable_id: deliverable.id.clone(),
                    artifact_id: Some(artifact_id),
                    status: "stale",
                    reason: "实际交付物文件缺失、被改写或来源记录不可用。",
                };
            };
            let captured = snapshot
                .evidence
                .iter()
                .find(|evidence| evidence.artifact_id.as_deref() == Some(artifact_id.as_str()));
            let source_matches = matches!(
                &provenance.source,
                crate::modules::task::artifact::ArtifactSource::Node {
                    graph_id,
                    node_id,
                    execution_id,
                    workspace_id,
                } if graph_id == &snapshot.graph_id
                    && workspace_id == &contract.workspace_id
                    && artifact.workspace_id.as_str() == contract.workspace_id
                    && artifact.task_execution_id.as_str() == execution_id.as_str()
                    && snapshot.executions.iter().any(|execution| {
                        execution.graph_id.as_str() == snapshot.graph_id
                            && execution.node_id.as_str() == node_id.as_str()
                            && execution.execution_id.as_str() == execution_id.as_str()
                    })
            );
            let digest_matches =
                captured.is_some_and(|evidence| evidence.digest == provenance.sha256);
            let current = source_matches && digest_matches;
            CurrentDeliverableEvidence {
                deliverable_id: deliverable.id.clone(),
                artifact_id: Some(artifact_id),
                status: if current { "current" } else { "stale" },
                reason: if current {
                    "实际文件字节、验收摘要与执行来源仍匹配。"
                } else {
                    "交付物字节或执行来源已变化，请重新验收。"
                },
            }
        })
        .collect()
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
