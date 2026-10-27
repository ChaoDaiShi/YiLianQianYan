import React, { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useNavigate } from "react-router-dom";
import {
  Search,
  Sparkles,
  Sun,
  Moon,
  Laptop,
  X,
  ShieldAlert,
  ShieldCheck,
  ListTodo,
} from "lucide-react";
import { useTheme } from "../../theme";
import { NAV_ENTRIES } from "../layout/navRegistry";
import WorkspaceIcon from "./WorkspaceIcon";

interface CommandItem {
  id: string;
  title: string;
  category: "最近使用" | "导航页面" | "系统与偏好" | "高频动作";
  description?: string;
  icon: React.ReactNode;
  shortcut?: string;
  keywords?: string[];
  action: () => void;
}

const RECENT_COMMANDS_KEY = "cyrene_recent_commands";

export function getRecentCommandIds(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_COMMANDS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      return parsed.filter((item): item is string => typeof item === "string");
    }
    return [];
  } catch {
    return [];
  }
}

export function saveRecentCommandId(id: string) {
  if (typeof id !== "string" || !id) return;
  try {
    const list = getRecentCommandIds().filter((item) => item !== id);
    list.unshift(id);
    localStorage.setItem(RECENT_COMMANDS_KEY, JSON.stringify(list.slice(0, 5)));
  } catch {
    // Ignore storage errors
  }
}

