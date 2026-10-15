import type {
  AcceptanceReport,
  CurrentDeliverableEvidence,
  EvidenceSnapshot,
  GoalContract,
  ProofDraft,
} from "../../../api/proofloop";
import type { TaskGraphDetail, TaskNodeExecutionSummary } from "../../../api/taskWorld";
import { formatAcceptanceReportDisplayStatus } from "../proofloop/proofloopModel";

export type GoalViewPhase =
  | "planning"
  | "needs_input"
  | "ready"
  | "running"
  | "waiting_approval"
  | "verifying"
  | "completed"
  | "failed"
  | "cancelled"
  | "needs_revalidation";

export interface GoalViewState {
  graphId: string;
  title: string;
  phase: GoalViewPhase;
  completedNodes: number;
  totalNodes: number;
  blockers: { code: string; message: string; nodeId: string | null }[];
  deliverables: {
    id: string;
    title: string;
    artifactId: string | null;
    state: "pass" | "fail" | "not_checked" | "human_pending" | "stale";
    reason: string;
  }[];
}

const ACTIVE_EXECUTION = new Set<TaskNodeExecutionSummary["status"]>([
  "dispatching", "running",
]);

function latestExecutions(detail: TaskGraphDetail | null): TaskNodeExecutionSummary[] {
  return detail?.nodes.flatMap((node) => node.latest_execution ? [node.latest_execution] : []) ?? [];
}

function isReportCurrent(
  graphId: string,
  contract: GoalContract | null,
  detail: TaskGraphDetail | null,
  report: AcceptanceReport | null,
  snapshot: EvidenceSnapshot | null,
  currentEvidence: CurrentDeliverableEvidence[] | null,
): boolean {
  const versionMatches = Boolean(
    contract && detail && report
    && report.graph_id === graphId
    && report.goal_version === contract.version
    && report.graph_revision === detail.revision,
  );
  if (!versionMatches || !contract || !report) return false;
  if (formatAcceptanceReportDisplayStatus(report, contract).needsReevaluation) return false;

  const mappings = new Map((snapshot?.deliverables ?? []).map((mapping) => [mapping.deliverable_id, mapping.artifact_id]));
  return contract.deliverables.every((deliverable) => {
    const artifactId = mappings.get(deliverable.id);
    const current = currentEvidence?.find((item) => item.deliverable_id === deliverable.id);
    return Boolean(
      artifactId
      && current?.status === "current"
      && current.artifact_id === artifactId,
    );
  });
}

function phaseFor(
  graphId: string,
  contract: GoalContract | null,
  draft: ProofDraft | null,
  detail: TaskGraphDetail | null,
  report: AcceptanceReport | null,
  snapshot: EvidenceSnapshot | null,
  currentEvidence: CurrentDeliverableEvidence[] | null,
): GoalViewPhase {
  const executions = latestExecutions(detail);
  const active = executions.some((execution) => ACTIVE_EXECUTION.has(execution.status));
  const cancelled = Boolean(
    detail?.nodes.length
    && detail.nodes.some((node) => node.status === "cancelled" || node.latest_execution?.status === "cancelled")
    && detail.nodes.every((node) => node.status === "cancelled" || node.status === "succeeded"),
  );
  if (cancelled) return "cancelled";

  const reportCurrent = isReportCurrent(graphId, contract, detail, report, snapshot, currentEvidence);
  if (report && !reportCurrent) return "needs_revalidation";
  if (report?.results.some((result) => result.status === "stale")) return "needs_revalidation";

  if (executions.some((execution) => execution.status === "waiting_approval")) return "waiting_approval";
  if (detail?.nodes.some((node) => node.kind === "user_checkpoint" && node.status !== "succeeded" && node.status !== "cancelled")) return "needs_input";
  if (draft?.compilation.status === "needs_input") return "needs_input";
  if (detail?.nodes.some((node) => node.status === "blocked")) return "needs_input";
  if (active) return "running";
  if (executions.some((execution) => execution.status === "validating")) return "verifying";

  if (reportCurrent) {
    if (report?.status === "pass") return "completed";
    if (report?.status === "fail") return "failed";
    return "verifying";
  }
  if (detail?.nodes.some((node) => node.status === "failed" || node.latest_execution?.status === "failed")) return "failed";
  if (detail?.nodes.length && detail.nodes.every((node) => node.status === "succeeded")) return "verifying";
  if (draft?.compilation.status === "invalid") return "needs_input";
  if (contract && detail) return "ready";
  return "planning";
}

