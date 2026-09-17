import { useCallback, useState } from "react";
import { NavLink, useNavigate } from "react-router-dom";
import { cn } from "../ui/cn";
import { Tooltip } from "../ui";
import { useTheme } from "../../theme";
import { healthCheck } from "../../api/client";
import { NAV_GROUPS } from "./navGroups";
import ModuleSetup from "../system/ModuleSetup";
import { useProductPreferences } from "../system/useProductPreferences";
import { TRUSTED_MODULES } from "../system/productPreferences";
import { useVisibleRefresh } from "../system/useVisibleRefresh";

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
  const { preferences } = useProductPreferences();
  const items = NAV_GROUPS.flatMap((group) => group.items);
  const groups = [{ label: "导航", items: preferences.navigation.filter((item) => item.visible
    && !TRUSTED_MODULES.some((module) => !module.mandatory && module.navigation === item.id && !preferences.enabled_modules.includes(module.id)))
    .flatMap((item) => { const found = items.find((entry) => entry.id === item.id); return found ? [found] : []; }) }];
  const [backendStatus, setBackendStatus] =
    useState<BackendStatus>("connecting");

  const refreshBackendStatus = useCallback(async () => {
      const health = await healthCheck();
      setBackendStatus(
        health?.status === "healthy" && health.database === "healthy"
          ? "healthy"
          : "unavailable",
      );
  }, []);
  useVisibleRefresh(refreshBackendStatus);

  const backendStatusMeta = BACKEND_STATUS_META[backendStatus];

  return (
    <nav
      aria-label="全局导航"
      className="nav-rail z-20 flex h-full w-[68px] shrink-0 flex-col items-center gap-0.5 border-r border-[var(--border)] bg-[var(--sidebar-bg)] py-3 text-[var(--sidebar-text)] min-[960px]:w-[80px]"
    >
      <Tooltip content="忆涟千言">
        <button
          type="button"
          onClick={() => navigate("/chat")}
          className="mb-2 flex h-9 w-9 shrink-0 items-center justify-center rounded-xl p-1 transition-colors hover:bg-white/10"
          aria-label="忆涟千言 · 新对话"
        >
          <img
            src="/favicon.png"
            alt=""
            className="h-8 w-8 rounded-lg object-cover"
          />
        </button>
      </Tooltip>

      <div className="mb-2 w-10 shrink-0 border-t border-white/[0.07]" />

      <div className="nav-rail-list nav-rail-compact scrollbar-thin min-h-0 w-full flex-1 overflow-y-auto overflow-x-hidden px-1">
        {groups.map((group, groupIndex) => (
          <div
            key={group.label}
            className={cn(
              "nav-rail-group flex w-full flex-col items-center",
              groupIndex > 0 && "nav-rail-group-separated",
            )}
          >
            {groupIndex > 0 && (
              <div
                className="mb-3 w-10 border-t border-white/[0.07]"
                aria-hidden="true"
              />
            )}
            {group.items.map((item) => {
              const Icon = item.icon;
              return (
                <Tooltip key={item.id} content={item.label}>
                  <NavLink
                    to={item.to}
                    aria-label={item.label}
                    className={({ isActive }) =>
                      cn(
                        "nav-rail-item relative flex min-h-14 w-16 flex-col items-center justify-center gap-1 rounded-xl px-1 text-[11px] leading-tight transition-colors duration-[var(--motion-fast)] min-[960px]:w-[68px]",
                        isActive
                          ? "bg-[var(--sidebar-active)] font-medium text-white"
                          : "text-[var(--sidebar-text)] opacity-80 hover:bg-white/10 hover:opacity-100",
                      )
                    }
                  >
                    {({ isActive }) => (
                      <>
                        <Icon
                          className="h-[18px] w-[18px]"
                          strokeWidth={isActive ? 2.2 : 1.8}
                        />
                        <span
                          className={cn(
                            "nav-rail-item-label whitespace-nowrap",
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
                            className="absolute right-1 top-1 text-[10px] text-[var(--accent-gold)]"
                            aria-hidden="true"
                          >
                            ✦
                          </span>
                        )}
                      </>
                    )}
                  </NavLink>
                </Tooltip>
              );
            })}
          </div>
        ))}
      </div>

      <div className="flex shrink-0 flex-col items-center gap-1 text-[10px]" aria-label="必需安全入口">
        <button type="button" onClick={() => navigate("/settings")} aria-label="安全 Gateway 与权限设置">安全</button>
        <button type="button" onClick={() => navigate("/system?card=approvals")} aria-label="查看待审批操作">审批</button>
        <button type="button" onClick={() => navigate("/tasks")} aria-label="任务恢复入口">恢复</button>
        <ModuleSetup />
      </div>

      <Tooltip content={`后端状态：${backendStatusMeta.label}`}>
        <div
          className="nav-rail-status-motif mb-1 mt-2 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-[var(--sidebar-text)] opacity-80"
          aria-live="polite"
          aria-label={`后端状态：${backendStatusMeta.label}`}
        >
          <span
            className={`block h-2 w-2 rounded-full ${backendStatusMeta.color}`}
          />
        </div>
      </Tooltip>
    </nav>
  );
}
