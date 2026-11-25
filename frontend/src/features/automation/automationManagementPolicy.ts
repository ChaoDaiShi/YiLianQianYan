import type { Automation } from "../../api/automations";

export type AutomationFilter = "all" | "enabled" | "paused" | "attention" | "archived";
const activeStates = new Set(["dispatching", "created", "queued", "running", "waiting_approval", "awaiting_approval", "waiting_input"]);
const waitingStates = new Set(["waiting_approval", "awaiting_approval", "waiting_input"]);
const reviewStates = new Set(["failed", "cancelled", "interrupted", "needs_review", "blocked", "stopped"]);

/** Presentation policy only; the server rechecks current state for every action. */
export function automationManagementPolicy(item: Automation) {
  const personal = item.target?.kind === "personal_assistant";
  const status = item.runtime?.latest_status ?? item.assistant_activity?.latest?.status ?? "";
  const archived = Boolean(item.archived);
  const active = activeStates.has(status);
  const waiting = waitingStates.has(status);
  const requiresReview = personal && reviewStates.has(status);
  const exhausted = personal && (item.assistant_activity?.used_runs ?? 0) >= (item.target?.kind === "personal_assistant" ? item.target.max_runs : Infinity);
  const unknown = personal && Boolean(status) && !activeStates.has(status) && !reviewStates.has(status) && !["completed", "skipped"].includes(status);
  const sourceStatus = item.attention?.status;
  const attentionBadge = sourceStatus === "blocked" ? "关注来源受阻"
    : sourceStatus === "conflict" ? "观察有冲突"
    : sourceStatus === "proposed" || item.attention?.proposal ? "有目标提议" : null;
  const needsAttention = waiting || (personal && (requiresReview || exhausted || unknown || Boolean(item.assistant_activity?.source_error) || Boolean(attentionBadge)));
  const runReason = archived ? "已归档，恢复为暂停状态后才能继续" : waiting ? "请先在原任务对话处理审批或回答" : active ? "当前轮次尚未结束，请查看任务详情" : exhausted ? "执行轮次预算已用完，请先编辑预算" : unknown ? "状态暂不支持直接运行，请查看详情并刷新" : requiresReview ? "先核对上轮结果，再从任务详情显式继续新轮次" : "先查看准备情况，再开始一轮";
  return {
    personal, status, archived, active, waiting, requiresReview, exhausted, needsAttention, attentionBadge,
    canEdit: !archived && !active,
    canOpenRun: !archived && !active && !exhausted && !unknown,
    canStop: !archived && (active || item.enabled || requiresReview),
    canToggleSchedule: !archived && (item.enabled || !active),
    runReason,
    editReason: archived ? "先恢复归档的委托" : active ? "请先停止当前轮次，再修改目标或操作范围" : "编辑任务",
    scheduleReason: archived ? "归档的委托不能启用计划" : !item.enabled && active ? "当前轮次尚未结束；暂停后请等待结束或停止，再恢复计划" : item.enabled ? "只暂停后续触发，不停止当前轮次" : "只恢复未来计划，不重放失败或中断的操作",
  };
}

export function matchesAutomation(item: Automation, filter: AutomationFilter, query: string): boolean {
  const policy = automationManagementPolicy(item);
  if (filter === "archived" ? !policy.archived : policy.archived) return false;
  if (filter === "enabled" && !item.enabled) return false;
  if (filter === "paused" && item.enabled) return false;
  if (filter === "attention" && !policy.needsAttention) return false;
  const text = [item.name, item.target?.kind === "personal_assistant" ? item.target.goal : ""].join("\n").toLocaleLowerCase();
  return text.includes(query.trim().toLocaleLowerCase());
}
