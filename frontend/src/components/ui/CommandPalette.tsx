import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useNavigate } from "react-router-dom";
import {
  Activity,
  BookOpen,
  Bot,
  Boxes,
  Brain,
  Compass,
  FolderGit2,
  LayoutGrid,
  Moon,
  Puzzle,
  ScrollText,
  Search,
  Settings,
  Sparkles,
  Sun,
  User,
  Workflow,
  X,
} from "lucide-react";
import { useTheme } from "../../theme";

interface CommandItem {
  id: string;
  title: string;
  category: "导航页面" | "系统与偏好" | "高频动作";
  description?: string;
  icon: React.ReactNode;
  shortcut?: string;
  action: () => void;
}

export interface CommandPaletteProps {
  open?: boolean;
  onClose?: () => void;
}

export default function CommandPalette({
  open: controlledOpen,
  onClose: controlledOnClose,
}: CommandPaletteProps = {}) {
  const [internalOpen, setInternalOpen] = useState(false);
  const isOpen = controlledOpen !== undefined ? controlledOpen : internalOpen;
  const setIsOpen = (value: boolean) => {
    if (controlledOnClose && !value) {
      controlledOnClose();
    }
    setInternalOpen(value);
  };

  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [mounted, setMounted] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();
  const { setMode, resolvedScheme } = useTheme();

  useEffect(() => {
    setMounted(true);
  }, []);

  // Global Ctrl+K / Cmd+K listener
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.isComposing) return;
      const isCmdK = (event.ctrlKey || event.metaKey) && (event.key === "k" || event.key === "K");
      if (isCmdK) {
        event.preventDefault();
        setIsOpen(!isOpen);
      } else if (event.key === "Escape" && isOpen) {
        event.preventDefault();
        setIsOpen(false);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen]);

  useEffect(() => {
    if (isOpen) {
      setQuery("");
      setSelectedIndex(0);
      window.setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  const runAndClose = (fn: () => void) => {
    fn();
    setIsOpen(false);
  };

  const commands: CommandItem[] = useMemo(() => [
    // 导航
    {
      id: "nav-chat",
      title: "对话工作台",
      category: "导航页面",
      description: "与小昔涟开启智能体协作对话",
      icon: <Sparkles className="h-4 w-4 text-[#ea91b9]" />,
      shortcut: "Alt+2",
      action: () => navigate("/chat"),
    },
    {
      id: "nav-tasks",
      title: "任务中心",
      category: "导航页面",
      description: "查看与监控正在运行的任务列表",
      icon: <LayoutGrid className="h-4 w-4 text-[#99cfea]" />,
      shortcut: "Alt+1",
      action: () => navigate("/tasks"),
    },
    {
      id: "nav-workflows",
      title: "工作流编排",
      category: "导航页面",
      description: "可视化搭建与运行任务 DAG 流程",
      icon: <Workflow className="h-4 w-4 text-[#ad9be8]" />,
      shortcut: "Alt+4",
      action: () => navigate("/workflows"),
    },
    {
      id: "nav-workspaces",
      title: "工作区管理",
      category: "导航页面",
      description: "管理本地工作区目录与工程文件集",
      icon: <FolderGit2 className="h-4 w-4 text-[#ebcf8c]" />,
      shortcut: "Alt+5",
      action: () => navigate("/workspaces"),
    },
    {
      id: "nav-capabilities",
      title: "能力与工具中心",
      category: "导航页面",
      description: "总览智能体、技能与工具执行健康度",
      icon: <Boxes className="h-4 w-4 text-[#52b798]" />,
      shortcut: "Alt+6",
      action: () => navigate("/capabilities"),
    },
    {
      id: "nav-agents",
      title: "智能体配置 (Agents)",
      category: "导航页面",
      description: "管理角色人设、Prompt 与工具绑定",
      icon: <Bot className="h-4 w-4 text-[#ad9be8]" />,
      action: () => navigate("/agents"),
    },
    {
      id: "nav-skills",
      title: "技能库 (Skills)",
      category: "导航页面",
      description: "查看与试运行已安装的技能模块",
      icon: <Puzzle className="h-4 w-4 text-[#99cfea]" />,
      action: () => navigate("/skills"),
    },
    {
      id: "nav-plugins",
      title: "扩展插件 (Plugins)",
      category: "导航页面",
      description: "配置 MCP 服务、浏览器插件与适配器",
      icon: <Compass className="h-4 w-4 text-[#ea91b9]" />,
      action: () => navigate("/plugins"),
    },
    {
      id: "nav-memory",
      title: "长期记忆库 (Memory)",
      category: "导航页面",
      description: "检索与编辑用户偏好与对话提炼事实",
      icon: <Brain className="h-4 w-4 text-[#ad9be8]" />,
      action: () => navigate("/memory"),
    },
    {
      id: "nav-knowledge",
      title: "本地知识库 (Knowledge)",
      category: "导航页面",
      description: "上传文档与管理向量化检索知识库",
      icon: <BookOpen className="h-4 w-4 text-[#ebcf8c]" />,
      action: () => navigate("/knowledge"),
    },
    {
      id: "nav-search",
      title: "综合检索 (Search)",
      category: "导航页面",
      description: "跨会话、工作区与记忆的全文检索",
      icon: <Search className="h-4 w-4 text-[#99cfea]" />,
      shortcut: "Alt+3",
      action: () => navigate("/search"),
    },
    {
      id: "nav-system",
      title: "系统状态监控",
      category: "导航页面",
      description: "监控后端进程健康、内存占用与网络指标",
      icon: <Activity className="h-4 w-4 text-[#52b798]" />,
      action: () => navigate("/system"),
    },
    {
      id: "nav-logs",
      title: "运行日志诊断",
      category: "导航页面",
      description: "实时诊断日志与堆栈输出追踪",
      icon: <ScrollText className="h-4 w-4 text-[#d9a441]" />,
      action: () => navigate("/logs"),
    },
    {
      id: "nav-account",
      title: "个人账户与云端",
      category: "导航页面",
      description: "云端契约同步、额度查看与凭据配置",
      icon: <User className="h-4 w-4 text-[#ea91b9]" />,
      action: () => navigate("/account"),
    },
    {
      id: "nav-settings",
      title: "系统设置",
      category: "导航页面",
      description: "外观主题、模型 API 密钥与安全策略",
      icon: <Settings className="h-4 w-4 text-[#696276]" />,
      shortcut: "Ctrl+,",
      action: () => navigate("/settings"),
    },

    // 高频动作
    {
      id: "action-theme-toggle",
      title: resolvedScheme === "dark" ? "切换为明亮「昔涟水波」" : "切换为暗夜「昔涟暗夜」",
      category: "系统与偏好",
      description: "即时切换全站色彩主题",
      icon: resolvedScheme === "dark" ? <Sun className="h-4 w-4 text-[#ebcf8c]" /> : <Moon className="h-4 w-4 text-[#ad9be8]" />,
      action: () => {
        setMode(resolvedScheme === "dark" ? "light" : "dark");
      },
    },
    {
      id: "action-new-chat",
      title: "新建会话",
      category: "高频动作",
      description: "开启一段崭新的智能体对话",
      icon: <Sparkles className="h-4 w-4 text-[#ea91b9]" />,
      action: () => navigate("/chat"),
    },
    {
      id: "action-new-workflow",
      title: "创建新工作流",
      category: "高频动作",
      description: "进入工作流画布开始编排节点",
      icon: <Workflow className="h-4 w-4 text-[#ad9be8]" />,
      action: () => navigate("/workflows"),
    },
  ], [navigate, resolvedScheme, setMode]);

  const filteredCommands = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter(
      (cmd) =>
        cmd.title.toLowerCase().includes(q) ||
        (cmd.description && cmd.description.toLowerCase().includes(q)) ||
        cmd.category.toLowerCase().includes(q)
    );
  }, [commands, query]);

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelectedIndex((prev) => (prev + 1) % Math.max(1, filteredCommands.length));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelectedIndex((prev) => (prev - 1 + filteredCommands.length) % Math.max(1, filteredCommands.length));
    } else if (event.key === "Enter") {
      event.preventDefault();
      const current = filteredCommands[selectedIndex];
      if (current) {
        runAndClose(current.action);
      }
    }
  };

  if (!isOpen) return null;

  const content = (
    <div
      className="fixed inset-0 z-[var(--z-command-palette,80)] flex items-start justify-center pt-[15vh] px-4"
      role="presentation"
      onClick={() => setIsOpen(false)}
    >
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-[#1e1b2e]/55 backdrop-blur-md animate-fade-in"
        aria-hidden="true"
      />

      {/* Spotlight Dialog */}
      <div
        role="dialog"
        aria-modal="true"
        aria-label="全局命令中心"
        className="relative w-full max-w-xl overflow-hidden rounded-2xl border border-white/20 bg-[var(--surface-solid)] text-[var(--text-primary)] shadow-2xl backdrop-blur-xl animate-scale-in"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        {/* Top Glow bar */}
        <div className="h-1 w-full bg-gradient-to-r from-[#ea91b9] via-[#ad9be8] to-[#99cfea]" />

        {/* Search Input Bar */}
        <div className="flex items-center gap-3 border-b border-[var(--border-soft)] px-4 py-3.5">
          <Search className="h-5 w-5 shrink-0 text-[#ea91b9]" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelectedIndex(0);
            }}
            placeholder="搜索页面、执行快捷动作或切换主题……"
            className="w-full bg-transparent text-sm text-[var(--text-primary)] placeholder:text-[var(--text-faint)] focus:outline-none"
          />
          {query && (
            <button
              type="button"
              onClick={() => {
                setQuery("");
                inputRef.current?.focus();
              }}
              className="rounded p-1 text-[var(--text-faint)] hover:text-[var(--text-primary)]"
              aria-label="清空输入"
            >
              <X className="h-4 w-4" />
            </button>
          )}
          <kbd className="rounded bg-[var(--surface-muted)] px-2 py-0.5 text-[11px] font-mono text-[var(--text-faint)] border border-[var(--border-soft)]">
            ESC
          </kbd>
        </div>

        {/* Command List */}
        <div ref={listRef} className="max-h-[360px] overflow-y-auto p-2 scrollbar-thin">
          {filteredCommands.length === 0 ? (
            <div className="py-10 text-center text-sm text-[var(--text-faint)]">
              未找到匹配的命令或页面
            </div>
          ) : (
            filteredCommands.map((cmd, index) => {
              const selected = index === selectedIndex;
              return (
                <button
                  type="button"
                  key={cmd.id}
                  onClick={() => runAndClose(cmd.action)}
                  onMouseEnter={() => setSelectedIndex(index)}
                  className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left transition-colors duration-[100ms] ${
                    selected
                      ? "bg-[var(--accent-soft)] text-[var(--accent-contrast)] shadow-sm"
                      : "text-[var(--text-primary)] hover:bg-[var(--surface-hover)]"
                  }`}
                >
                  <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-[var(--surface-muted)] border border-[var(--border-soft)]">
                    {cmd.icon}
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium">{cmd.title}</span>
                      <span className="rounded bg-[var(--surface-muted)] px-1.5 py-0.5 text-[10px] text-[var(--text-faint)]">
                        {cmd.category}
                      </span>
                    </div>
                    {cmd.description && (
                      <p className="truncate text-xs text-[var(--text-secondary)] mt-0.5">
                        {cmd.description}
                      </p>
                    )}
                  </div>
                  {cmd.shortcut && (
                    <kbd className="shrink-0 rounded bg-[var(--surface-solid)] px-2 py-0.5 text-[11px] font-mono text-[var(--text-muted)] border border-[var(--border-soft)]">
                      {cmd.shortcut}
                    </kbd>
                  )}
                </button>
              );
            })
          )}
        </div>

        {/* Footer Hint Bar */}
        <div className="flex items-center justify-between border-t border-[var(--border-soft)] bg-[var(--surface-muted)] px-4 py-2 text-[11px] text-[var(--text-faint)]">
          <div className="flex items-center gap-3">
            <span>↑↓ 选择</span>
            <span>↵ 确认</span>
            <span>ESC 关闭</span>
          </div>
          <span>忆涟千言 · 命令中心</span>
        </div>
      </div>
    </div>
  );

  if (mounted && typeof document !== "undefined") {
    return createPortal(content, document.body);
  }
  return content;
}