export default function CommandPalette() {
  const [isOpen, setIsOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [mounted, setMounted] = useState(false);
  const triggerRef = useRef<HTMLElement | null>(null);

  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();
  const { setMode } = useTheme();

  useEffect(() => {
    setMounted(true);
    const handleGlobalKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && (event.key === "k" || event.key === "K")) {
        event.preventDefault();
        triggerRef.current = document.activeElement as HTMLElement | null;
        setIsOpen((open) => !open);
      }
    };
    window.addEventListener("keydown", handleGlobalKeyDown);
    return () => window.removeEventListener("keydown", handleGlobalKeyDown);
  }, []);

  useEffect(() => {
    if (isOpen) {
      setQuery("");
      setSelectedIndex(0);
      window.setTimeout(() => inputRef.current?.focus(), 50);
    } else if (triggerRef.current) {
      triggerRef.current.focus();
    }
  }, [isOpen]);

  const runAndClose = (id: string, fn: () => void) => {
    saveRecentCommandId(id);
    fn();
    setIsOpen(false);
  };

  const commands: CommandItem[] = useMemo(() => {
    // Derive navigation commands directly from single registry
    const navCmds: CommandItem[] = NAV_ENTRIES.map((entry) => {
      return {
        id: `nav-${entry.id}`,
        title: entry.label,
        category: "导航页面",
        description: entry.description,
        icon: <WorkspaceIcon name={entry.workspaceIcon} size={24} variant="mono" className="text-[var(--text-secondary)]" />,
        shortcut: entry.shortcut,
        keywords: entry.keywords ? [...entry.keywords] : undefined,
        action: () => navigate(entry.to),
      };
    });

    const actionCmds: CommandItem[] = [
      {
        id: "action-new-chat",
        title: "开启新对话",
        category: "高频动作",
        description: "进入智能体对话工作台开启崭新会话",
        icon: <Sparkles className="h-4 w-4 text-[var(--accent-primary)]" />,
        keywords: ["新对话", "新建", "chat", "new"],
        action: () => navigate("/chat"),
      },
      {
        id: "action-connectors",
        title: "打开连接器",
        category: "高频动作",
        description: "查看对话、智能体和能力资源的连接",
        icon: <ListTodo className="h-4 w-4 text-[var(--accent-purple)]" />,
        keywords: ["新任务", "新建任务", "task"],
        action: () => navigate("/connectors"),
      },
      {
        id: "action-approvals",
        title: "查看待审批操作",
        category: "高频动作",
        description: "前往安全网关与系统监控审批待确认操作",
        icon: <ShieldAlert className="h-4 w-4 text-[var(--warning)]" />,
        keywords: ["审批", "approval", "待确认", "安全"],
        action: () => navigate("/system?card=approvals"),
      },
      {
        id: "action-security-settings",
        title: "打开安全与权限设置",
        category: "高频动作",
        description: "配置工具风险策略与沙箱安全边界",
        icon: <ShieldCheck className="h-4 w-4 text-[var(--success)]" />,
        keywords: ["安全", "权限", "gateway", "permission"],
        action: () => navigate("/settings?section=permission"),
      },
      // 系统偏好
      {
        id: "pref-theme-light",
        title: "切换到昔涟 · 浅色模式",
        category: "系统与偏好",
        description: "白昼通透晨光材质外观",
        icon: <Sun className="h-4 w-4 text-[var(--accent-gold)]" />,
        keywords: ["浅色", "白天", "light"],
        action: () => setMode("light"),
      },
      {
        id: "pref-theme-dark",
        title: "切换到昔涟 · 暗色模式",
        category: "系统与偏好",
        description: "夜幕静谧深紫琉璃材质外观",
        icon: <Moon className="h-4 w-4 text-[var(--accent-purple)]" />,
        keywords: ["深色", "暗色", "夜间", "dark"],
        action: () => setMode("dark"),
      },
      {
        id: "pref-theme-system",
        title: "跟随系统外观设置",
        category: "系统与偏好",
        description: "自动适配 Windows 系统主题切换",
        icon: <Laptop className="h-4 w-4 text-[var(--text-secondary)]" />,
        keywords: ["系统", "跟随", "auto", "system"],
        action: () => setMode("system"),
      },
    ];

    return [...navCmds, ...actionCmds];
  }, [navigate, setMode]);

  const filteredCommands = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) {
      const recentIds = getRecentCommandIds();
      if (recentIds.length === 0) return commands;

      const recentItems: CommandItem[] = [];
      const otherItems: CommandItem[] = [];

      for (const cmd of commands) {
        if (recentIds.includes(cmd.id)) {
          recentItems.push({ ...cmd, category: "最近使用" });
        } else {
          otherItems.push(cmd);
        }
      }

      // Sort recentItems by order in recentIds
      recentItems.sort((a, b) => recentIds.indexOf(a.id) - recentIds.indexOf(b.id));
      return [...recentItems, ...otherItems];
    }

    return commands.filter(
      (cmd) =>
        cmd.title.toLowerCase().includes(q) ||
        (cmd.description && cmd.description.toLowerCase().includes(q)) ||
        cmd.category.toLowerCase().includes(q) ||
        cmd.keywords?.some((k) => k.toLowerCase().includes(q)),
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
        runAndClose(current.id, current.action);
      }
    } else if (event.key === "Escape") {
      event.preventDefault();
      setIsOpen(false);
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
        className="absolute inset-0 bg-[var(--backdrop)] backdrop-blur-md animate-fade-in"
        aria-hidden="true"
      />

      {/* Spotlight Dialog */}
      <div
        role="dialog"
        aria-modal="true"
        aria-label="全局命令中心"
        className="relative w-full max-w-xl overflow-hidden rounded-2xl border border-[var(--border-soft)] bg-[var(--surface-solid)] text-[var(--text-primary)] shadow-[var(--shadow-float)] backdrop-blur-xl animate-scale-in"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        {/* Top Glow bar */}
        <div className="h-1 w-full shrink-0" style={{ backgroundImage: "var(--cyrene-gradient-primary)" }} aria-hidden="true" />

        {/* Search Input Bar */}
        <div className="flex items-center gap-3 border-b border-[var(--border-soft)] px-4 py-3.5">
          <Search className="h-5 w-5 shrink-0 text-[var(--accent-primary)]" />
          <input
            ref={inputRef}
            type="text"
            role="combobox"
            aria-autocomplete="list"
            aria-controls="command-listbox"
            aria-expanded="true"
            aria-activedescendant={filteredCommands[selectedIndex] ? `cmd-${filteredCommands[selectedIndex].id}` : undefined}
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

        {/* Command Listbox */}
        <div
          ref={listRef}
          id="command-listbox"
          role="listbox"
          aria-label="命令建议列表"
          className="max-h-[360px] overflow-y-auto p-2 scrollbar-thin"
        >
          {filteredCommands.length === 0 ? (
            <div className="py-10 text-center text-sm text-[var(--text-faint)]">
              <p>没有匹配的命令或页面</p>
              <p className="mt-1 text-xs text-[var(--text-disabled)]">可以尝试输入关键词、快捷键或页面功能</p>
            </div>
          ) : (
            filteredCommands.map((cmd, index) => {
              const selected = index === selectedIndex;
              return (
                <button
                  type="button"
                  role="option"
                  id={`cmd-${cmd.id}`}
                  aria-selected={selected}
                  key={cmd.id}
                  onClick={() => runAndClose(cmd.id, cmd.action)}
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
