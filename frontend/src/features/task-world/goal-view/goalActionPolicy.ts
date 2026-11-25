import type { TaskGraphDetail, TaskNodeDetail } from "../../../api/taskWorld";
import type { GoalTrackingData } from "./goalTracking";
import { buildGoalViewState } from "./goalViewModel";

const activeStatuses = ["pending", "ready", "dispatching", "running", "waiting_approval", "validating"];
const graphPath = (id: string, node?: string) => `/task-world/${encodeURIComponent(id)}${node ? `?node=${encodeURIComponent(node)}` : ""}`;
export interface GoalNextAction { kind: "navigate" | "verify" | "resume" | "refresh" | "none"; label: string; reason: string; href?: string }

/** Presentation decisions only. Actual execution and permission checks stay in the backend. */
export function goalActionPolicy(data: GoalTrackingData, readError?: string | null) {
  const { detail, binding, report, snapshot, currentEvidence } = data;
  const state = buildGoalViewState(detail.graph_id, binding?.contract ?? null, detail, report, snapshot, null, currentEvidence, binding?.conversations);
  const budgetError = binding?.budget_error?.message ?? null;
  const executions = detail.nodes.flatMap(node => node.latest_execution ? [node.latest_execution] : []);
  const active = executions.some(item => activeStatuses.includes(item.status)) || binding?.conversations?.some(item => ["running", "waiting_input", "waiting_approval"].includes(item.run_status));
  const cancelled = detail.control_state === "cancelled" || state.phase === "cancelled";
  const paused = detail.control_state === "paused";
  const canVerify = !readError && !cancelled && !paused && !active && executions.length > 0 && !executions.some(item => item.status === "stale" || item.status === "cancelled");
  const canResume = !readError && !budgetError && paused;
  const canPause = !readError && !cancelled && !paused && Boolean(active);
  // A transient observation failure must not disable the user's stop action.
  const canCancel = !cancelled && (state.phase !== "completed" || Boolean(readError));
  let next: GoalNextAction = { kind: "none", label: "", reason: "当前无需操作。" };
  if (readError) next = { kind: "refresh", label: "重新读取", reason: "当前状态尚未确认，重新读取后再继续执行。" };
  else if (cancelled) next.reason = "任务已取消，不再启动后续动作。";
  else if (paused) next = canResume ? { kind: "resume", label: "恢复任务", reason: "恢复已有任务的调度，工具仍经过原有权限检查。" } : { kind: "navigate", label: "查看任务约束", reason: budgetError!, href: `${graphPath(detail.graph_id)}?panel=proofloop` };
  else if (state.phase === "waiting_approval") next = { kind: "navigate", label: "处理审批", reason: "先核对具体工具、目标和风险，再决定批准或拒绝。", href: "/system?card=approvals" };
  else if (state.phase !== "running") {
    const chat = state.blockers.find(item => item.conversationId);
    const node = state.blockers.find(item => item.nodeId);
    if (chat) next = { kind: "navigate", label: "返回任务对话", reason: chat.message, href: `/chat/${encodeURIComponent(chat.conversationId!)}` };
    else if (node) next = { kind: "navigate", label: state.phase === "failed" ? "定位失败并恢复" : "处理任务步骤", reason: node.message, href: graphPath(detail.graph_id, node.nodeId!) };
    else if (state.blockers.some(item => item.code === "criterion_human_pending")) next = { kind: "navigate", label: "人工复核成果", reason: "需要按目标验收条件复核，不能由完成状态代替决定。", href: `${graphPath(detail.graph_id)}?panel=proofloop` };
    else if (state.phase === "completed") next = { kind: "none", label: "", reason: "当前产物已通过验收，可在详情中下载。" };
    else if (canVerify) next = { kind: "verify", label: "核验产物", reason: "核对实际文件、来源和当前执行记录。" };
    else if (state.phase === "ready") next = { kind: "navigate", label: "查看并执行方案", reason: "在任务画布选择可执行步骤。", href: graphPath(detail.graph_id) };
  }
  return { state, next, canVerify, canResume, canPause, canCancel, blockedReason: readError || budgetError };
}

export function goalNodeAction(detail: TaskGraphDetail, node: TaskNodeDetail | null, reason?: string | null): { allowed: boolean; reason: string } {
  if (reason) return { allowed: false, reason };
  if (detail.control_state === "cancelled") return { allowed: false, reason: "任务已取消，不能继续执行。" };
  if (detail.control_state === "paused") return { allowed: false, reason: "请先恢复任务。" };
  if (!node) return { allowed: false, reason: "请先选择任务步骤。" };
  if (detail.nodes.some(item => item.latest_execution && activeStatuses.includes(item.latest_execution.status))) return { allowed: false, reason: "已有步骤正在执行或等待审批，请先处理当前步骤。" };
  if (!node.executor_ref) return { allowed: false, reason: "请先为该步骤配置执行能力。" };
  const dependenciesReady = detail.edges.filter(edge => edge.to === node.id).every(edge => {
    const dependency = detail.nodes.find(item => item.id === edge.from);
    return dependency?.status === "succeeded" && dependency.latest_execution?.status !== "stale";
  });
  if (!dependenciesReady) return { allowed: false, reason: "请先完成该步骤的上游依赖。" };
  if (node.status !== "runnable" && node.status !== "invalidated") return { allowed: false, reason: node.status === "failed" || node.latest_execution?.status === "stale" ? "请先检查输入并准备重跑。" : "该步骤尚未满足执行条件或依赖。" };
  return { allowed: true, reason: "执行仍受当前权限与审批设置约束。" };
}
