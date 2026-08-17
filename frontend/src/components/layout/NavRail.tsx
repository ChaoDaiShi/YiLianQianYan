import { useEffect, useState } from "react";
import { NavLink, useNavigate } from "react-router-dom";
import { cn } from "../ui/cn";
import { useTheme } from "../../theme";
import { healthCheck } from "../../api/client";
import { NAV_GROUPS } from "./navGroups";

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
      className="z-20 flex h-full w-[68px] shrink-0 flex-col items-center gap-0.5 border-r border-[var(--border)] bg-[var(--sidebar-bg)] py-3 text-[var(--sidebar-text)] min-[960px]:w-[88px] min-[1180px]:w-[220px] min-[1180px]:items-stretch"
    >
      <button
        type="button"
        onClick={() => navigate("/chat")}
        className="mb-2 flex shrink-0 items-center rounded-xl p-1 transition-colors hover:bg-white/10 min-[1180px]:mx-3 min-[1180px]:gap-3 min-[1180px]:px-3"
        title="忆涟千言 · 新对话"
        aria-label="忆涟千言 · 新对话"
      >
        <img
          src="/favicon.png"
          alt=""
          className="h-9 w-9 rounded-lg object-cover"
        />
        <span className="hidden text-sm font-semibold tracking-tight min-[1180px]:block">
          忆涟千言
        </span>
      </button>

      <div className="mb-2 w-8 shrink-0 border-t border-white/15 min-[1180px]:mx-3 min-[1180px]:w-auto" />

      <div
        className="mb-2 hidden shrink-0 items-center gap-2 rounded-lg px-3 py-1.5 text-xs text-[var(--sidebar-text)] opacity-80 min-[1180px]:mx-3 min-[1180px]:flex"
        aria-live="polite"
        aria-label={`后端状态：${backendStatusMeta.label}`}
      >
        <span
          className={`block h-2 w-2 shrink-0 rounded-full ${backendStatusMeta.color}`}
        />
        <span>{backendStatusMeta.label}</span>
      </div>

      <div className="scrollbar-thin min-h-0 w-full flex-1 overflow-y-auto overflow-x-hidden">
        {NAV_GROUPS.map((group, groupIndex) => (
          <div
            key={group.label}
            className={cn(
              "flex w-full flex-col items-center min-[1180px]:items-stretch",
              groupIndex > 0 && "mt-2 pt-2 min-[1180px]:mt-3 min-[1180px]:pt-3",
            )}
          >
            {groupIndex > 0 && (
              <div
                className="mb-2 w-8 border-t border-white/10 min-[1180px]:mx-3 min-[1180px]:mb-3 min-[1180px]:w-auto"
                aria-hidden="true"
              />
            )}
            <div className="mb-1 hidden px-3 text-xs tracking-[0.12em] text-[var(--sidebar-muted)] min-[1180px]:block">
              {group.label}
            </div>
            {group.items.map((item) => {
              const Icon = item.icon;
              return (
                <NavLink
                  key={item.id}
                  to={item.to}
                  className={({ isActive }) =>
                    cn(
                      "relative flex w-[60px] flex-col items-center gap-0.5 rounded-lg py-1.5 text-[10px] leading-tight transition-colors min-[960px]:w-[76px] min-[1180px]:mx-3 min-[1180px]:w-auto min-[1180px]:flex-row min-[1180px]:gap-2 min-[1180px]:rounded-xl min-[1180px]:px-3 min-[1180px]:py-2 min-[1180px]:text-sm",
                      isActive
                        ? "bg-[var(--sidebar-active)] font-medium text-[var(--accent-primary)]"
                        : "text-[var(--sidebar-text)] opacity-80 hover:bg-white/10 hover:opacity-100",
                    )
                  }
                  title={item.label}
                >
                  {({ isActive }) => (
                    <>
                      <Icon
                        className="h-[18px] w-[18px]"
                        strokeWidth={isActive ? 2.2 : 1.8}
                      />
                      <span
                        className={cn(
                          "whitespace-nowrap",
                          theme.monoTitles && "font-mono",
                        )}
                      >
                        {item.label}
                      </span>
                      {isActive && (
                        <span className="absolute left-0 top-1/2 h-7 w-0.5 -translate-y-1/2 rounded-r-full bg-[var(--accent-primary)]" />
                      )}
                      {isActive && (
                        <span
                          className="absolute right-3 hidden text-xs text-[var(--accent-gold)] min-[1180px]:block"
                          aria-hidden="true"
                        >
                          ✦
                        </span>
                      )}
                    </>
                  )}
                </NavLink>
              );
            })}
          </div>
        ))}
      </div>

      <div
        className="mb-1 mt-2 flex shrink-0 flex-col items-center gap-1 text-[9px] text-[var(--sidebar-text)] opacity-80 min-[1180px]:hidden"
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
