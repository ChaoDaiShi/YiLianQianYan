import type { Automation, AutomationSchedule } from "../../api/automations";
import type { Workspace } from "../../api/workspaces";

export interface AutomationDraft {
  name: string; targetKind: "workflow" | "personal_assistant" | "agent_turn";
  graphId: string; workspaceId: string; timezone: string; schedule: AutomationSchedule;
  goal: string; maxRuns: number;
  enabled?: boolean;
}

function validLocalDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}T(?:[01]\d|2[0-3]):[0-5]\d$/.test(value)) return false;
  const date = new Date(`${value}:00Z`);
  return !Number.isNaN(date.getTime()) && date.toISOString().slice(0, 16) === value;
}

export function isLocalAbsoluteFilePath(value: string): boolean {
  const path = value.trim().replace(/^\\\\\?\\/, "");
  if (!/^[A-Za-z]:[\\/]/.test(path) && !/^\/(?!\/)/.test(path)) return false;
  if (path.split(/[\\/]/).some(part => part === ".." || (part !== "." && /[. ]$/.test(part)))) return false;
  const withoutDrive = path.replace(/^[A-Za-z]:/, "");
  return !withoutDrive.includes(":") && !path.includes("\0") && !/[\\/]$/.test(path);
}

/** Local form feedback. Filesystem existence, DST and revision remain server checks. */
export function validateAutomationDraft(draft: AutomationDraft, workspaces: Workspace[]): string[] {
  const errors: string[] = [];
  if (!draft.name.trim()) errors.push("请输入任务名称");
  else if ([...draft.name.trim()].length > 120) errors.push("任务名称最多 120 个字符");
  if (!workspaces.some(item => item.id === draft.workspaceId && item.status === "active")) errors.push("请选择一个有效且未归档的工作空间");
  if (!draft.timezone.trim()) errors.push("请输入 IANA 时区");
  else {
    try { new Intl.DateTimeFormat("zh-CN", { timeZone: draft.timezone.trim() }).format(0); }
    catch { errors.push("请输入有效的 IANA 时区，例如 Asia/Shanghai"); }
  }
  if (draft.targetKind === "workflow" && !draft.graphId) errors.push("请选择已有可执行工作流");
  if (draft.targetKind === "personal_assistant") {
    if (!draft.goal.trim()) errors.push("请输入持续跟进的目标");
    else if ([...draft.goal.trim()].length > 8000) errors.push("个人目标最多 8000 个字符");
    if (!Number.isInteger(draft.maxRuns) || draft.maxRuns < 1 || draft.maxRuns > 20) errors.push("执行轮次预算须为 1–20 的整数");
  }
  const schedule = draft.schedule;
  if (schedule.kind === "file_changed") {
    if (draft.targetKind !== "personal_assistant") errors.push("本地文件变化仅可关联个人主动助手");
    if (!isLocalAbsoluteFilePath(schedule.path)) errors.push("监听文件须使用本地绝对文件路径，不能含上级路径、网络共享或特殊名称");
  } else if (schedule.kind === "watch") {
    if (draft.targetKind !== "personal_assistant") errors.push("主动关注仅可关联个人主动助手");
    if (!Number.isFinite(schedule.expires_at) || (draft.enabled !== false && schedule.expires_at <= Date.now())) errors.push("请输入未来的关注截止时间");
    if (!Number.isSafeInteger(schedule.min_interval_secs) || schedule.min_interval_secs < 1) errors.push("最短复查间隔须为正整数秒");
    if (!Number.isSafeInteger(schedule.max_checks) || schedule.max_checks < 1) errors.push("检查上限须为正整数");
    if (!Number.isSafeInteger(schedule.max_retries) || schedule.max_retries < 0 || schedule.max_retries > schedule.max_checks) errors.push("重试上限须在 0 与检查上限之间");
    if (schedule.source.kind === "file") {
      if (!isLocalAbsoluteFilePath(schedule.source.path)) errors.push("关注文件须使用本地绝对文件路径");
    } else {
      try {
        const url = new URL(schedule.source.url);
        if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) throw new Error();
      } catch { errors.push("请提供无账号凭据的 HTTP 或 HTTPS 网址"); }
    }
  } else if (schedule.kind === "once") {
    if (!validLocalDate(schedule.local_at)) errors.push("请输入有效的执行日期与时间");
  } else {
    if (!/^(?:[01]\d|2[0-3]):[0-5]\d$/.test(schedule.time)) errors.push("请输入有效的执行时间（HH:mm）");
    if (schedule.kind === "weekly" && (!schedule.weekdays.length || schedule.weekdays.some(day => !Number.isInteger(day) || day < 1 || day > 7))) errors.push("每周任务至少选择一个有效执行日");
  }
  return errors;
}

export function prospectiveWorkspaceRoot(value: Automation | null, workspaceId: string, workspaces: Workspace[]): { root: string; changed: boolean; workspaceRootChanged: boolean } {
  const selected = workspaces.find(item => item.id === workspaceId);
  const savedRoot = value?.target?.kind === "personal_assistant" ? value.target.root_path : undefined;
  const sameWorkspace = value?.workspace_id === workspaceId;
  const root = (sameWorkspace ? savedRoot : undefined) || selected?.root_path || "应用工作目录（保存时由服务端固定）";
  const normalize = (path: string) => path.replace(/^\\\\\?\\/, "").replace(/\\/g, "/").replace(/\/$/, "").toLocaleLowerCase();
  return { root, changed: Boolean(value && !sameWorkspace), workspaceRootChanged: Boolean(sameWorkspace && savedRoot && selected?.root_path && normalize(savedRoot) !== normalize(selected.root_path)) };
}
