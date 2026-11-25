import type { AutomationOccurrence, PersonalStartInput } from "../../api/automations";

export const automationStatusLabels: Record<string, string> = {
  dispatching: "正在启动", created: "已创建", running: "运行中", waiting_approval: "等待审批",
  waiting_input: "等待你的回答", awaiting_approval: "等待审阅", queued: "等待运行", stopped: "已停止",
  completed: "本轮已汇报", failed: "失败", cancelled: "已取消", interrupted: "已中断", skipped: "已跳过",
  blocked: "等待人工检查", needs_review: "结果需核实",
};

export const automationTriggerLabels: Record<AutomationOccurrence["trigger"], string> = {
  manual: "手动运行", scheduled: "计划触发", file_changed: "文件内容变化", attention: "主动关注",
};

export function occurrenceTime(value: number, timezone?: string): string {
  if (!Number.isFinite(value)) return "时间不可用";
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return "时间不可用";
  try { return date.toLocaleString("zh-CN", timezone ? { timeZone: timezone } : undefined); }
  catch { return date.toLocaleString("zh-CN"); }
}

/** Keep chronological page order while accepting fresher versions of duplicate rows. */
export function mergeOccurrencePages(previous: AutomationOccurrence[], incoming: AutomationOccurrence[]): AutomationOccurrence[] {
  const positions = new Map(previous.map((item, index) => [item.id, index]));
  const merged = [...previous];
  for (const item of incoming) {
    const position = positions.get(item.id);
    if (position === undefined) { positions.set(item.id, merged.length); merged.push(item); }
    else merged[position] = item;
  }
  return merged;
}

const pendingKey = (id: string) => `yilian.personal.start.v1:${id}`;
export function restorePendingPersonalStart(id: string): PersonalStartInput | null {
  try {
    const value: unknown = JSON.parse(sessionStorage.getItem(pendingKey(id)) || "null");
    if (!value || typeof value !== "object") return null;
    const row = value as Record<string, unknown>;
    if (!Number.isSafeInteger(row.expected_revision) || Number(row.expected_revision) < 1
      || typeof row.request_id !== "string" || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(row.request_id)) return null;
    if (row.reviewed_occurrence_id !== undefined && typeof row.reviewed_occurrence_id !== "string") return null;
    if (row.acknowledged !== undefined && typeof row.acknowledged !== "boolean") return null;
    return { expected_revision: Number(row.expected_revision), request_id: row.request_id,
      ...(typeof row.reviewed_occurrence_id === "string" ? { reviewed_occurrence_id: row.reviewed_occurrence_id } : {}),
      ...(typeof row.acknowledged === "boolean" ? { acknowledged: row.acknowledged } : {}) };
  } catch { return null; }
}
export function savePendingPersonalStart(id: string, input: PersonalStartInput | null): void {
  try { if (input) sessionStorage.setItem(pendingKey(id), JSON.stringify(input)); else sessionStorage.removeItem(pendingKey(id)); }
  catch { /* Optional UI recovery; server idempotency remains authoritative. */ }
}

export function personalStateGuidance(status: string | undefined, archived: boolean, budgetExhausted: boolean): string {
  if (archived) return "委托已归档，保留历史与生成文件。恢复归档后仍暂停，不会自动启动。";
  if (status === "waiting_approval") return "当前操作尚未获批。请在原任务对话核对具体操作，批准或拒绝；不要重新启动。";
  if (status === "waiting_input") return "当前轮次需要你的回答。请回到原任务对话回答，继续使用同一轮次。";
  if (status === "running" || status === "dispatching") return "当前轮次正在处理。暂停计划只停止后续触发；停止委托会取消当前轮次，但不回滚已完成的文件操作。";
  if (budgetExhausted) return "执行轮次预算已用完。核对历史后可在列表编辑预算，上限仍为 20 轮。";
  if (["failed", "cancelled", "interrupted", "needs_review", "blocked"].includes(status || "")) return "请先核实文件和执行记录。明确确认后只开启新一轮，不会重放旧工具调用，也不保证撤销已完成的操作。";
  if (status === "completed") return "本轮报告已保存；这不等于长期目标已经全部完成。可查看结果，或按原权限继续新一轮。";
  return "尚未开始执行。核对目标、操作范围和准备检查后，可运行一轮或等待后续触发。";
}
