export type SystemTone = "default" | "success" | "warning" | "danger" | "info";

export interface PresentationStatus {
  label: string;
  tone: SystemTone;
}

const STATUS_META: Record<string, PresentationStatus> = {
  healthy: { label: "正常", tone: "success" },
  connected: { label: "已连接", tone: "info" },
  running: { label: "运行中", tone: "info" },
  connecting: { label: "连接中", tone: "warning" },
  degraded: { label: "降级", tone: "warning" },
  unavailable: { label: "不可用", tone: "danger" },
  error: { label: "错误", tone: "danger" },
  unknown: { label: "未知", tone: "default" },
};

const LEVEL_META: Record<string, PresentationStatus> = {
  trace: { label: "TRACE", tone: "default" },
  debug: { label: "DEBUG", tone: "default" },
  info: { label: "INFO", tone: "info" },
  warn: { label: "WARN", tone: "warning" },
  warning: { label: "WARN", tone: "warning" },
  error: { label: "ERROR", tone: "danger" },
  tool: { label: "TOOL", tone: "success" },
  chat: { label: "CHAT", tone: "info" },
};

export function formatSystemStatus(status: string | null | undefined): PresentationStatus {
  const normalized = status?.trim().toLowerCase();
  if (!normalized) return STATUS_META.unknown;
  return STATUS_META[normalized] ?? { label: status ?? "未知", tone: "default" };
}

export function formatLogLevel(level: string): PresentationStatus {
  const normalized = level.trim().toLowerCase();
  return LEVEL_META[normalized] ?? { label: level.toUpperCase(), tone: "default" };
}

export function formatLogTimestamp(timestamp: number): string {
  return new Date(timestamp).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}
