import {
  Activity,
  BookOpen,
  Bot,
  Boxes,
  Brain,
  FolderKanban,
  GitBranch,
  ListTodo,
  MessageSquare,
  Puzzle,
  ScrollText,
  Search,
  Settings,
  Sparkles,
  UserCircle,
  type LucideIcon,
} from "lucide-react";
import type { NavGroup } from "./navGroups";

export type NavGroupType = "core" | "capability" | "system";

export interface NavEntry {
  readonly id: string;
  readonly to: string;
  readonly label: string;
  readonly description: string;
  readonly icon: LucideIcon;
  readonly group: NavGroupType;
  readonly primary: boolean;
  readonly shortcut?: string;
  readonly moduleId?: string;
  readonly keywords?: readonly string[];
}

export const NAV_ENTRIES: readonly NavEntry[] = [
  // 核心主导航 (Primary)
  {
    id: "tasks",
    to: "/tasks",
    label: "任务",
    description: "查看与管理任务执行状态及主从链路",
    icon: ListTodo,
    group: "core",
    primary: true,
    shortcut: "Alt+1",
    keywords: ["任务", "task", "renwu", "rw"],
  },
  {
    id: "chat",
    to: "/chat",
    label: "对话",
    description: "与小昔涟开启智能体协作对话工作台",
    icon: MessageSquare,
    group: "core",
    primary: true,
    shortcut: "Alt+2",
    keywords: ["对话", "聊天", "chat", "duihua"],
  },
  {
    id: "search",
    to: "/search",
    label: "搜索",
    description: "AI 深度搜索与全域知识检索",
    icon: Search,
    group: "core",
    primary: true,
    shortcut: "Alt+3",
    keywords: ["搜索", "search", "sousuo"],
  },
  {
    id: "workflows",
    to: "/workflows",
    label: "工作流",
    description: "可视化搭建与运行任务 DAG 流程",
    icon: GitBranch,
    group: "core",
    primary: true,
    shortcut: "Alt+4",
    keywords: ["工作流", "workflow", "gongzuoliu"],
  },
  {
    id: "workspaces",
    to: "/workspaces",
    label: "工作空间",
    description: "管理本地工作区目录与工程文件集",
    icon: FolderKanban,
    group: "core",
    primary: true,
    shortcut: "Alt+5",
    keywords: ["工作空间", "workspace", "gongzuokongjian"],
  },

  // 能力与扩展 (Capability Group)
  {
    id: "skills",
    to: "/skills",
    label: "技能",
    description: "管理本地技能与提示词流程规范",
    icon: Sparkles,
    group: "capability",
    primary: false,
    keywords: ["技能", "skill", "jineng"],
  },
  {
    id: "plugins",
    to: "/plugins",
    label: "插件",
    description: "MCP 服务与扩展插件连接及管理",
    icon: Puzzle,
    group: "capability",
    primary: false,
    keywords: ["插件", "plugin", "mcp", "chajian"],
  },
  {
    id: "agents",
    to: "/agents",
    label: "智能体",
    description: "智能体角色预设与执行团队配置",
    icon: Bot,
    group: "capability",
    primary: false,
    keywords: ["智能体", "agent", "zhinengti"],
  },
  {
    id: "capabilities",
    to: "/capabilities",
    label: "能力",
    description: "总览智能体、技能与工具执行健康度",
    icon: Boxes,
    group: "capability",
    primary: true,
    shortcut: "Alt+6",
    keywords: ["能力", "工具", "capabilities", "nengli"],
  },
  {
    id: "memory",
    to: "/memory",
    label: "记忆",
    description: "管理长期上下文与知识事实事实库",
    icon: Brain,
    group: "capability",
    primary: false,
    keywords: ["记忆", "memory", "jiyi"],
  },
  {
    id: "knowledge",
    to: "/knowledge",
    label: "知识库",
    description: "知识库与本地文档接入能力地图",
    icon: BookOpen,
    group: "capability",
    primary: false,
    keywords: ["知识库", "knowledge", "zhishiku"],
  },

  // 系统与控制 (System Group)
  {
    id: "system",
    to: "/system",
    label: "监控",
    description: "系统资源、进程与服务健康监控仪表盘",
    icon: Activity,
    group: "system",
    primary: false,
    keywords: ["监控", "系统", "system", "jiankong"],
  },
  {
    id: "logs",
    to: "/logs",
    label: "日志",
    description: "实时控制台与系统运行日志检视",
    icon: ScrollText,
    group: "system",
    primary: false,
    keywords: ["日志", "log", "rizhi"],
  },
  {
    id: "settings",
    to: "/settings",
    label: "设置",
    description: "模型服务、安全网关与外观偏好设置",
    icon: Settings,
    group: "system",
    primary: false,
    shortcut: "Ctrl+,",
    keywords: ["设置", "settings", "shezhi"],
  },
  {
    id: "account",
    to: "/account",
    label: "账号",
    description: "本地身份凭据与云端协作账号中心",
    icon: UserCircle,
    group: "system",
    primary: false,
    keywords: ["账号", "account", "zhanghao"],
  },
] as const;

/**
 * Derives legacy NAV_GROUPS format from the single registry.
 * Preserves exact group ordering ("核心", "能力", "系统") and route order for contract tests.
 */
export const DERIVED_NAV_GROUPS: readonly NavGroup[] = [
  {
    label: "核心",
    items: NAV_ENTRIES.filter((e) => e.group === "core").map((e) => ({
      to: e.to,
      id: e.id,
      label: e.label,
      icon: e.icon,
    })),
  },
  {
    label: "能力",
    items: NAV_ENTRIES.filter((e) => e.group === "capability").map((e) => ({
      to: e.to,
      id: e.id,
      label: e.label,
      icon: e.icon,
    })),
  },
  {
    label: "系统",
    items: NAV_ENTRIES.filter((e) => e.group === "system").map((e) => ({
      to: e.to,
      id: e.id,
      label: e.label,
      icon: e.icon,
    })),
  },
] as const;

export function getNavEntryById(id: string): NavEntry | undefined {
  return NAV_ENTRIES.find((entry) => entry.id === id);
}

export function getNavEntryByPath(pathname: string): NavEntry | undefined {
  return NAV_ENTRIES.find(
    (entry) => pathname === entry.to || pathname.startsWith(entry.to + "/"),
  );
}
