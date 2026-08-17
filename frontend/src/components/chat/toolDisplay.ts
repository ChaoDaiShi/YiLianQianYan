import type { ToolCallRecord } from "../../types";

const TOOL_LABELS: Record<string, string> = {
  bash: "执行命令",
  powershell: "PowerShell",
  filesystem: "文件操作",
  read_file: "读取文件",
  write_file: "写入文件",
  edit_file: "编辑文件",
  grep: "搜索内容",
  glob: "查找文件",
  http_request: "网络请求",
  load_skill: "加载技能",
  write_todos: "更新计划",
  process: "进程管理",
  screenshot: "截取屏幕",
  mouse: "鼠标操作",
  keyboard: "键盘操作",
  upscale_image: "放大图片",
};

function titleCase(value: string): string {
  return value
    .split(/[_\s-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
    .join(" ");
}

export function formatToolDisplayName(name: string): string {
  const normalized = name.trim();
  if (!normalized) return "未知工具";
  if (TOOL_LABELS[normalized]) return TOOL_LABELS[normalized];
  if (normalized.toLowerCase() === "http") return "网络请求";
  if (normalized.toLowerCase().startsWith("mcp_")) {
    const parts = normalized.split("_").filter(Boolean);
    return "MCP · " + titleCase(parts[parts.length - 1] || "Tool");
  }
  return titleCase(normalized);
}

export function formatToolActivity(
  name: string,
  _args: Record<string, unknown>,
): string {
  return `正在调用 ${formatToolDisplayName(name)}`;
}

export function formatToolResultSummary(result?: string): string {
  return result?.trim() ? "工具已返回结果" : "工具未返回内容";
}

export function formatToolStatus(
  status: ToolCallRecord["status"],
): string {
  if (status === "running") return "正在执行";
  if (status === "success") return "已完成";
  if (status === "blocked") return "等待你的确认";
  return "执行失败";
}

export function formatElapsed(startedAt: number, finishedAt = Date.now()): string {
  const milliseconds = Math.max(0, finishedAt - startedAt);
  const totalSeconds = Math.floor(milliseconds / 1000);
  if (totalSeconds < 60) {
    const tenths = Math.floor(milliseconds / 100) / 10;
    return `已运行 ${tenths % 1 === 0 ? tenths.toFixed(0) : tenths.toFixed(1)} 秒`;
  }
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `已运行 ${minutes} 分 ${seconds} 秒`;
}
