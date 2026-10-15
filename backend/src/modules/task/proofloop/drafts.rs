//! ProofDraft lifecycle management: creation, update, compilation and confirmation.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/01-capability-and-compiler.md §6

use tokio_util::sync::CancellationToken;

use crate::db::{Database, DraftWriteConflict, ProofLoopDbError};
use crate::integrations::llm::client::LlmClient;
use crate::modules::task::{runtime::TaskWorldRuntime, task_graph::TaskGraphId};

use super::compiler::compile_candidate;
use super::contracts::{
    CapabilitySnapshot, CompileStatus, ConfirmProofDraftResponse, CreateProofDraftRequest,
    DraftStatus, ProofDraft, ProofDraftSummary, ProofError, UpdateProofDraftRequest,
};
use super::enhancement_contracts::{
    PlanOption, PlanningReview, PlanningStrategy, RouteScore, SelectCandidateRequest,
    SkillRecommendation,
};
mod confirmations;
use super::{
    candidate_planner::propose_candidates_with_usage,
    plan_selection::{score_candidate, select_recommended},
};

fn draft_write_error(error: ProofLoopDbError) -> ProofError {
    match error {
        ProofLoopDbError::DraftConflict(DraftWriteConflict::NotFound) => {
            ProofError::new("draft_not_found", "指定的任务草稿不存在")
        }
        ProofLoopDbError::DraftConflict(DraftWriteConflict::AlreadyConfirmed) => ProofError::new(
            "draft_already_confirmed",
            "已确认的草稿不可直接修改，请基于新目标创建草稿",
        ),
        ProofLoopDbError::DraftConflict(DraftWriteConflict::StaleRevision { expected, actual }) => {
            ProofError::new(
                "proofloop_stale_revision",
                format!("草稿版本冲突：预期版本 {expected}，实际当前版本 {actual}"),
            )
        }
        error => ProofError::new("draft_save_failed", error.to_string()),
    }
}

fn next_draft_revision(revision: u32) -> Result<u32, ProofError> {
    revision
        .checked_add(1)
        .ok_or_else(|| ProofError::new("draft_revision_overflow", "草稿版本号已到上限"))
}

fn confirmed_response(draft: ProofDraft) -> Option<ConfirmProofDraftResponse> {
    if draft.status != DraftStatus::Confirmed {
        return None;
    }
    let graph_revision = draft.confirmed_graph_revision?;
    Some(ConfirmProofDraftResponse {
        graph_id: draft.graph_id,
        graph_revision,
        goal_id: draft.contract.goal_id,
        goal_version: draft.contract.version,
    })
}

pub struct DraftService {
    db: Database,
}

