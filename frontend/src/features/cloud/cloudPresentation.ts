import type { CloudIdentity, CloudSyncStatus, LifecycleJob, SearchResult } from "../../api/cloud";

export type PresentationTone = "default" | "success" | "warning" | "danger" | "info" | "accent";

export function identityPresentation(identity: CloudIdentity): {
  label: string;
  tone: PresentationTone;
  description: string;
} {
  switch (identity.status) {
    case "authenticated":
      return { label: "已登录", tone: "success", description: "本地工作区可以按你的选择同步到账号。" };
    case "offline_authenticated":
      return { label: "离线工作", tone: "warning", description: "本地功能保持可用，联网后继续同步。" };
    case "closing":
      return { label: "账号注销中", tone: "warning", description: "本地数据仍由你控制，可查看云端删除进度。" };
    default:
      return { label: "仅本地", tone: "default", description: "核心功能可用，登录后可启用跨设备同步。" };
  }
}

export function syncPresentation(sync: CloudSyncStatus): {
  label: string;
  tone: PresentationTone;
  detail: string;
} {
  if (sync.conflict_count > 0 || sync.error_count > 0) {
    return {
      label: "需要处理",
      tone: "warning",
      detail: `${sync.pending_count} 项待同步 · ${sync.conflict_count} 项冲突 · ${sync.error_count} 项失败`,
    };
  }
  if (sync.pending_count > 0) {
    return { label: "等待同步", tone: "info", detail: `${sync.pending_count} 项本地变更等待上传` };
  }
  return {
    label: sync.last_success_at ? "已同步" : "尚未同步",
    tone: sync.last_success_at ? "success" : "default",
    detail: sync.last_success_at ? `上次成功：${new Date(sync.last_success_at).toLocaleString()}` : "当前没有待同步项目",
  };
}

export function wizardCanFinish(readiness: {
  identity: boolean;
  model: boolean;
  search: boolean;
  stt: boolean;
  tts: boolean;
}): boolean {
  return readiness.model;
}

export function buildSearchChatPrompt(result: SearchResult): string {
  return `请总结并核实以下搜索结果，说明关键结论、证据与仍需确认的部分。\n\n标题：${result.title}\n来源：${result.url}\n摘要：${result.snippet || "暂无摘要"}`;
}

export function buildSearchTaskDraft(result: SearchResult): { title: string; description: string } {
  return {
    title: `研究：${result.title}`,
    description: `${result.snippet || "整理并核实该搜索结果。"}\n\n来源：${result.url}`,
  };
}

export function buildSearchKnowledgeRecord(result: SearchResult): {
  content: string;
  category: "knowledge";
  source: "ai_search";
  metadata: string;
} {
  return {
    content: `${result.title}\n${result.snippet || result.content || ""}\n来源：${result.url}`.trim(),
    category: "knowledge",
    source: "ai_search",
    metadata: JSON.stringify({ title: result.title, url: result.url }),
  };
}

export function lifecyclePresentation(job: LifecycleJob): {
  title: string;
  label: string;
  tone: PresentationTone;
  description: string;
  isTerminal: boolean;
  canDownload: boolean;
  waitingForBackups: boolean;
} {
  const isExport = job.kind === "export";
  const title = isExport ? "云端数据导出任务" : "云端账号注销任务";
  const canDownload = isExport && job.status === "completed";
  const waitingForBackups = job.kind === "deletion" && job.status === "waiting_backup_expiry";
  const isTerminal = job.status === "completed" || job.status === "failed";

  switch (job.status) {
    case "queued":
      return {
        title,
        label: "排队中",
        tone: "default",
        description: "任务已排队，等待空闲工作节点领取",
        isTerminal,
        canDownload,
        waitingForBackups,
      };
    case "running":
      return {
        title,
        label: "处理中",
        tone: "accent",
        description: "工作进程已锁定并执行流式处理",
        isTerminal,
        canDownload,
        waitingForBackups,
      };
    case "waiting_for_producers":
      return {
        title,
        label: "等待写操作排空",
        tone: "warning",
        description: "正在等待进行中的导出或同步写入退出",
        isTerminal,
        canDownload,
        waitingForBackups,
      };
    case "waiting_backup_expiry":
      return {
        title,
        label: "等待备份到期擦除",
        tone: "warning",
        description: "活跃数据已清理，正在等待备份保留周期到期",
        isTerminal: false,
        canDownload,
        waitingForBackups: true,
      };
    case "retry_wait":
      return {
        title,
        label: "等待重试",
        tone: "danger",
        description: "遇到临时异常，系统进入退避等待，稍后重试",
        isTerminal,
        canDownload,
        waitingForBackups,
      };
    case "completed":
      return {
        title,
        label: "已完成",
        tone: "success",
        description: "任务目标已全部达成并通过一致性复核",
        isTerminal: true,
        canDownload,
        waitingForBackups: false,
      };
    case "failed":
      return {
        title,
        label: "执行失败",
        tone: "danger",
        description: "重试次数达到上限或遇到不可恢复错误",
        isTerminal: true,
        canDownload,
        waitingForBackups: false,
      };
    default:
      return {
        title,
        label: job.status,
        tone: "default",
        description: "任务状态更新中",
        isTerminal,
        canDownload,
        waitingForBackups,
      };
  }
}
