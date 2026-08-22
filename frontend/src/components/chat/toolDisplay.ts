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
  open_url: "打开网站",
  open_application: "打开应用",
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

const SUMMARY_LIMIT = 84;

function stringArg(
  args: Record<string, unknown>,
  ...keys: string[]
): string | null {
  for (const key of keys) {
    const value = args[key];
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return null;
}

function oneLine(value: string, limit = SUMMARY_LIMIT): string {
  const firstLine =
    value.split(/\r?\n/).find((line) => line.trim())?.trim() || "";
  return firstLine.length > limit
    ? `${firstLine.slice(0, limit - 1)}…`
    : firstLine;
}

function baseName(value: string): string {
  const parts = value.replace(/\\/g, "/").split("/").filter(Boolean);
  return parts[parts.length - 1] || value;
}

function displayUrl(value: string): string {
  try {
    const url = new URL(value);
    return `${url.host}${url.pathname === "/" ? "" : url.pathname}`;
  } catch {
    return oneLine(value);
  }
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

export function formatToolActionSummary(
  name: string,
  args: Record<string, unknown>,
): string {
  const normalizedName = name.trim().toLowerCase();
  const command = stringArg(args, "command", "script");
  const path = stringArg(args, "path", "file", "target", "file_path");
  const url = stringArg(args, "url", "uri", "href");
  const query = stringArg(args, "query", "pattern", "text");
  const application = stringArg(args, "application", "app");

  if (
    (normalizedName === "bash" || normalizedName === "powershell") &&
    command
  ) {
    return oneLine(`执行命令：${oneLine(command, 78)}`);
  }
  if (normalizedName === "read_file" && path) {
    return oneLine(`读取文件：${baseName(path)}`);
  }
  if (normalizedName === "write_file" && path) {
    return oneLine(`写入文件：${baseName(path)}`);
  }
  if (normalizedName === "edit_file" && path) {
    return oneLine(`编辑文件：${baseName(path)}`);
  }
  if (normalizedName === "grep" && query) {
    return oneLine(`搜索内容：${query}`);
  }
  if (normalizedName === "glob" && query) {
    return oneLine(`查找文件：${query}`);
  }
  if (
    (normalizedName === "http_request" || normalizedName === "http") &&
    url
  ) {
    return oneLine(`请求网络资源：${displayUrl(url)}`);
  }
  if (normalizedName === "open_url" && url) {
    return oneLine(`打开网站：${displayUrl(url)}`);
  }
  if (normalizedName === "open_application" && application) {
    return oneLine(`打开应用：${application}`);
  }
  if (normalizedName === "process") return "管理系统进程";
  if (normalizedName === "screenshot") return "截取当前屏幕";
  if (normalizedName === "mouse") return "执行鼠标操作";
  if (normalizedName === "keyboard") return "执行键盘操作";
  return `调用 ${formatToolDisplayName(name)}`;
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
