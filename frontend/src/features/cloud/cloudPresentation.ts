import type { CloudIdentity, CloudIdentityResponse, LifecycleJob, SearchResult } from "../../api/cloud";

export type PresentationTone = "default" | "success" | "warning" | "danger" | "info" | "accent";

export function identityPresentation(identity: CloudIdentity): {
  label: string;
  tone: PresentationTone;
} {
  switch (identity.status) {
    case "authenticated":
      return { label: "已登录", tone: "success" };
    case "offline_authenticated":
      return { label: "离线工作", tone: "warning" };
    case "closing":
      return { label: "账号注销中", tone: "warning" };
    default:
      return { label: "仅本地", tone: "default" };
  }
}

export function syncPresentation({ identity, sync, sync_existing_choice: choice }: CloudIdentityResponse): {
  label: string;
  tone: PresentationTone;
  detail: string;
} {
  if (identity.status === "guest") {
    return { label: "登录后可用", tone: "default", detail: "本机工作区可继续使用。" };
  }
  const needsAttention = sync.conflict_count > 0 || sync.error_count > 0;
  const detail = needsAttention
    ? `${sync.pending_count} 项待同步 · ${sync.conflict_count} 项冲突 · ${sync.error_count} 项失败`
    : sync.pending_count > 0
      ? `${sync.pending_count} 项本地变更等待上传`
      : "当前没有待同步项目";
  if (identity.status === "closing") {
    return { label: "账号注销中", tone: "warning", detail };
  }
  if (choice === "local_only") {
    return { label: "仅本地", tone: "default", detail };
  }
  if (choice === "undecided") {
    return { label: "尚未启用同步", tone: "default", detail };
  }
  if (identity.status === "offline_authenticated") {
    return { label: "离线，等待联网", tone: "warning", detail };
  }
  if (needsAttention) {
    return { label: "需要处理", tone: "warning", detail };
  }
  if (sync.pending_count > 0) {
    return { label: "等待同步", tone: "info", detail };
  }
  return {
    label: sync.last_success_at ? "已同步" : "尚未同步",
    tone: sync.last_success_at ? "success" : "default",
    detail: sync.last_success_at ? `上次成功：${new Date(sync.last_success_at).toLocaleString()}` : detail,
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
  const title = isExport ? "数据导出进度" : "账号注销进度";
  const canDownload = isExport && job.status === "completed";
  const waitingForBackups = job.kind === "deletion" && job.status === "waiting_backup_expiry";
  const isTerminal = job.status === "completed" || job.status === "failed";

  switch (job.status) {
    case "queued":
      return {
        title,
        label: "排队中",
        tone: "default",
        description: "",
        isTerminal,
        canDownload,
        waitingForBackups,
      };
    case "running":
      return {
        title,
        label: "处理中",
        tone: "accent",
        description: "",
        isTerminal,
        canDownload,
        waitingForBackups,
      };
    case "waiting_for_producers":
      return {
        title,
        label: "等待进行中的操作完成",
        tone: "warning",
        description: "云端正在完成已开始的导出或同步。",
        isTerminal,
        canDownload,
        waitingForBackups,
      };
    case "waiting_backup_expiry":
      return {
        title,
        label: "等待备份到期擦除",
        tone: "warning",
        description: "",
        isTerminal: false,
        canDownload,
        waitingForBackups: true,
      };
    case "retry_wait":
      return {
        title,
        label: "等待重试",
        tone: "danger",
        description: "遇到临时错误，稍后会自动重试。",
        isTerminal,
        canDownload,
        waitingForBackups,
      };
    case "completed":
      return {
        title,
        label: "已完成",
        tone: "success",
        description: "",
        isTerminal: true,
        canDownload,
        waitingForBackups: false,
      };
    case "failed":
      return {
        title,
        label: "执行失败",
        tone: "danger",
        description: "云端未能完成此操作。",
        isTerminal: true,
        canDownload,
        waitingForBackups: false,
      };
    default:
      return {
        title,
        label: job.status,
        tone: "default",
        description: "",
        isTerminal,
        canDownload,
        waitingForBackups,
      };
  }
}
