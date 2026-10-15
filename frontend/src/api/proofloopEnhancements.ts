import type { CandidatePlan, CompileResult } from "./proofloop";

export type PlanningStrategy = "existing" | "ranked" | "experience";

export interface PlanningOptions {
  strategy: PlanningStrategy;
  candidate_count: 1 | 2 | 3;
}

export interface RouteScore {
  capability_id: string;
  lexical_bp: number;
  type_bp: number;
  history_bp: number | null;
  total_bp: number;
  reasons: string[];
}

export interface PlanScore {
  coverage_bp: number;
  type_bp: number;
  evidence_bp: number;
  risk_bp: number;
  cost_bp: number;
  cost_known: boolean;
  total: number;
  rejected_codes: string[];
}

export interface PlanOption {
  candidate_id: string;
  candidate: CandidatePlan;
  compilation: CompileResult;
  score: PlanScore;
}

export interface SkillRecommendation {
  skill_name: string;
  version: number;
  score_bp: number;
  evidence_report_ids: string[];
  rationale: string;
  context_text: string;
}

export interface PlanningReview {
  contract_version: number;
  strategy: PlanningStrategy;
  snapshot_id: string;
  routes: RouteScore[];
  options: PlanOption[];
  recommended_candidate_id: string | null;
  selected_candidate_id: string | null;
  recommendations: SkillRecommendation[];
  scoring_version: string;
}

export interface SelectCandidateRequest {
  expected_revision: number;
  candidate_id: string;
}
