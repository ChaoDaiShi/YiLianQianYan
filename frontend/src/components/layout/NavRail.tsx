import { useEffect, useState } from "react";
import { NavLink, useNavigate } from "react-router-dom";
import {
  Activity,
  BookOpen,
  GitBranch,
  MessageSquare,
  Puzzle,
  ScrollText,
  Settings,
  Sparkles,
} from "lucide-react";
import { cn } from "../ui/cn";
import { useTheme } from "../../theme";
import { healthCheck } from "../../api/client";

const NAV_ITEMS = [
  { to: "/chat", id: "chat", label: "对话", icon: MessageSquare },
  { to: "/system", id: "system", label: "监控", icon: Activity },
  { to: "/logs", id: "logs", label: "日志", icon: ScrollText },
  { to: "/skills", id: "skills", label: "技能", icon: Sparkles },
  { to: "/plugins", id: "plugins", label: "插件", icon: Puzzle },
  { to: "/workflows", id: "workflows", label: "工作流", icon: GitBranch },
  { to: "/knowledge", id: "knowledge", label: "知识库", icon: BookOpen },
  { to: "/settings", id: "settings", label: "设置", icon: Settings },
] as const;

type BackendStatus = "connecting" | "healthy" | "unavailable";

const BACKEND_STATUS_META: Record<
  BackendStatus,
  { label: string; color: string }
> = {
  connecting: { label: "连接中", color: "bg-[var(--warning)]" },
  healthy: { label: "后端正常", color: "bg-[var(--success)]" },
  unavailable: { label: "后端不可用", color: "bg-[var(--danger)]" },
};

export default function NavRail() {
  const navigate = useNavigate();
  const { theme } = useTheme();
  const [backendStatus, setBackendStatus] =
    useState<BackendStatus>("connecting");

  useEffect(() => {
    let active = true;

    const refreshBackendStatus = async () => {
      const health = await healthCheck();
      if (!active) return;
      setBackendStatus(
        health?.status === "healthy" && health.database === "healthy"
          ? "healthy"
          : "unavailable",
      );
    };

    void refreshBackendStatus();
    const interval = window.setInterval(refreshBackendStatus, 30_000);
    return () => {
      active = false;
      window.clearInterval(interval);
    };
  }, []);

  const backendStatusMeta = BACKEND_STATUS_META[backendStatus];

  return (
    <nav
      aria-label="全局导航"
      className="z-20 flex h-full w-[68px] shrink-0 flex-col items-center gap-0.5 border-r border-[var(--border)] bg-[var(--nav)] py-3 text-[var(--nav-text)] min-[960px]:w-[88px]"
    >
      <button
        type="button"
        onClick={() => navigate("/chat")}
        className="mb-2 rounded-xl p-1 transition-colors hover:bg-white/10"
        title="忆涟千言 · 新对话"
        aria-label="忆涟千言 · 新对话"
      >
        <img
          src="/favicon.png"
          alt=""
          className="h-9 w-9 rounded-lg object-cover"
        />
      </button>

      <div className="mb-2 w-8 border-t border-white/15" />

      {NAV_ITEMS.map((item) => {
        const Icon = item.icon;
        return (
          <NavLink
            key={item.id}
            to={item.to}
            className={({ isActive }) =>
              cn(
                "relative flex w-[60px] flex-col items-center gap-0.5 rounded-lg py-1.5 text-[10px] leading-tight transition-colors min-[960px]:w-[76px]",
                isActive
                  ? "bg-white/10 font-medium text-[var(--accent)]"
                  : "text-[var(--nav-text)] opacity-80 hover:bg-white/10 hover:opacity-100"
              )
            }
            title={item.label}
          >
            {({ isActive }) => (
              <>
                <Icon className="h-[18px] w-[18px]" strokeWidth={isActive ? 2.2 : 1.8} />
                <span className={cn("whitespace-nowrap", theme.monoTitles && "font-mono")}>
                  {item.label}
                </span>
                {isActive && (
                  <span className="absolute left-0 top-1/2 h-7 w-0.5 -translate-y-1/2 rounded-r-full bg-[var(--accent)]" />
                )}
              </>
            )}
          </NavLink>
        );
      })}

      <div
        className="mt-auto mb-1 flex flex-col items-center gap-1 text-[9px] text-[var(--nav-text)] opacity-80"
        aria-live="polite"
        aria-label={`后端状态：${backendStatusMeta.label}`}
        title={`后端状态：${backendStatusMeta.label}`}
      >
        <span
          className={`block h-2 w-2 rounded-full ${backendStatusMeta.color}`}
        />
        <span>{backendStatusMeta.label}</span>
      </div>
    </nav>
  );
}
