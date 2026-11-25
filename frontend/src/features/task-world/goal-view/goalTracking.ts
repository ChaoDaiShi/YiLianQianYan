import { getTaskGraphDetail, type TaskGraphDetail } from "../../../api/taskWorld";
import {
  evaluateGoal, getEvaluationDetail, getGraphGoal, listGoalEvaluations,
  type AcceptanceReport, type CurrentDeliverableEvidence, type EvidenceSnapshot, type GraphGoalBinding,
} from "../../../api/proofloop";
import { listArtifactResults, type ArtifactRecord } from "../../../api/evidence";

export interface GoalTrackingData {
  observedAt?: number;
  detail: TaskGraphDetail;
  binding: GraphGoalBinding | null;
  report: AcceptanceReport | null;
  snapshot: EvidenceSnapshot | null;
  currentEvidence: CurrentDeliverableEvidence[] | null;
  artifacts: ArtifactRecord[];
  artifactError: string | null;
}

/** Reads the existing durable goal and executions; no second task state is stored. */
export async function loadGoalTracking(graphId: string, options: { detail?: TaskGraphDetail; signal?: AbortSignal; force?: boolean; observeOnly?: boolean } = {}): Promise<GoalTrackingData> {
  const observedAt = Date.now();
  const { signal, force } = options;
  const [graph, goal] = await Promise.all([
    options.detail ? Promise.resolve({ ok: true as const, data: options.detail }) : getTaskGraphDetail(graphId),
    getGraphGoal(graphId, signal),
  ]);
  if (!graph.ok) throw new Error(graph.error.message);
  if (!goal.ok && goal.error.status !== 404) throw new Error(goal.error.message);
  const data: GoalTrackingData = { observedAt, detail: graph.data, binding: goal.ok ? goal.data : null, report: null, snapshot: null, currentEvidence: null, artifacts: [], artifactError: null };
  if (!data.binding) return data;
  const { detail, binding } = data;
  const reports = await listGoalEvaluations(binding.goal_id, signal);
  if (!reports.ok) throw new Error(reports.error.message);
  const executions = detail.nodes.flatMap(node => node.latest_execution ? [{ nodeId: node.id, ...node.latest_execution }] : []);
  let report = reports.data.reports.find(item => item.graph_id === graphId && item.graph_revision === detail.revision && item.goal_version === binding.goal_version);
  let evaluation = report ? await getEvaluationDetail(report.report_id, signal) : null;
  if (evaluation && !evaluation.ok) throw new Error(evaluation.error.message);
  const active = executions.some(item => ["pending", "ready", "dispatching", "running", "waiting_approval", "validating"].includes(item.status))
    || binding.conversations?.some(item => ["running", "waiting_approval", "waiting_input"].includes(item.run_status));
  const canEvaluate = executions.length > 0 && !active && detail.control_state !== "cancelled" && detail.control_state !== "paused" && !executions.some(item => item.status === "cancelled" || item.status === "stale");
  if (!options.observeOnly && force && canEvaluate) {
    const checked = await evaluateGoal(binding.goal_id, { goal_version: binding.goal_version, graph_revision: detail.revision, execution_ids: executions.map(item => item.execution_id) }, signal);
    if (!checked.ok) throw new Error(checked.error.message);
    report = checked.data;
    evaluation = await getEvaluationDetail(report.report_id, signal);
    if (!evaluation.ok) throw new Error(evaluation.error.message);
  }
  data.report = report ?? null;
  if (evaluation?.ok) {
    data.snapshot = evaluation.data.snapshot;
    data.currentEvidence = evaluation.data.current_evidence ?? [];
    try {
      const origins = [...new Map(evaluation.data.snapshot.executions.map(item => [item.execution_id, item])).values()];
      data.artifacts = (await Promise.all(origins.map(origin => listArtifactResults({ kind: "node", graph_id: origin.graph_id, node_id: origin.node_id, execution_id: origin.execution_id })))).flat();
    } catch (error) {
      data.artifactError = error instanceof Error ? error.message : String(error);
    }
  }
  return data;
}
