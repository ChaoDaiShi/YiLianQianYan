//! ProofLoop contracts and core data transfer objects.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/00-contracts-and-ownership.md
//! Schema version: 1.

use serde::{Deserialize, Serialize};

use crate::modules::task::{input_binding::InputBinding, TaskGraph};

pub const PROOFLOOP_SCHEMA_VERSION: u32 = 1;

pub type FieldPath = Vec<String>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofError {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planning_usage: Option<PlanningUsage>,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_path: Option<FieldPath>,
}

impl ProofError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            planning_usage: None,
            code: code.into(),
            message: message.into(),
            node_id: None,
            target_path: None,
        }
    }

    pub fn with_planning_usage(mut self, usage: PlanningUsage) -> Self {
        self.planning_usage = Some(usage);
        self
    }

    pub fn with_node(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    pub fn with_target_path(mut self, path: FieldPath) -> Self {
        self.target_path = Some(path);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub max_model_calls: u32,
    pub max_total_tokens: u64,
    pub max_wall_time_ms: u64,
    pub max_repair_rounds: u32,
    #[serde(default)]
    pub time_basis: BudgetTimeBasis,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetTimeBasis {
    #[default]
    ActiveWork,
    WallClock,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalTimeUsage {
    pub planning_elapsed_ms: Option<u64>,
    pub execution_active_ms: u64,
    pub human_wait_ms: Option<u64>,
    pub accounting_state: String,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_model_calls: 6,
            max_total_tokens: 24_000,
            max_wall_time_ms: 600_000,
            max_repair_rounds: 2,
            time_basis: BudgetTimeBasis::ActiveWork,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CriterionCheck {
    ArtifactExists {
        deliverable_id: String,
    },
    ArtifactDigestMatches {
        deliverable_id: String,
    },
    RequiredSections {
        deliverable_id: String,
        headings: Vec<String>,
    },
    SourceTraceability {
        deliverable_id: String,
        min_sources: u32,
    },
    HumanReview {
        deliverable_id: String,
        rubric: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Criterion {
    pub id: String,
    pub description: String,
    pub required: bool,
    pub check: CriterionCheck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliverableSpec {
    pub id: String,
    pub title: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalContract {
    pub schema_version: u32,
    pub goal_id: String,
    pub version: u32,
    pub workspace_id: String,
    pub user_goal: String,
    pub deliverables: Vec<DeliverableSpec>,
    pub criteria: Vec<Criterion>,
    #[serde(default)]
    pub constraints: Vec<String>,
    pub allowed_capability_ids: Vec<String>,
    pub budget: Budget,
}

impl GoalContract {
    pub fn validate_source_requirements(&self) -> Result<(), ProofError> {
        for (index, criterion) in self.criteria.iter().enumerate() {
            if matches!(
                criterion.check,
                CriterionCheck::SourceTraceability { min_sources: 0, .. }
            ) {
                return Err(ProofError::new(
                    "proofloop_invalid_source_threshold",
                    format!(
                        "来源追溯条件 '{}' 的最低来源数量必须至少为 1，请修正目标契约",
                        criterion.id
                    ),
                )
                .with_target_path(vec![
                    "criteria".into(),
                    index.to_string(),
                    "check".into(),
                    "min_sources".into(),
                ]));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKindSpec {
    McpTool,
    Workflow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilitySpec {
    pub capability_id: String,
    pub executor_ref: String,
    pub kind: CapabilityKindSpec,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub output_path_prefix: FieldPath,
    pub output_provenance: String, // "declared" | "unknown"
    pub risk: String,
    pub readiness: String, // "ready" | "unavailable"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilitySnapshot {
    pub snapshot_id: String,
    pub captured_at_ms: i64,
    pub capabilities: Vec<CapabilitySpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateNode {
    pub id: String,
    pub title: String,
    pub instruction: String,
    pub capability_id: String,
    #[serde(default)]
    pub literal_input: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub bindings: Vec<InputBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliverableNodeMapping {
    pub deliverable_id: String,
    pub node_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidatePlan {
    pub schema_version: u32,
    pub nodes: Vec<CandidateNode>,
    pub edges: Vec<CandidateEdge>,
    #[serde(default)]
    pub deliverable_nodes: Vec<DeliverableNodeMapping>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingInputReason {
    MissingLiteral,
    UnknownOutput,
    UnsupportedSchema,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingInput {
    pub node_id: String,
    pub target_path: FieldPath,
    pub reason: PendingInputReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingConfirmation {
    pub node_id: String,
    pub target_path: FieldPath,
    pub source_node_id: String,
    pub source_capability_id: String,
    pub source_execution_id: String,
    pub source_path: FieldPath,
    pub observed_type: String,
    pub confirmed_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompileStatus {
    Ready,
    NeedsInput,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompileResult {
    pub status: CompileStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<TaskGraph>,
    #[serde(default)]
    pub diagnostics: Vec<ProofError>,
    #[serde(default)]
    pub pending_inputs: Vec<PendingInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftStatus {
    Proposed,
    NeedsInput,
    Invalid,
    ReadyForConfirmation,
    Confirmed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningUsage {
    pub model_calls: u32,
    pub total_tokens: Option<u64>,
    pub started_at_ms: i64,
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
}

impl Default for PlanningUsage {
    fn default() -> Self {
        Self {
            model_calls: 1,
            total_tokens: None,
            started_at_ms: 0,
            elapsed_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProofDraft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planning_review: Option<super::enhancement_contracts::PlanningReview>,
    pub draft_id: String,
    pub revision: u32,
    pub graph_id: String,
    pub contract: GoalContract,
    pub snapshot: CapabilitySnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate: Option<CandidatePlan>,
    #[serde(default)]
    pub binding_confirmations: Vec<BindingConfirmation>,
    pub compilation: CompileResult,
    pub status: DraftStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_graph_revision: Option<u64>,
    #[serde(default)]
    pub planning_usage: PlanningUsage,
}

// Evidence and Acceptance models

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRef {
    pub graph_id: String,
    pub graph_revision: u64,
    pub node_id: String,
    pub execution_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub evidence_id: String,
    pub origin: ExecutionRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_path: Option<FieldPath>,
    pub digest: String,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliverableArtifactMapping {
    pub deliverable_id: String,
    pub artifact_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceSource {
    pub source_id: String,
    pub evidence_id: String,
    pub uri: String,
    pub retrieved_at_ms: i64,
    pub cited_by_deliverable_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceSnapshot {
    pub snapshot_id: String,
    pub goal_id: String,
    pub goal_version: u32,
    pub graph_id: String,
    pub graph_revision: u64,
    pub executions: Vec<ExecutionRef>,
    pub evidence: Vec<EvidenceRef>,
    pub deliverables: Vec<DeliverableArtifactMapping>,
    pub sources: Vec<EvidenceSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanReview {
    pub criterion_id: String,
    pub decision: String, // "pass" | "fail"
    pub reason: String,
    pub reviewer_subject: String,
    pub reviewed_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriterionStatus {
    Pass,
    Fail,
    NotChecked,
    HumanPending,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriterionResult {
    pub criterion_id: String,
    pub status: CriterionStatus,
    pub evidence_ids: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliverableResult {
    pub deliverable_id: String,
    pub status: CriterionStatus,
    pub code: String,
    pub evidence_ids: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Pass,
    Fail,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceReport {
    pub report_id: String,
    pub goal_id: String,
    pub goal_version: u32,
    pub graph_id: String,
    pub graph_revision: u64,
    pub snapshot_id: String,
    pub status: GoalStatus,
    pub results: Vec<CriterionResult>,
    #[serde(default)]
    pub deliverable_results: Vec<DeliverableResult>,
    pub human_reviews: Vec<HumanReview>,
    pub checked_at_ms: i64,
}

// Repair models

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepairOperation {
    ReplaceLiteral {
        node_id: String,
        target_path: FieldPath,
        value: serde_json::Value,
    },
    ReplaceBinding {
        node_id: String,
        binding: InputBinding,
    },
    ReplaceCapability {
        node_id: String,
        capability_id: String,
        literal_input: serde_json::Map<String, serde_json::Value>,
        bindings: Vec<InputBinding>,
    },
    RerunNode {
        node_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepairProposal {
    pub schema_version: u32,
    pub repair_id: String,
    pub goal_id: String,
    pub goal_version: u32,
    pub expected_graph_revision: u64,
    pub source_report_id: String,
    pub failure_execution_ids: Vec<String>,
    pub round: u32,
    pub operations: Vec<RepairOperation>,
    pub rerun_root_node_id: String,
    pub affected_node_ids: Vec<String>,
    pub rationale: String,
}

// Request and Response DTOs

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateProofDraftRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirements: Option<super::enhancement_contracts::GoalRequirements>,
    #[serde(default)]
    pub planning: super::enhancement_contracts::PlanningOptions,
    pub workspace_id: String,
    pub user_goal: String,
    #[serde(default)]
    pub allowed_capability_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateProofDraftRequest {
    pub expected_revision: u32,
    pub contract: GoalContract,
    #[serde(default)]
    pub candidate: Option<CandidatePlan>,
    #[serde(default)]
    pub binding_confirmations: Option<Vec<BindingConfirmation>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmProofDraftRequest {
    pub expected_revision: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmProofDraftResponse {
    pub graph_id: String,
    pub graph_revision: u64,
    pub goal_id: String,
    pub goal_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofDraftSummary {
    pub draft_id: String,
    pub revision: u32,
    pub graph_id: String,
    pub goal_id: String,
    pub user_goal: String,
    pub status: DraftStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluateGoalRequest {
    pub goal_version: u32,
    pub graph_revision: u64,
    pub execution_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitHumanReviewRequest {
    pub criterion_id: String,
    pub decision: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposeRepairRequest {
    pub report_id: String,
    pub failure_execution_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyRepairRequest {
    pub expected_graph_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyRepairResponse {
    pub graph_revision: u64,
    pub affected_node_ids: Vec<String>,
}
