import type { TaskGraphDefinition as TaskGraph } from "./taskWorld";
import type { InputBinding } from "../features/task-world/input-binding/model";
import { API_BASE } from "./transport";
import { controlSessionHeaders } from "./controlSession";
import type { ApiResult } from "./taskWorld";

export type Json = null | boolean | number | string | Json[] | { [k: string]: Json };
export type JsonObject = { [k: string]: Json };
export type JsonSchema = JsonObject;
export type FieldPath = string[];

export interface ProofError {
  code: string;
  message: string;
  node_id?: string;
  target_path?: FieldPath;
}

export interface Budget {
  max_model_calls: number;
  max_total_tokens: number;
  max_wall_time_ms: number;
  max_repair_rounds: number;
}

export type CriterionCheck =
  | { kind: "artifact_exists"; deliverable_id: string }
  | { kind: "artifact_digest_matches"; deliverable_id: string }
  | { kind: "required_sections"; deliverable_id: string; headings: string[] }
  | { kind: "source_traceability"; deliverable_id: string; min_sources: number }
  | { kind: "human_review"; deliverable_id: string; rubric: string };

export interface Criterion {
  id: string;
  description: string;
  required: boolean;
  check: CriterionCheck;
}

export interface DeliverableSpec {
  id: string;
  title: string;
  mime_type: string;
}

export interface GoalContract {
  schema_version: 1;
  goal_id: string;
  version: number;
  workspace_id: string;
  user_goal: string;
  deliverables: DeliverableSpec[];
  criteria: Criterion[];
  constraints: string[];
  allowed_capability_ids: string[];
  budget: Budget;
}

export interface CapabilitySpec {
  capability_id: string;
  executor_ref: string;
  kind: "mcp_tool" | "workflow";
  name: string;
  description: string;
  input_schema: JsonSchema | null;
  output_schema: JsonSchema | null;
  output_path_prefix: FieldPath;
  output_provenance: "declared" | "unknown";
  risk: string;
  readiness: "ready" | "unavailable";
  unavailable_reason: string | null;
}

export interface CapabilitySnapshot {
  snapshot_id: string;
  captured_at_ms: number;
  capabilities: CapabilitySpec[];
}

export interface CandidateNode {
  id: string;
  title: string;
  instruction: string;
  capability_id: string;
  literal_input: JsonObject;
  bindings: InputBinding[];
}

export interface CandidatePlan {
  schema_version: 1;
  nodes: CandidateNode[];
  edges: { from: string; to: string }[];
  deliverable_nodes: { deliverable_id: string; node_id: string }[];
}

export interface PendingInput {
  node_id: string;
  target_path: FieldPath;
  reason: "missing_literal" | "unknown_output" | "unsupported_schema";
}

export interface BindingConfirmation {
  node_id: string;
  target_path: FieldPath;
  source_node_id: string;
  source_capability_id: string;
  source_execution_id: string;
  source_path: FieldPath;
  observed_type:
    | "string"
    | "number"
    | "integer"
    | "boolean"
    | "object"
    | "array"
    | "null";
  confirmed_at_ms: number;
}

export interface CompileResult {
  status: "ready" | "needs_input" | "invalid";
  graph: TaskGraph | null;
  diagnostics: ProofError[];
  pending_inputs: PendingInput[];
}

export interface ProofDraft {
  draft_id: string;
  revision: number;
  graph_id: string;
  contract: GoalContract;
  snapshot: CapabilitySnapshot;
  candidate: CandidatePlan | null;
  binding_confirmations: BindingConfirmation[];
  compilation: CompileResult;
  status:
    | "proposed"
    | "needs_input"
    | "invalid"
    | "ready_for_confirmation"
    | "confirmed";
  confirmed_graph_revision: number | null;
}

export interface ExecutionRef {
  graph_id: string;
  graph_revision: number;
  node_id: string;
  execution_id: string;
}

export interface EvidenceRef {
  evidence_id: string;
  origin: ExecutionRef;
  artifact_id: string | null;
  output_path: FieldPath | null;
  digest: string;
  observed_at_ms: number;
}

export interface DeliverableArtifactMapping {
  deliverable_id: string;
  artifact_id: string;
}

export interface EvidenceSource {
  source_id: string;
  evidence_id: string;
  uri: string;
  retrieved_at_ms: number;
  cited_by_deliverable_ids: string[];
}

export interface EvidenceSnapshot {
  snapshot_id: string;
  goal_id: string;
  goal_version: number;
  graph_id: string;
  graph_revision: number;
  executions: ExecutionRef[];
  evidence: EvidenceRef[];
  deliverables: DeliverableArtifactMapping[];
  sources: EvidenceSource[];
}

