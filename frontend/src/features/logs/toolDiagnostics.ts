import type { ToolDiagnostic } from "../../api/system";

export function filterToolDiagnostics(entries: ToolDiagnostic[], search: string): ToolDiagnostic[] {
  const query = search.trim().toLowerCase();
  return entries.filter(entry => `${entry.name} ${entry.toolCallId} ${entry.conversationTitle} ${entry.conversationId} ${JSON.stringify(entry.args)} ${entry.result ?? ""} ${entry.verificationReason ?? ""} ${entry.reason ?? ""}`.toLowerCase().includes(query));
}

export function conversationLogPath(entry: ToolDiagnostic): string {
  return `/chat/${encodeURIComponent(entry.conversationId)}`;
}

export function toolDiagnosticStatus(entry: ToolDiagnostic): { label: string; tone: "danger" | "success" | "warning" | "default" } {
  if (entry.executionStatus === "failed") return { label: "执行失败", tone: "danger" };
  if (entry.verificationStatus === "failed") return { label: "验证失败", tone: "danger" };
  const statuses = {
    succeeded: { label: "执行成功", tone: "success" },
    running: { label: "执行中", tone: "warning" },
    queued: { label: "排队中", tone: "default" },
    awaiting_approval: { label: "等待审批", tone: "warning" },
    rejected: { label: "已拒绝", tone: "default" },
    cancelled: { label: "已取消", tone: "default" },
    interrupted: { label: "已中断", tone: "warning" },
  } as const;
  return statuses[entry.executionStatus];
}
