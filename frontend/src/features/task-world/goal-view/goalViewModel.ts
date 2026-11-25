import type {
  AcceptanceReport,
  CurrentDeliverableEvidence,
  EvidenceSnapshot,
  GoalContract,
  GoalConversation,
  ProofDraft,
} from "../../../api/proofloop";
import type { TaskGraphDetail, TaskNodeExecutionSummary } from "../../../api/taskWorld";
import { formatAcceptanceReportDisplayStatus } from "../proofloop/proofloopModel";

export type GoalViewPhase =
  | "planning"
  | "needs_input"
  | "ready"
  | "running"
  | "paused"
  | "waiting_approval"
  | "verifying"
  | "completed"
  | "failed"
  | "cancelled"
  | "needs_revalidation";

export const GOAL_PHASE_LABELS: Record<GoalViewPhase, string> = {
  planning: "规划中", ready: "等待开始", running: "执行中", paused: "已暂停",
  needs_input: "需要补充信息", waiting_approval: "等待审批", verifying: "验收中",
  completed: "已通过验收", failed: "未通过", cancelled: "已取消", needs_revalidation: "需要重新验收",
};

export interface GoalViewState {
  graphId: string;
  title: string;
  phase: GoalViewPhase;
  completedNodes: number;
  totalNodes: number;
  blockers: { code: string; message: string; nodeId: string | null; conversationId?: string }[];
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
  if (report.goal_id !== contract.goal_id || snapshot?.goal_id !== contract.goal_id
    || snapshot.goal_version !== contract.version || snapshot.graph_id !== graphId
    || snapshot.graph_revision !== detail?.revision) return false;
  if (detail?.nodes.some((node) => node.latest_execution && !snapshot.executions.some((execution) =>
    execution.node_id === node.id && execution.execution_id === node.latest_execution?.execution_id))) return false;
  if (formatAcceptanceReportDisplayStatus(report, contract).needsReevaluation) return false;

  const mappings = new Map((snapshot?.deliverables ?? []).map((mapping) => [mapping.deliverable_id, mapping.artifact_id]));
  return contract.deliverables.every((deliverable) => {
    const result = report.deliverable_results.find((item) => item.deliverable_id === deliverable.id);
    if (report.status !== "pass" && result?.status === "fail") return true;
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
  conversations: GoalConversation[],
): GoalViewPhase {
  const executions = latestExecutions(detail);
  const active = executions.some((execution) => ACTIVE_EXECUTION.has(execution.status));
  if (detail?.control_state === "cancelled") return "cancelled";
  if (detail?.control_state === "paused") return "paused";
  const cancelled = Boolean(
    detail?.nodes.length
    && detail.nodes.some((node) => node.status === "cancelled" || node.latest_execution?.status === "cancelled")
    && !active && !executions.some((execution) => execution.status === "waiting_approval" || execution.status === "validating"),
  );
  if (cancelled) return "cancelled";

  const reportCurrent = isReportCurrent(graphId, contract, detail, report, snapshot, currentEvidence);
  if (executions.some((execution) => execution.status === "waiting_approval") || conversations.some(item => item.run_status === "waiting_approval")) return "waiting_approval";
  if (active || conversations.some(item => item.run_status === "running")) return "running";
  if (conversations.some(item => item.run_status === "waiting_input")) return "needs_input";
  if (executions.some((execution) => execution.status === "validating")) return "verifying";
  if (detail?.nodes.some((node) => node.status === "failed" || node.latest_execution?.status === "failed")) return "failed";
  if (detail?.nodes.some((node) => node.kind === "user_checkpoint" && (node.status === "runnable" || node.status === "blocked"))) return "needs_input";
  if (draft?.compilation.status === "needs_input") return "needs_input";
  if (detail?.nodes.some((node) => node.status === "blocked")) return "needs_input";

  const chatFailed = conversations.some(item =>
    (item.run_status === "failed" || item.run_status === "interrupted")
    && (!reportCurrent || report?.status !== "pass" || (item.run_finished_at ?? item.run_started_at ?? Infinity) > report.checked_at_ms));
  if (chatFailed) return "failed";
  if (executions.some(item => item.status === "stale") || detail?.nodes.some(node => node.status === "invalidated")) return "needs_revalidation";

  if (report && !reportCurrent) return "needs_revalidation";
  if (report?.results.some((result) => result.status === "stale")) return "needs_revalidation";

  if (reportCurrent) {
    if (report?.status === "pass") return "completed";
    if (report?.status === "fail") return "failed";
    return "verifying";
  }
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
  conversations: GoalConversation[],
): GoalViewState["blockers"] {
  const blockers: GoalViewState["blockers"] = [];
  if (phase === "cancelled") return blockers;
  if (phase === "paused") return [{ code: "task_paused", message: "任务已暂停，需要恢复后才能继续执行。", nodeId: null }];
  for (const conversation of conversations) {
    if (phase === "completed" && (conversation.run_status === "failed" || conversation.run_status === "interrupted")) continue;
    if (conversation.run_status === "waiting_approval" || conversation.run_status === "waiting_input" || conversation.run_status === "failed" || conversation.run_status === "interrupted") {
      const label = conversation.run_status === "waiting_approval" ? "等待审批" : conversation.run_status === "waiting_input" ? "等待补充信息" : conversation.run_status === "interrupted" ? "执行中断，请回到对话恢复" : "执行失败，请回到对话重试";
      blockers.push({ code: `goal_chat_${conversation.run_status}`, message: `${conversation.title}：${conversation.run_error || label}`, nodeId: null, conversationId: conversation.conversation_id });
    }
  }
  if (phase === "needs_revalidation") blockers.push({ code: "proofloop_evidence_stale", message: "任务图或目标已更新，现有验收需要重新核验。", nodeId: null });

  for (const node of detail?.nodes ?? []) {
    if (node.latest_execution?.status === "waiting_approval") {
      blockers.push({ code: "task_waiting_approval", message: `${node.title}正在等待审批。`, nodeId: node.id });
    } else if (node.kind === "user_checkpoint" && (node.status === "runnable" || node.status === "blocked")) {
      blockers.push({ code: "task_needs_input", message: node.instruction_summary || `${node.title}需要补充信息。`, nodeId: node.id });
    } else if (node.latest_execution?.status === "failed" || node.status === "failed") {
      blockers.push({ code: node.latest_execution?.failure_code || "task_execution_failed", message: node.latest_execution?.error || node.state.error || `${node.title}执行失败。`, nodeId: node.id });
    } else if (node.latest_execution?.status === "stale" || node.status === "invalidated") {
      blockers.push({ code: "task_execution_stale", message: `${node.title}的执行记录已失效，请检查输入并从此节点重跑后验收。`, nodeId: node.id });
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
  conversations: GoalConversation[] = [],
): GoalViewState {
  const reportCurrent = isReportCurrent(graphId, contract, detail, report, snapshot, currentEvidence);
  const phase = phaseFor(graphId, contract, draft, detail, report, snapshot, currentEvidence, conversations);
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
    blockers: buildBlockers(phase, detail, report, reportCurrent, conversations),
    deliverables,
  };
}