function buildBlockers(
  phase: GoalViewPhase,
  detail: TaskGraphDetail | null,
  report: AcceptanceReport | null,
  reportCurrent: boolean,
): GoalViewState["blockers"] {
  const blockers: GoalViewState["blockers"] = [];
  if (phase === "needs_revalidation") blockers.push({ code: "proofloop_evidence_stale", message: "任务图或目标已更新，现有验收需要重新核验。", nodeId: null });

  for (const node of detail?.nodes ?? []) {
    if (node.latest_execution?.status === "waiting_approval") {
      blockers.push({ code: "task_waiting_approval", message: `${node.title}正在等待审批。`, nodeId: node.id });
    } else if (node.kind === "user_checkpoint" && node.status !== "succeeded" && node.status !== "cancelled") {
      blockers.push({ code: "task_needs_input", message: node.instruction_summary || `${node.title}需要补充信息。`, nodeId: node.id });
    } else if (node.latest_execution?.status === "failed" || node.status === "failed") {
      blockers.push({ code: node.latest_execution?.failure_code || "task_execution_failed", message: node.latest_execution?.error || node.state.error || `${node.title}执行失败。`, nodeId: node.id });
    } else if (node.status === "blocked") {
      blockers.push({ code: "task_node_blocked", message: node.state.error || `${node.title}当前被阻塞。`, nodeId: node.id });
    }
  }

  if (report && reportCurrent) {
    for (const result of report.results) {
      if (result.status === "fail" || result.status === "human_pending" || result.status === "not_checked") {
        blockers.push({ code: `criterion_${result.status}`, message: result.reason, nodeId: null });
      }
    }
    for (const result of report.deliverable_results) {
      if (result.status === "fail") blockers.push({ code: result.code, message: result.reason, nodeId: null });
    }
  } else if (phase === "verifying") {
    blockers.push({ code: "proofloop_acceptance_pending", message: "任务步骤已完成，等待目标和交付物验收。", nodeId: null });
  }
  return blockers;
}

export function buildGoalViewState(
  graphId: string,
  contract: GoalContract | null,
  detail: TaskGraphDetail | null,
  report: AcceptanceReport | null,
  snapshot: EvidenceSnapshot | null,
  draft: ProofDraft | null = null,
  currentEvidence: CurrentDeliverableEvidence[] | null = null,
): GoalViewState {
  const reportCurrent = isReportCurrent(graphId, contract, detail, report, snapshot, currentEvidence);
  const phase = phaseFor(graphId, contract, draft, detail, report, snapshot, currentEvidence);
  const deliverableResults = new Map((report?.deliverable_results ?? []).map((result) => [result.deliverable_id, result]));
  const artifactIds = new Map((snapshot?.deliverables ?? []).map((mapping) => [mapping.deliverable_id, mapping.artifact_id]));
  const deliverables = contract?.deliverables.map((deliverable) => {
    const result = deliverableResults.get(deliverable.id);
    const state: GoalViewState["deliverables"][number]["state"] = !reportCurrent
      ? report ? "stale" : "not_checked"
      : result?.status ?? "not_checked";
    return {
      id: deliverable.id,
      title: deliverable.title,
      artifactId: artifactIds.get(deliverable.id) ?? null,
      state,
      reason: report && reportCurrent ? result?.reason || "尚无此交付物的验收结果。" : report ? "目标、交付物或实际文件状态已变化，请重新验收。" : "尚未进行目标验收。",
    };
  }) ?? [];

  return {
    graphId,
    title: contract?.user_goal || draft?.contract.user_goal || "当前任务图未关联可验证目标",
    phase,
    completedNodes: detail?.nodes.filter((node) => node.status === "succeeded" || node.latest_execution?.status === "succeeded").length ?? 0,
    totalNodes: detail?.nodes.length ?? 0,
    blockers: buildBlockers(phase, detail, report, reportCurrent),
    deliverables,
  };
}
