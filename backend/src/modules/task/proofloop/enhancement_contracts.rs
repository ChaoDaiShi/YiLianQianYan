use serde::{Deserialize, Serialize};
use super::contracts::{CandidatePlan, CompileResult, GoalContract, PlanningUsage};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningStrategy {
    #[default]
    Existing,
    Ranked,
    Experience,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanningOptions {
    pub strategy: PlanningStrategy,
    pub candidate_count: u8,
}
impl Default for PlanningOptions {
    fn default() -> Self {
        Self { strategy: PlanningStrategy::Existing, candidate_count: 1 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteScore {
    pub capability_id: String,
    pub lexical_bp: u16,
    pub type_bp: u16,
    pub history_bp: Option<u16>,
    pub total_bp: u16,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanScore {
    pub coverage_bp: u16,
    pub type_bp: u16,
    pub evidence_bp: u16,
    pub risk_bp: u16,
    pub cost_bp: u16,
    pub cost_known: bool,
    pub total: i64,
    pub rejected_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanOption {
    pub candidate_id: String,
    pub candidate: CandidatePlan,
    pub compilation: CompileResult,
    pub score: PlanScore,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillRecommendation {
    pub skill_name: String,
    pub version: u32,
    pub score_bp: u16,
    pub evidence_report_ids: Vec<String>,
    pub rationale: String,
    pub context_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanningReview {
    pub contract_version: u32,
    pub strategy: PlanningStrategy,
    pub snapshot_id: String,
    pub routes: Vec<RouteScore>,
    pub options: Vec<PlanOption>,
    pub recommended_candidate_id: Option<String>,
    pub selected_candidate_id: Option<String>,
    pub recommendations: Vec<SkillRecommendation>,
    pub scoring_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankedPlanningBundle {
    pub contract: GoalContract,
    pub candidates: Vec<CandidatePlan>,
    pub usage: PlanningUsage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectCandidateRequest {
    pub expected_revision: u32,
    pub candidate_id: String,
}
impl PlanningOptions {
    pub fn validate(&self) -> Result<(), super::contracts::ProofError> {
        let valid = match self.strategy {
            PlanningStrategy::Existing => self.candidate_count == 1,
            PlanningStrategy::Ranked | PlanningStrategy::Experience => (2..=3).contains(&self.candidate_count),
        };
        if valid { Ok(()) } else { Err(super::contracts::ProofError::new("proofloop_invalid_planning_options", "原规划需要1个方案，增强规划需要2或3个候选方案")) }
    }
}
