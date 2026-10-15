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
use super::planner::propose_candidate_with_usage;

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
        let (contract, candidate, planning_usage) = propose_candidate_with_usage(
            &request.workspace_id,
            &request.user_goal,
            &snapshot,
            llm_client,
            cancel,
        )
        .await?;

        let draft_id = format!("draft-{}", uuid::Uuid::new_v4());
        let graph_id = TaskGraphId::generate();

        let compilation = compile_candidate(&contract, &candidate, &snapshot, &graph_id, &[]);

        let status = match compilation.status {
            CompileStatus::Ready => DraftStatus::ReadyForConfirmation,
            CompileStatus::NeedsInput => DraftStatus::NeedsInput,
            CompileStatus::Invalid => DraftStatus::Invalid,
        };

        let draft = ProofDraft {
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

    fn verify_confirmations(
        &self,
        candidate: Option<&super::contracts::CandidatePlan>,
        confirmations: &[super::contracts::BindingConfirmation],
        now: i64,
    ) -> Result<Vec<super::contracts::BindingConfirmation>, ProofError> {
        let mut verified = Vec::new();
        for confirmation in confirmations {
            let source_node = candidate
                .and_then(|plan| {
                    plan.nodes
                        .iter()
                        .find(|node| node.id == confirmation.source_node_id)
                })
                .ok_or_else(|| {
                    ProofError::new("invalid_binding_confirmation", "确认的来源节点不存在")
                })?;
            if source_node.capability_id != confirmation.source_capability_id {
                return Err(ProofError::new(
                    "invalid_binding_confirmation",
                    "来源能力已改变，请重新选择真实输出",
                ));
            }
            let id = super::super::NodeExecutionId::new(&confirmation.source_execution_id)
                .map_err(|error| {
                    ProofError::new("invalid_binding_confirmation", error.to_string())
                })?;
            let execution = self
                .db
                .load_node_execution(&id)
                .map_err(|error| ProofError::new("db_error", error.to_string()))?
                .ok_or_else(|| {
                    ProofError::new("invalid_binding_confirmation", "确认引用的执行不存在")
                })?;
            if execution.status != super::super::NodeExecutionStatus::Succeeded
                || !execution.validation.as_ref().is_some_and(|validation| {
                    validation.status == super::super::execution::ValidationStatus::Accepted
                })
                || execution
                    .executor_ref
                    .as_ref()
                    .map(|reference| reference.as_str())
                    != Some(format!("capability://{}", confirmation.source_capability_id).as_str())
            {
                return Err(ProofError::new(
                    "invalid_binding_confirmation",
                    "来源执行没有通过验证或能力身份不一致",
                ));
            }
            let mut value = execution.output.as_ref().ok_or_else(|| {
                ProofError::new("invalid_binding_confirmation", "执行没有结构化结果")
            })?;
            for segment in &confirmation.source_path {
                value = value
                    .as_object()
                    .and_then(|object| object.get(segment))
                    .ok_or_else(|| {
                        ProofError::new("invalid_binding_confirmation", "真实输出中不存在该路径")
                    })?;
            }
            let mut item = confirmation.clone();
            item.observed_type = match value {
                serde_json::Value::Null => "null",
                serde_json::Value::Bool(_) => "boolean",
                serde_json::Value::String(_) => "string",
                serde_json::Value::Array(_) => "array",
                serde_json::Value::Object(_) => "object",
                serde_json::Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
                _ => "number",
            }
            .into();
            item.confirmed_at_ms = now;
            verified.push(item);
        }
        Ok(verified)
    }
}