export interface HumanReview {
  criterion_id: string;
  decision: "pass" | "fail";
  reason: string;
  reviewer_subject: string;
  reviewed_at_ms: number;
}

export interface CriterionResult {
  criterion_id: string;
  status: "pass" | "fail" | "not_checked" | "human_pending" | "stale";
  evidence_ids: string[];
  reason: string;
}

export interface AcceptanceReport {
  report_id: string;
  goal_id: string;
  goal_version: number;
  graph_id: string;
  graph_revision: number;
  snapshot_id: string;
  status: "pass" | "fail" | "incomplete";
  results: CriterionResult[];
  human_reviews: HumanReview[];
  checked_at_ms: number;
}

export type RepairOperation =
  | { kind: "replace_literal"; node_id: string; target_path: FieldPath; value: Json }
  | { kind: "replace_binding"; node_id: string; binding: InputBinding }
  | {
      kind: "replace_capability";
      node_id: string;
      capability_id: string;
      literal_input: JsonObject;
      bindings: InputBinding[];
    }
  | { kind: "rerun_node"; node_id: string };

export interface RepairProposal {
  schema_version: 1;
  repair_id: string;
  goal_id: string;
  goal_version: number;
  expected_graph_revision: number;
  source_report_id: string;
  failure_execution_ids: string[];
  round: number;
  operations: RepairOperation[];
  rerun_root_node_id: string;
  affected_node_ids: string[];
  rationale: string;
}

// Request & Response shapes
export interface CreateProofDraftRequest {
  workspace_id: string;
  user_goal: string;
  allowed_capability_ids?: string[];
}

export interface UpdateProofDraftRequest {
  expected_revision: number;
  contract: GoalContract;
  candidate: CandidatePlan | null;
  binding_confirmations?: BindingConfirmation[];
}

export interface ConfirmProofDraftRequest {
  expected_revision: number;
}

export interface ConfirmProofDraftResponse {
  graph_id: string;
  graph_revision: number;
  goal_id: string;
  goal_version: number;
}

export interface EvaluateGoalRequest {
  goal_version: number;
  graph_revision: number;
  execution_ids: string[];
}

export interface SubmitHumanReviewRequest {
  criterion_id: string;
  decision: "pass" | "fail";
  reason: string;
}

export interface ProposeRepairRequest {
  report_id: string;
  failure_execution_ids: string[];
}

export interface ApplyRepairRequest {
  expected_graph_revision: number;
}

export interface ApplyRepairResponse {
  graph_revision: number;
  affected_node_ids: string[];
}

export interface ProofDraftSummary {
  draft_id: string;
  revision: number;
  graph_id: string;
  goal_id: string;
  user_goal: string;
  status: ProofDraft["status"];
}

interface ErrorPayload {
  error?: string;
  code?: string;
  message?: string;
}

function parseJson(text: string): unknown {
  if (!text.trim()) return null;
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return null;
  }
}

function errorMessage(payload: unknown): string | null {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return null;
  const value = payload as ErrorPayload;
  return typeof value.message === "string"
    ? value.message
    : typeof value.error === "string"
      ? value.error
      : null;
}

function errorCode(payload: unknown): string | null {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return null;
  const value = payload as ErrorPayload;
  const code = value.code || value.error;
  return typeof code === "string" && /^[a-z0-9_\-]+$/.test(code) ? code : null;
}

export async function requestProofLoop<T>(
  method: string,
  path: string,
  body?: unknown,
  signal?: AbortSignal,
): Promise<ApiResult<T>> {
  try {
    const options: RequestInit = {
      method,
      headers: {
        "Content-Type": "application/json",
        ...controlSessionHeaders(),
      },
      signal,
    };
    if (body !== undefined) options.body = JSON.stringify(body);

    const response = await fetch(`${API_BASE}${path}`, options);
    const text = response.status === 204 ? "" : await response.text();
    const payload = parseJson(text);

    if (!response.ok) {
      const message = errorMessage(payload) || `HTTP ${response.status}`;
      return {
        ok: false,
        error: {
          status: response.status,
          code: errorCode(payload) || `http_${response.status}`,
          message,
        },
      };
    }

    if (!text) return { ok: true, data: {} as T };
    if (payload === null) {
      return {
        ok: false,
        error: {
          status: 0,
          code: "invalid_json",
          message: "ProofLoop API 返回了无效 JSON。",
        },
      };
    }
    return { ok: true, data: payload as T };
  } catch (error: unknown) {
    if (error instanceof DOMException && error.name === "AbortError") {
      return {
        ok: false,
        error: {
          status: 0,
          code: "aborted",
          message: "请求已被用户取消",
        },
      };
    }
    return {
      ok: false,
      error: {
        status: 0,
        code: "network_error",
        message: error instanceof Error ? error.message : String(error),
      },
    };
  }
}

