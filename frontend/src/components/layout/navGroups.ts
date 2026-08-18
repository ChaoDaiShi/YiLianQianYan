import {
  Activity,
  BookOpen,
  Brain,
  Bot,
  Boxes,
  FolderKanban,
  GitBranch,
  MessageSquare,
  ListTodo,
  Puzzle,
  ScrollText,
  Settings,
  Sparkles,
  type LucideIcon,
} from "lucide-react";

export interface NavGroupItem {
  readonly to: string;
  readonly id: string;
  readonly label: string;
  readonly icon: LucideIcon;
}

export interface NavGroup {
  readonly label: string;
  readonly items: readonly NavGroupItem[];
}

export const NAV_GROUPS: readonly NavGroup[] = [
  {
    label: "核心",
    items: [
      { to: "/tasks", id: "tasks", label: "任务", icon: ListTodo },
      { to: "/chat", id: "chat", label: "对话", icon: MessageSquare },
      { to: "/workflows", id: "workflows", label: "工作流", icon: GitBranch },
      {
        to: "/workspaces",
        id: "workspaces",
        label: "工作空间",
        icon: FolderKanban,
      },
    ],
  },
  {
    label: "能力",
    items: [
      { to: "/skills", id: "skills", label: "技能", icon: Sparkles },
      { to: "/plugins", id: "plugins", label: "插件", icon: Puzzle },
      { to: "/agents", id: "agents", label: "智能体", icon: Bot },
      { to: "/capabilities", id: "capabilities", label: "能力", icon: Boxes },
      { to: "/memory", id: "memory", label: "记忆", icon: Brain },
      { to: "/knowledge", id: "knowledge", label: "知识库", icon: BookOpen },
    ],
  },
  {
    label: "系统",
    items: [
      { to: "/system", id: "system", label: "监控", icon: Activity },
      { to: "/logs", id: "logs", label: "日志", icon: ScrollText },
      { to: "/settings", id: "settings", label: "设置", icon: Settings },
    ],
  },
] as const;
