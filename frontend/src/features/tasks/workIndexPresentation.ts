import type { WorkIndexItem, WorkKind } from "../../api/workIndex";
import { deriveTaskDisplayTitle } from "../../components/chat/taskTitle";

export const workKindLabels: Record<WorkKind, string> = {
  conversation: "对话", legacy_task: "任务", task_graph: "任务图", personal_delegation: "个人委托",
};
export const workStatusLabels: Record<string, string> = {
  draft: "草稿", idle: "待开始", pending: "待开始", ready: "待执行", runnable: "待执行", queued: "排队中",
  running: "进行中", working: "进行中", dispatching: "启动中", validating: "核验中", waiting_approval: "等待审批",
  waiting_input: "等待输入", waiting_user: "等待输入", blocked: "已阻塞", completed: "已完成",
  succeeded: "已完成", reported: "本轮已汇报", failed: "失败", interrupted: "已中断",
  cancelled: "已取消", stopped: "已停止", paused: "已暂停", archived: "已归档", needs_review: "需要核对",
  scheduled: "已安排", skipped: "已跳过",
};
const running = new Set(["running", "working", "dispatching", "validating", "waiting_approval", "waiting_input", "waiting_user", "queued"]);
export type WorkFilter = "all" | "active" | "attention" | "completed";
export function workKey(item: Pick<WorkIndexItem, "kind" | "id">): string { return `${item.kind}:${item.id}`; }
export function workDisplayTitle(item: WorkIndexItem): string {
  const title = item.title.trim();
  if (item.kind === "task_graph" && (!title || title === item.id || title === `Task graph ${item.id}`)) return "未命名任务画布";
  if (!title || title === item.id) return `未命名${workKindLabels[item.kind]}`;
  return item.kind === "conversation" || item.kind === "legacy_task" ? deriveTaskDisplayTitle(title) : title;
}
export function matchesWork(item: WorkIndexItem, query: string, filter: WorkFilter, kind: WorkKind | "all"): boolean {
  if (kind !== "all" && item.kind !== kind) return false;
  if (filter === "active" && !running.has(item.status)) return false;
  if (filter === "attention" && !item.attention_required) return false;
  if (filter === "completed" && !["completed", "succeeded", "reported"].includes(item.status)) return false;
  return !query.trim() || [workDisplayTitle(item), item.title, item.id, workKindLabels[item.kind], workStatusLabels[item.status] ?? item.status]
    .join(" ").toLocaleLowerCase().includes(query.trim().toLocaleLowerCase());
}
