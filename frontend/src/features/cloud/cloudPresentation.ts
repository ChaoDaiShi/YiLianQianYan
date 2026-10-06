import type { CloudIdentity, CloudSyncStatus, SearchResult } from "../../api/cloud";

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