// API client functions
export async function createProofDraft(
  request: CreateProofDraftRequest,
  signal?: AbortSignal,
): Promise<ApiResult<ProofDraft>> {
  return requestProofLoop<ProofDraft>("POST", "/api/task-world/proofloop/drafts", request, signal);
}

export async function listProofDrafts(
  workspaceId?: string,
  signal?: AbortSignal,
): Promise<ApiResult<{ drafts: ProofDraftSummary[] }>> {
  const query = workspaceId ? `?workspace_id=${encodeURIComponent(workspaceId)}` : "";
  return requestProofLoop<{ drafts: ProofDraftSummary[] }>(
    "GET",
    `/api/task-world/proofloop/drafts${query}`,
    undefined,
    signal,
  );
}

export async function getProofDraft(
  draftId: string,
  signal?: AbortSignal,
): Promise<ApiResult<ProofDraft>> {
  return requestProofLoop<ProofDraft>(
    "GET",
    `/api/task-world/proofloop/drafts/${encodeURIComponent(draftId)}`,
    undefined,
    signal,
  );
}

export async function updateProofDraft(
  draftId: string,
  request: UpdateProofDraftRequest,
  signal?: AbortSignal,
): Promise<ApiResult<ProofDraft>> {
  return requestProofLoop<ProofDraft>(
    "PUT",
    `/api/task-world/proofloop/drafts/${encodeURIComponent(draftId)}`,
    request,
    signal,
  );
}

export async function confirmProofDraft(
  draftId: string,
  request: ConfirmProofDraftRequest,
  signal?: AbortSignal,
): Promise<ApiResult<ConfirmProofDraftResponse>> {
  return requestProofLoop<ConfirmProofDraftResponse>(
    "POST",
    `/api/task-world/proofloop/drafts/${encodeURIComponent(draftId)}/confirm`,
    request,
    signal,
  );
}

export async function evaluateGoal(
  goalId: string,
  request: EvaluateGoalRequest,
  signal?: AbortSignal,
): Promise<ApiResult<AcceptanceReport>> {
  return requestProofLoop<AcceptanceReport>(
    "POST",
    `/api/task-world/proofloop/goals/${encodeURIComponent(goalId)}/evaluations`,
    request,
    signal,
  );
}

export async function listGoalEvaluations(
  goalId: string,
  signal?: AbortSignal,
): Promise<ApiResult<{ reports: AcceptanceReport[] }>> {
  return requestProofLoop<{ reports: AcceptanceReport[] }>(
    "GET",
    `/api/task-world/proofloop/goals/${encodeURIComponent(goalId)}/evaluations`,
    undefined,
    signal,
  );
}

export async function submitHumanReview(
  evaluationId: string,
  request: SubmitHumanReviewRequest,
  signal?: AbortSignal,
): Promise<ApiResult<AcceptanceReport>> {
  return requestProofLoop<AcceptanceReport>(
    "POST",
    `/api/task-world/proofloop/evaluations/${encodeURIComponent(evaluationId)}/human-review`,
    request,
    signal,
  );
}

export async function proposeRepair(
  goalId: string,
  request: ProposeRepairRequest,
  signal?: AbortSignal,
): Promise<ApiResult<RepairProposal>> {
  return requestProofLoop<RepairProposal>(
    "POST",
    `/api/task-world/proofloop/goals/${encodeURIComponent(goalId)}/repairs`,
    request,
    signal,
  );
}

export async function applyRepair(
  repairId: string,
  request: ApplyRepairRequest,
  signal?: AbortSignal,
): Promise<ApiResult<ApplyRepairResponse>> {
  return requestProofLoop<ApplyRepairResponse>(
    "POST",
    `/api/task-world/proofloop/repairs/${encodeURIComponent(repairId)}/apply`,
    request,
    signal,
  );
}

export interface GraphGoalBinding { goal_id: string; goal_version: number; graph_id: string; contract: GoalContract }
export const getGraphGoal = (graphId: string, signal?: AbortSignal) => requestProofLoop<GraphGoalBinding>("GET", `/api/task-world/proofloop/graphs/${encodeURIComponent(graphId)}/goal`, undefined, signal);
export const getEvaluationDetail = (reportId: string, signal?: AbortSignal) => requestProofLoop<{ report: AcceptanceReport; snapshot: EvidenceSnapshot }>("GET", `/api/task-world/proofloop/evaluations/${encodeURIComponent(reportId)}`, undefined, signal);
