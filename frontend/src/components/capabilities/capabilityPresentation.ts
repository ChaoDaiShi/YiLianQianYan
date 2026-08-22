export type CapabilityTone = "default" | "success" | "warning" | "danger" | "info" | "accent";

const KIND_LABELS: Record<string, string> = {
  tool: "工具",
  mcp_tool: "MCP 工具",
  subagent: "子智能体",
  agent: "Agent",
  workflow: "工作流",
  skill: "技能",
};

const PROVIDER_LABELS: Record<string, string> = {
  builtin: "内置",
  mcp: "MCP",
  subagent: "子智能体运行时",
  agent_runtime: "Agent 运行时",
  workflow_runtime: "工作流运行时",
  skill_runtime: "技能运行时",
};

const STATUS_PRESENTATIONS: Record<string, { label: string; tone: CapabilityTone }> = {
  ready: { label: "就绪", tone: "success" },
  unavailable: { label: "不可用", tone: "danger" },
  disabled: { label: "已禁用", tone: "default" },
  misconfigured: { label: "配置错误", tone: "warning" },
  degraded: { label: "降级", tone: "warning" },
  unknown: { label: "未知", tone: "default" },
};

const RISK_PRESENTATIONS: Record<string, { label: string; tone: CapabilityTone }> = {
  low: { label: "低风险", tone: "success" },
  medium: { label: "中风险", tone: "warning" },
  high: { label: "高风险", tone: "danger" },
  dynamic: { label: "动态风险", tone: "info" },
};

const PERMISSION_LABELS: Record<string, string> = {
  "filesystem.read": "读取文件",
  "filesystem.write": "写入文件",
  "shell.execute": "执行 Shell",
  "process.inspect": "查看进程",
  "process.control": "控制进程",
  "network.request": "发起网络请求",
  "desktop.observe": "观察桌面",
  "desktop.interact": "操作桌面",
  "skill.load": "加载技能",
  "mcp.invoke": "调用 MCP 工具",
  "agent.delegate": "委派 Agent",
};

const AGENT_SOURCE_LABELS: Record<string, string> = {
  builtin: "内置",
  local_file: "本地文件",
  database: "数据库",
};

const TRANSPORT_LABELS: Record<string, string> = {
  stdio: "Stdio",
  streamable_http: "Streamable HTTP",
  http: "Streamable HTTP",
  sse: "SSE（旧版）",
};

export function formatCapabilityKind(value: string): string {
  return KIND_LABELS[value] ?? value;
}

export function formatCapabilityProvider(value: string): string {
  return PROVIDER_LABELS[value] ?? value;
}

export function formatCapabilityStatus(value: string): { label: string; tone: CapabilityTone } {
  return STATUS_PRESENTATIONS[value] ?? { label: value, tone: "default" };
}

export function formatCapabilityRisk(value: string): { label: string; tone: CapabilityTone } {
  return RISK_PRESENTATIONS[value] ?? { label: value, tone: "default" };
}

export function formatPermission(value: string): string {
  return PERMISSION_LABELS[value] ?? value;
}

export function formatAgentSource(value: string): string {
  return AGENT_SOURCE_LABELS[value] ?? value;
}

export function formatPluginTransport(value: string): string {
  return TRANSPORT_LABELS[value] ?? value;
}