impl DraftService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Create a new draft with AI proposed candidate plan and deterministic compilation.
    pub async fn create_draft(
        &self,
        request: CreateProofDraftRequest,
        snapshot: CapabilitySnapshot,
        llm_client: Option<&LlmClient>,
        cancel: &CancellationToken,
        now: i64,
    ) -> Result<ProofDraft, ProofError> {
        self.create_draft_with_context(
            request,
            snapshot,
            llm_client,
            cancel,
            now,
            Vec::new(),
            Vec::new(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_draft_with_context(
        &self,
        request: CreateProofDraftRequest,
        snapshot: CapabilitySnapshot,
        llm_client: Option<&LlmClient>,
        cancel: &CancellationToken,
        now: i64,
        routes: Vec<RouteScore>,
        recommendations: Vec<SkillRecommendation>,
    ) -> Result<ProofDraft, ProofError> {
        request.planning.validate()?;
        let planning_goal = super::enhancement_contracts::planning_goal_text(
            &request.user_goal,
            request.requirements.as_ref(),
        )?;
        let requested_budget = request
            .requirements
            .as_ref()
            .and_then(|requirements| requirements.budget)
            .unwrap_or_default();
        if request
            .allowed_capability_ids
            .as_ref()
            .is_some_and(|selected| {
                selected.iter().any(|id| {
                    !snapshot
                        .capabilities
                        .iter()
                        .any(|cap| cap.capability_id == *id)
                })
            })
        {
            return Err(ProofError::new(
                "proofloop_capability_selection_incomplete",
                "所选工具未完整纳入能力快照，可能不可用或超出规划长度、数量上限。请检查工具配置或减少所选能力范围后重试",
            ));
        }
        let graph_id = TaskGraphId::generate();
        let (contract, candidate, planning_usage, planning_review) = if request.planning.strategy
            == PlanningStrategy::Existing
        {
            let (mut contract, candidate, usage) =
                super::planner::propose_candidate_with_usage_and_budget(
                    &request.workspace_id,
                    &planning_goal,
                    &snapshot,
                    llm_client,
                    cancel,
                    requested_budget,
                )
                .await?;
            contract.user_goal = request.user_goal.clone();
            if let Some(requirements) = &request.requirements {
                requirements.apply(&mut contract);
            }
            (contract, candidate, usage, None)
        } else {
            let client = llm_client
                .ok_or_else(|| ProofError::new("proofloop_model_not_configured", "大模型未配置"))?;
            let mut bundle = propose_candidates_with_usage(
                &request.workspace_id,
                &planning_goal,
                &snapshot,
                &recommendations,
                &request.planning,
                client,
                cancel,
                requested_budget,
            )
            .await?;
            bundle.contract.user_goal = request.user_goal.clone();
            if let Some(requirements) = &request.requirements {
                requirements.apply(&mut bundle.contract);
            }
            let mut options = Vec::new();
            for (index, candidate) in bundle.candidates.into_iter().enumerate() {
                let compilation =
                    compile_candidate(&bundle.contract, &candidate, &snapshot, &graph_id, &[]);
                let score = score_candidate(&bundle.contract, &candidate, &compilation, &snapshot);
                options.push(PlanOption {
                    candidate_id: format!("candidate-{index:02}"),
                    candidate,
                    compilation,
                    score,
                });
            }
            let recommended = select_recommended(&options);
            let selected = recommended
                .as_ref()
                .and_then(|id| options.iter().find(|o| &o.candidate_id == id))
                .or_else(|| options.first())
                .ok_or_else(|| {
                    ProofError::new("proofloop_candidate_count_mismatch", "没有候选方案")
                })?
                .candidate
                .clone();
            let review = PlanningReview {
                contract_version: bundle.contract.version,
                strategy: request.planning.strategy,
                snapshot_id: snapshot.snapshot_id.clone(),
                routes,
                options,
                recommended_candidate_id: recommended,
                selected_candidate_id: None,
                recommendations,
                scoring_version: "plan-score-v1".into(),
            };
            (bundle.contract, selected, bundle.usage, Some(review))
        };
        if planning_usage.total_tokens.is_none() {
            return Err(ProofError::new(
                "proofloop_usage_unavailable",
                "新目标的规划 token 用量未知，不能保存或确认目标",
            )
            .with_planning_usage(planning_usage));
        }
        if planning_usage.model_calls > contract.budget.max_model_calls
            || planning_usage
                .total_tokens
                .is_some_and(|tokens| tokens > contract.budget.max_total_tokens)
        {
            return Err(
                ProofError::new("proofloop_budget_exhausted", "规划消耗超过明确目标预算")
                    .with_planning_usage(planning_usage),
            );
        }
        let draft_id = format!("draft-{}", uuid::Uuid::new_v4());
        let compilation = compile_candidate(&contract, &candidate, &snapshot, &graph_id, &[]);

        let status = match compilation.status {
            CompileStatus::Ready => DraftStatus::ReadyForConfirmation,
            CompileStatus::NeedsInput => DraftStatus::NeedsInput,
            CompileStatus::Invalid => DraftStatus::Invalid,
        };

        let draft = ProofDraft {
            planning_review,
            draft_id,
            revision: 1,
            graph_id: graph_id.to_string(),
            contract,
            snapshot,
            candidate: Some(candidate),
            binding_confirmations: Vec::new(),
            compilation,
            status,
            confirmed_graph_revision: None,
            planning_usage,
        };

        self.db
            .insert_proof_draft(&draft, now)
            .map_err(|e| ProofError::new("draft_save_failed", e.to_string()))?;

        Ok(draft)
    }

    pub fn get_draft(&self, draft_id: &str) -> Result<Option<ProofDraft>, ProofError> {
        self.db
            .load_proof_draft(draft_id)
            .map_err(|e| ProofError::new("draft_load_failed", e.to_string()))
    }

    pub fn list_drafts(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<ProofDraftSummary>, ProofError> {
        self.db
            .list_proof_drafts(workspace_id)
            .map_err(|e| ProofError::new("draft_list_failed", e.to_string()))
    }

    pub fn select_candidate(
        &self,
        draft_id: &str,
        request: SelectCandidateRequest,
        snapshot: CapabilitySnapshot,
        now: i64,
    ) -> Result<ProofDraft, ProofError> {
        let mut draft = self
            .get_draft(draft_id)?
            .ok_or_else(|| ProofError::new("draft_not_found", "任务草稿不存在"))?;
        if draft.revision != request.expected_revision {
            return Err(ProofError::new(
                "proofloop_stale_revision",
                "草稿版本已变化，请加载最新方案",
            ));
        }
        if draft.status == DraftStatus::Confirmed {
            return Err(ProofError::new(
                "draft_already_confirmed",
                "已确认草稿不可切换候选",
            ));
        }
        let mut review = draft.planning_review.take().ok_or_else(|| {
            ProofError::new("proofloop_candidate_not_found", "该草稿没有多个候选方案")
        })?;
        let graph_id = TaskGraphId::new(&draft.graph_id)
            .map_err(|e| ProofError::new("invalid_graph_id", e.to_string()))?;
        for option in &mut review.options {
            option.compilation = compile_candidate(
                &draft.contract,
                &option.candidate,
                &snapshot,
                &graph_id,
                &[],
            );
            option.score = score_candidate(
                &draft.contract,
                &option.candidate,
                &option.compilation,
                &snapshot,
            );
        }
        let option = review
            .options
            .iter()
            .find(|o| o.candidate_id == request.candidate_id)
            .ok_or_else(|| {
                ProofError::new("proofloop_candidate_not_found", "候选方案不属于该草稿")
            })?;
        if option.compilation.status == CompileStatus::Invalid {
            return Err(ProofError::new(
                "proofloop_candidate_not_executable",
                "该候选方案未通过编译，请修正诊断",
            ));
        }
        draft.candidate = Some(option.candidate.clone());
        draft.compilation = option.compilation.clone();
        draft.status = if draft.compilation.status == CompileStatus::Ready {
            DraftStatus::ReadyForConfirmation
        } else {
            DraftStatus::NeedsInput
        };
        review.snapshot_id = snapshot.snapshot_id.clone();
        review.recommended_candidate_id = select_recommended(&review.options);
        review.selected_candidate_id = Some(request.candidate_id);
        draft.planning_review = Some(review);
        draft.snapshot = snapshot;
        draft.binding_confirmations.clear();
        draft.revision = next_draft_revision(draft.revision)?;
        self.db
            .update_proof_draft_if_current(request.expected_revision, &draft, now)
            .map_err(draft_write_error)?;
        Ok(draft)
    }

    /// Update draft contract / candidate / confirmations, recompile deterministically, and persist.
    pub fn update_draft(
        &self,
        draft_id: &str,
        request: UpdateProofDraftRequest,
        current_snapshot: CapabilitySnapshot,
        now: i64,
    ) -> Result<ProofDraft, ProofError> {
        let existing = self
            .get_draft(draft_id)?
            .ok_or_else(|| ProofError::new("draft_not_found", "指定的任务草稿不存在"))?;

        if existing.revision != request.expected_revision {
            return Err(ProofError::new(
                "proofloop_stale_revision",
                format!(
                    "草稿版本冲突：预期版本 {}，实际当前版本 {}",
                    request.expected_revision, existing.revision
                ),
            ));
        }

        if existing.status == DraftStatus::Confirmed {
            return Err(ProofError::new(
                "draft_already_confirmed",
                "已确认的草稿不可直接修改，请基于新目标创建草稿",
            ));
        }

        let graph_id = TaskGraphId::new(&existing.graph_id).map_err(|e| {
            ProofError::new("invalid_graph_id", format!("草稿关联的图标识无效: {e}"))
        })?;

        let confirmations = request
            .binding_confirmations
            .unwrap_or_else(|| existing.binding_confirmations.clone());
        let confirmations =
            self.verify_confirmations(request.candidate.as_ref(), &confirmations, now)?;

        let compilation = match &request.candidate {
            Some(cand) => compile_candidate(
                &request.contract,
                cand,
                &current_snapshot,
                &graph_id,
                &confirmations,
            ),
            None => super::contracts::CompileResult {
                status: CompileStatus::NeedsInput,
                graph: None,
                diagnostics: vec![ProofError::new("missing_candidate", "草稿未提供候选计划")],
                pending_inputs: Vec::new(),
            },
        };

        let status = match compilation.status {
            CompileStatus::Ready => DraftStatus::ReadyForConfirmation,
            CompileStatus::NeedsInput => DraftStatus::NeedsInput,
            CompileStatus::Invalid => DraftStatus::Invalid,
        };

        let updated_draft = ProofDraft {
            planning_review: None,
            draft_id: existing.draft_id,
            revision: next_draft_revision(existing.revision)?,
            graph_id: existing.graph_id,
            contract: request.contract,
            snapshot: current_snapshot,
            candidate: request.candidate,
            binding_confirmations: confirmations,
            compilation,
            status,
            confirmed_graph_revision: None,
            planning_usage: existing.planning_usage,
        };

        self.db
            .update_proof_draft_if_current(request.expected_revision, &updated_draft, now)
            .map_err(draft_write_error)?;

        Ok(updated_draft)
    }

    /// Confirm draft: re-evaluates snapshot against current capabilities, compiles to TaskGraph,
    /// persists GoalVersion and TaskGraph consistently, and marks draft confirmed.
    pub fn confirm_draft(
        &self,
        runtime: &TaskWorldRuntime,
        draft_id: &str,
        expected_revision: u32,
        current_snapshot: &CapabilitySnapshot,
        now: i64,
    ) -> Result<ConfirmProofDraftResponse, ProofError> {
        let mut draft = self
            .get_draft(draft_id)?
            .ok_or_else(|| ProofError::new("draft_not_found", "指定的任务草稿不存在"))?;

        if draft.revision != expected_revision {
            return Err(ProofError::new(
                "proofloop_stale_revision",
                format!(
                    "草稿版本冲突：预期版本 {}，实际当前版本 {}",
                    expected_revision, draft.revision
                ),
            ));
        }

        // Idempotency: if already confirmed, return prior confirmation
        if draft.status == DraftStatus::Confirmed {
            if let Some(rev) = draft.confirmed_graph_revision {
                return Ok(ConfirmProofDraftResponse {
                    graph_id: draft.graph_id,
                    graph_revision: rev,
                    goal_id: draft.contract.goal_id,
                    goal_version: draft.contract.version,
                });
            }
        }

        if draft
            .planning_review
            .as_ref()
            .is_some_and(|review| review.selected_candidate_id.is_none())
        {
            return Err(ProofError::new(
                "proofloop_candidate_selection_required",
                "请先选择一个候选方案，再确认保存",
            ));
        }

        let candidate = draft
            .candidate
            .as_ref()
            .ok_or_else(|| ProofError::new("no_candidate_plan", "草稿未包含可执行的候选计划"))?;
        draft.binding_confirmations =
            self.verify_confirmations(Some(candidate), &draft.binding_confirmations, now)?;

        let graph_id = TaskGraphId::new(&draft.graph_id)
            .map_err(|e| ProofError::new("invalid_graph_id", format!("草稿图标识无效: {e}")))?;

        // Recompile against current capability snapshot to ensure tools haven't become unavailable/changed
        let fresh_compilation = compile_candidate(
            &draft.contract,
            candidate,
            current_snapshot,
            &graph_id,
            &draft.binding_confirmations,
        );

        if fresh_compilation.status != CompileStatus::Ready {
            // Save updated compilation state in draft to reflect changes
            draft.compilation = fresh_compilation;
            draft.status = match draft.compilation.status {
                CompileStatus::NeedsInput => DraftStatus::NeedsInput,
                _ => DraftStatus::Invalid,
            };
            let expected_revision = draft.revision;
            draft.revision = next_draft_revision(expected_revision)?;
            self.db
                .update_proof_draft_if_current(expected_revision, &draft, now)
                .map_err(draft_write_error)?;

            return Err(ProofError::new(
                "proofloop_not_ready_for_confirmation",
                "当前计划未通过编译检查，无法确认保存",
            ));
        }

        let compiled_graph = fresh_compilation.graph.clone().ok_or_else(|| {
            ProofError::new("graph_compilation_missing", "编译未生成有效 TaskGraph")
        })?;

        draft.status = DraftStatus::Confirmed;
        draft.confirmed_graph_revision = Some(compiled_graph.revision.value());
        draft.compilation = fresh_compilation;
        let created_graph = match runtime.confirm_proofloop_draft(&draft, now) {
            Ok(graph) => graph,
            Err(error) => {
                let latest = self.get_draft(draft_id)?;
                if let Some(latest) = latest {
                    if latest.status == DraftStatus::Confirmed
                        && latest.revision == expected_revision
                        && latest.draft_id == draft.draft_id
                        && latest.graph_id == draft.graph_id
                        && latest.contract.goal_id == draft.contract.goal_id
                        && latest.contract.version == draft.contract.version
                        && latest.confirmed_graph_revision == draft.confirmed_graph_revision
                    {
                        return confirmed_response(latest).ok_or_else(|| {
                            ProofError::new(
                                "draft_confirmation_failed",
                                "确认草稿缺少已提交的任务图版本",
                            )
                        });
                    }
                    if latest.status == DraftStatus::Confirmed {
                        return Err(ProofError::new(
                            "draft_already_confirmed",
                            "草稿已由另一确认请求提交",
                        ));
                    }
                    if latest.revision != expected_revision {
                        return Err(ProofError::new(
                            "proofloop_stale_revision",
                            format!(
                                "草稿版本冲突：预期版本 {expected_revision}，实际当前版本 {}",
                                latest.revision
                            ),
                        ));
                    }
                }
                return Err(ProofError::new(
                    "draft_confirmation_failed",
                    error.to_string(),
                ));
            }
        };

        Ok(ConfirmProofDraftResponse {
            graph_id: created_graph.id.to_string(),
            graph_revision: created_graph.revision.value(),
            goal_id: draft.contract.goal_id,
            goal_version: draft.contract.version,
        })
    }
}
