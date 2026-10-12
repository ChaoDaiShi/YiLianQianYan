//! ProofDraft lifecycle management: creation, update, compilation and confirmation.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/01-capability-and-compiler.md §6

use tokio_util::sync::CancellationToken;

use crate::db::Database;
use crate::integrations::llm::client::LlmClient;
use crate::modules::task::{runtime::TaskWorldRuntime, task_graph::TaskGraphId};

use super::compiler::compile_candidate;
use super::contracts::{
    CapabilitySnapshot, CompileStatus, ConfirmProofDraftResponse, CreateProofDraftRequest,
    DraftStatus, ProofDraft, ProofDraftSummary, ProofError, UpdateProofDraftRequest,
};
use super::planner::propose_candidate;

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
        let (contract, candidate) = propose_candidate(
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
        };

        self.db
            .save_proof_draft(&draft, now)
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
            revision: existing.revision + 1,
            graph_id: existing.graph_id,
            contract: request.contract,
            snapshot: current_snapshot,
            candidate: request.candidate,
            binding_confirmations: confirmations,
            compilation,
            status,
            confirmed_graph_revision: None,
        };

        self.db
            .save_proof_draft(&updated_draft, now)
            .map_err(|e| ProofError::new("draft_save_failed", e.to_string()))?;

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
            let _ = self.db.save_proof_draft(&draft, now);

            return Err(ProofError::new(
                "proofloop_not_ready_for_confirmation",
                "当前计划未通过编译检查，无法确认保存",
            ));
        }

        let compiled_graph = fresh_compilation.graph.clone().ok_or_else(|| {
            ProofError::new("graph_compilation_missing", "编译未生成有效 TaskGraph")
        })?;

        // 1. Persist goal version
        self.db
            .save_goal_version(&draft.contract, now)
            .map_err(|e| ProofError::new("goal_version_save_failed", e.to_string()))?;

        // 2. Create and persist TaskGraph through TaskWorldRuntime
        let created_graph = runtime
            .create_graph(
                compiled_graph.id.clone(),
                compiled_graph.nodes,
                compiled_graph.edges,
                now,
            )
            .map_err(|e| ProofError::new("task_graph_create_failed", e.to_string()))?;

        // 3. Mark draft confirmed
        draft.status = DraftStatus::Confirmed;
        draft.confirmed_graph_revision = Some(created_graph.revision.value());
        draft.compilation = fresh_compilation;

        self.db
            .save_proof_draft(&draft, now)
            .map_err(|e| ProofError::new("draft_save_failed", e.to_string()))?;

        Ok(ConfirmProofDraftResponse {
            graph_id: created_graph.id.to_string(),
            graph_revision: created_graph.revision.value(),
            goal_id: draft.contract.goal_id,
            goal_version: draft.contract.version,
        })
    }
}
