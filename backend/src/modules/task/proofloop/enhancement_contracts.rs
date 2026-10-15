use super::contracts::{CandidatePlan, CompileResult, GoalContract, PlanningUsage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningStrategy {
    #[default]
    Existing,
    Ranked,
    Experience,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningOptions {
    pub strategy: PlanningStrategy,
    pub candidate_count: u8,
}
impl Default for PlanningOptions {
    fn default() -> Self {
        Self {
            strategy: PlanningStrategy::Existing,
            candidate_count: 1,
        }
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
            PlanningStrategy::Ranked | PlanningStrategy::Experience => {
                (2..=3).contains(&self.candidate_count)
            }
        };
        if valid {
            Ok(())
        } else {
            Err(super::contracts::ProofError::new(
                "proofloop_invalid_planning_options",
                "原规划需要1个方案，增强规划需要2或3个候选方案",
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalRequirements {
    pub deliverables: Vec<super::contracts::DeliverableSpec>,
    pub criteria: Vec<super::contracts::Criterion>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub budget: Option<super::contracts::Budget>,
}
impl GoalRequirements {
    pub fn validate(&self) -> Result<(), super::contracts::ProofError> {
        if self.deliverables.is_empty() || !self.criteria.iter().any(|c| c.required) {
            return Err(super::contracts::ProofError::new(
                "proofloop_invalid_goal_requirements",
                "明确目标须包含交付物和必需验收条件",
            ));
        }
        if self.budget.is_some_and(|budget| {
            budget.max_model_calls == 0
                || budget.max_total_tokens == 0
                || budget.max_wall_time_ms == 0
        }) {
            return Err(super::contracts::ProofError::new(
                "proofloop_invalid_goal_requirements",
                "目标预算须允许至少一次调用、正数token与执行时间",
            ));
        }
        for criterion in &self.criteria {
            if matches!(
                criterion.check,
                super::contracts::CriterionCheck::SourceTraceability { min_sources: 0, .. }
            ) {
                return Err(super::contracts::ProofError::new(
                    "proofloop_invalid_source_threshold",
                    "最低来源数量必须至少为1",
                ));
            }
        }
        Ok(())
    }
    pub fn apply(&self, contract: &mut super::contracts::GoalContract) {
        contract.deliverables = self.deliverables.clone();
        contract.criteria = self.criteria.clone();
        contract.constraints = self.constraints.clone();
        if let Some(budget) = self.budget {
            contract.budget = budget;
        }
    }
}
pub fn planning_goal_text(
    goal: &str,
    requirements: Option<&GoalRequirements>,
) -> Result<String, super::contracts::ProofError> {
    match requirements {
        None => Ok(goal.to_owned()),
        Some(value) => {
            value.validate()?;
            let encoded = serde_json::to_string(value).map_err(|e| {
                super::contracts::ProofError::new(
                    "proofloop_invalid_goal_requirements",
                    e.to_string(),
                )
            })?;
            Ok(format!("{goal}\n\n用户明确提供的目标约束，所有候选必须原样使用交付物id及验收条件：\n{encoded}"))
        }
    }
}
