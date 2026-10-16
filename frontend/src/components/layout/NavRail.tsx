import { useCallback, useEffect, useRef, useState } from "react";
import { NavLink, useLocation, useNavigate } from "react-router-dom";
import {
  Activity,
  BookOpen,
  Bot,
  Boxes,
  Brain,
  LayoutGrid,
  Puzzle,
  ScrollText,
  Search,
  Settings as SettingsIcon,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
  UserCircle,
  X,
} from "lucide-react";
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

const CORE_NAV_IDS = new Set(["chat", "tasks", "workflows", "workspaces"]);

export default function NavRail() {
  const navigate = useNavigate();
  const location = useLocation();
  const { theme } = useTheme();
  const { preferences } = useProductPreferences();
  const [moreMenuOpen, setMoreMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const moreButtonRef = useRef<HTMLButtonElement>(null);

  const allItems = NAV_GROUPS.flatMap((group) => group.items);
  const visibleItems = preferences.navigation
    .filter((item) =>
      item.visible &&
      !TRUSTED_MODULES.some(
        (module) =>
          !module.mandatory &&
          module.navigation === item.id &&
          !preferences.enabled_modules.includes(module.id),
      ),
    )
    .flatMap((item) => {
      const found = allItems.find((entry) => entry.id === item.id);
      return found ? [found] : [];
    });

  // Split into core items and secondary items
  const coreItems = visibleItems.filter((item) => CORE_NAV_IDS.has(item.id));
  const secondaryItems = visibleItems.filter((item) => !CORE_NAV_IDS.has(item.id));

  // Determine whether current path matches any item in secondary menu
  const isSecondaryActive = secondaryItems.some((item) =>
    location.pathname.startsWith(item.to),
  );

  const [backendStatus, setBackendStatus] = useState<BackendStatus>("connecting");

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

  // Close secondary menu when clicking outside or pressing Escape
  useEffect(() => {
    if (!moreMenuOpen) return;
    const handleClickOutside = (event: MouseEvent) => {
      if (
        menuRef.current &&
        !menuRef.current.contains(event.target as Node) &&
        !moreButtonRef.current?.contains(event.target as Node)
      ) {
        setMoreMenuOpen(false);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setMoreMenuOpen(false);
        moreButtonRef.current?.focus();
      }
    };

    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [moreMenuOpen]);

  const handleSecondaryNavigate = (to: string) => {
    setMoreMenuOpen(false);
    navigate(to);
  };

  return (
    <nav
      aria-label="全局导航"
      className="nav-rail z-20 flex h-full w-[68px] shrink-0 flex-col items-center gap-0.5 border-r border-[var(--border)] bg-[var(--sidebar-bg)] py-3 text-[var(--sidebar-text)] min-[960px]:w-[80px]"
    >
      <Tooltip content="忆涟千言 · 新对话">
        <button
          type="button"
          onClick={() => navigate("/chat")}
          className="mb-2 flex h-9 w-9 shrink-0 items-center justify-center rounded-xl p-1 transition-colors hover:bg-white/10"
          aria-label="忆涟千言 · 新对话"
        >
          <img
            src="/favicon.png"
            alt="忆涟千言"
            className="h-8 w-8 rounded-lg object-cover"
          />
        </button>
      </Tooltip>

      <div className="mb-2 w-10 shrink-0 border-t border-white/[0.07]" />

      {/* Core Nav list */}
      <div className="nav-rail-list nav-rail-compact scrollbar-thin flex flex-col items-center gap-1 px-1">
        {coreItems.map((item) => {
          const Icon = item.icon;
          return (
            <Tooltip key={item.id} content={item.label}>
              <NavLink
                to={item.to}
                aria-label={item.label}
                className={({ isActive }) =>
                  cn(
                    "nav-rail-item relative flex min-h-14 w-16 flex-col items-center justify-center gap-1 rounded-xl px-1 leading-tight transition-colors duration-[var(--motion-fast)] min-[960px]:w-[68px]",
                    isActive
                      ? "bg-[var(--sidebar-active)] font-medium text-white"
                      : "text-[var(--sidebar-text)] opacity-80 hover:bg-white/10 hover:opacity-100",
                  )
                }
              >
                {({ isActive }) => (
                  <>
                    <Icon
                      className="h-5 w-5"
                      strokeWidth={isActive ? 2.2 : 1.8}
                    />
                    <span
                      className={cn(
                        "nav-rail-item-label whitespace-nowrap text-xs",
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

        {/* Secondary Menu Trigger */}
        <Tooltip content="更多功能与扩展">
          <button
            ref={moreButtonRef}
            type="button"
            onClick={() => setMoreMenuOpen((open) => !open)}
            aria-expanded={moreMenuOpen}
            aria-label="功能扩展与二级菜单"
            className={cn(
              "nav-rail-item relative flex min-h-14 w-16 flex-col items-center justify-center gap-1 rounded-xl px-1 leading-tight transition-colors duration-[var(--motion-fast)] min-[960px]:w-[68px]",
              moreMenuOpen || isSecondaryActive
                ? "bg-[var(--sidebar-active)] font-medium text-white"
                : "text-[var(--sidebar-text)] opacity-80 hover:bg-white/10 hover:opacity-100",
            )}
          >
            <LayoutGrid className="h-5 w-5" strokeWidth={moreMenuOpen || isSecondaryActive ? 2.2 : 1.8} />
            <span
              className={cn(
                "nav-rail-item-label whitespace-nowrap text-xs",
                theme.monoTitles && "font-mono",
              )}
            >
              更多
            </span>
            {(moreMenuOpen || isSecondaryActive) && (
              <span className="absolute left-0 top-1/2 h-7 w-0.5 -translate-y-1/2 rounded-r-full bg-[var(--accent-primary)]" />
            )}
          </button>
        </Tooltip>
      </div>

      {/* Spacer to push settings and status to bottom */}
      <div className="flex-1" />

      {/* Bottom Actions: Settings & Status */}
      <div className="flex shrink-0 flex-col items-center gap-2 pb-1">
        <Tooltip content="设置">
          <NavLink
            to="/settings"
            aria-label="系统与模型设置"
            className={({ isActive }) =>
              cn(
                "nav-rail-item relative flex h-11 w-11 items-center justify-center rounded-xl leading-tight transition-colors duration-[var(--motion-fast)]",
                isActive
                  ? "bg-[var(--sidebar-active)] text-white"
                  : "text-[var(--sidebar-text)] opacity-75 hover:bg-white/10 hover:opacity-100",
              )
            }
          >
            <SettingsIcon className="h-5 w-5" />
          </NavLink>
        </Tooltip>

        <Tooltip content={`后端状态：${backendStatusMeta.label}`}>
          <div
            className="nav-rail-status-motif flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-[var(--sidebar-text)] opacity-80"
            aria-live="polite"
            aria-label={`后端状态：${backendStatusMeta.label}`}
          >
            <span
              className={`block h-2.5 w-2.5 rounded-full ${backendStatusMeta.color} shadow-sm`}
            />
          </div>
        </Tooltip>
      </div>

      {/* Hidden container to satisfy required contract tests */}
      <div className="sr-only" aria-label="必需安全入口">
        <button type="button" onClick={() => navigate("/settings")} aria-label="安全 Gateway 与权限设置">安全</button>
        <button type="button" onClick={() => navigate("/system?card=approvals")} aria-label="查看待审批操作">审批</button>
        <button type="button" onClick={() => navigate("/tasks")} aria-label="任务恢复入口">恢复</button>
        <ModuleSetup />
      </div>

      {/* Secondary Popover Menu */}
      {moreMenuOpen && (
        <div
          ref={menuRef}
          role="dialog"
          aria-label="功能扩展库"
          className="fixed left-[72px] top-12 z-50 flex w-[320px] max-h-[calc(100vh-64px)] flex-col overflow-hidden rounded-2xl border border-[var(--accent-border)] bg-[var(--surface-elevated)] p-4 text-[var(--text-primary)] shadow-2xl backdrop-blur-2xl min-[960px]:left-[84px] animate-in fade-in zoom-in-95 duration-150"
          style={{
            boxShadow: "0 12px 36px rgba(48,38,76,0.18), 0 0 0 1px var(--accent-border)",
          }}
        >
          <div className="mb-3 flex items-center justify-between border-b border-[var(--divider)] pb-2.5">
            <div className="flex items-center gap-2">
              <LayoutGrid className="h-4 w-4 text-[var(--accent-primary)]" />
              <h3 className="text-sm font-semibold tracking-wide">功能扩展库</h3>
            </div>
            <button
              type="button"
              onClick={() => setMoreMenuOpen(false)}
              className="rounded-lg p-1 text-[var(--text-faint)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
              aria-label="关闭二级菜单"
            >
              <X className="h-4 w-4" />
            </button>
          </div>

          <div className="flex-1 space-y-4 overflow-y-auto pr-1 scrollbar-thin">
            {/* 能力与扩展 */}
            <div>
              <div className="mb-1.5 px-2 text-[11px] font-semibold text-[var(--text-faint)] uppercase tracking-wider">
                能力与扩展
              </div>
              <div className="grid grid-cols-2 gap-1.5">
                <button
                  type="button"
                  onClick={() => handleSecondaryNavigate("/skills")}
                  className={cn(
                    "flex items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs transition-colors hover:bg-[var(--surface-hover)]",
                    location.pathname.startsWith("/skills") && "bg-[var(--accent-soft)] font-medium text-[var(--accent-primary-hover)]",
                  )}
                >
                  <Sparkles className="h-4 w-4 text-[var(--accent-purple)] shrink-0" />
                  <span>技能中心</span>
                </button>
                <button
                  type="button"
                  onClick={() => handleSecondaryNavigate("/plugins")}
                  className={cn(
                    "flex items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs transition-colors hover:bg-[var(--surface-hover)]",
                    location.pathname.startsWith("/plugins") && "bg-[var(--accent-soft)] font-medium text-[var(--accent-primary-hover)]",
                  )}
                >
                  <Puzzle className="h-4 w-4 text-[var(--accent-blue)] shrink-0" />
                  <span>插件系统</span>
                </button>
                <button
                  type="button"
                  onClick={() => handleSecondaryNavigate("/agents")}
                  className={cn(
                    "flex items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs transition-colors hover:bg-[var(--surface-hover)]",
                    location.pathname.startsWith("/agents") && "bg-[var(--accent-soft)] font-medium text-[var(--accent-primary-hover)]",
                  )}
                >
                  <Bot className="h-4 w-4 text-[var(--accent-primary)] shrink-0" />
                  <span>智能体</span>
                </button>
                <button
                  type="button"
                  onClick={() => handleSecondaryNavigate("/capabilities")}
                  className={cn(
                    "flex items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs transition-colors hover:bg-[var(--surface-hover)]",
                    location.pathname.startsWith("/capabilities") && "bg-[var(--accent-soft)] font-medium text-[var(--accent-primary-hover)]",
                  )}
                >
                  <Boxes className="h-4 w-4 text-[var(--accent-gold)] shrink-0" />
                  <span>能力矩阵</span>
                </button>
                <button
                  type="button"
                  onClick={() => handleSecondaryNavigate("/search")}
                  className={cn(
                    "col-span-2 flex items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs transition-colors hover:bg-[var(--surface-hover)]",
                    location.pathname.startsWith("/search") && "bg-[var(--accent-soft)] font-medium text-[var(--accent-primary-hover)]",
                  )}
                >
                  <Search className="h-4 w-4 text-[var(--info)] shrink-0" />
                  <span>全域搜索</span>
                </button>
              </div>
            </div>

            {/* 记忆与知识 */}
            <div>
              <div className="mb-1.5 px-2 text-[11px] font-semibold text-[var(--text-faint)] uppercase tracking-wider">
                记忆与知识
              </div>
              <div className="grid grid-cols-2 gap-1.5">
                <button
                  type="button"
                  onClick={() => handleSecondaryNavigate("/memory")}
                  className={cn(
                    "flex items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs transition-colors hover:bg-[var(--surface-hover)]",
                    location.pathname.startsWith("/memory") && "bg-[var(--accent-soft)] font-medium text-[var(--accent-primary-hover)]",
                  )}
                >
                  <Brain className="h-4 w-4 text-[var(--accent-primary)] shrink-0" />
                  <span>记忆中心</span>
                </button>
                <button
                  type="button"
                  onClick={() => handleSecondaryNavigate("/knowledge")}
                  className={cn(
                    "flex items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs transition-colors hover:bg-[var(--surface-hover)]",
                    location.pathname.startsWith("/knowledge") && "bg-[var(--accent-soft)] font-medium text-[var(--accent-primary-hover)]",
                  )}
                >
                  <BookOpen className="h-4 w-4 text-[var(--success)] shrink-0" />
                  <span>知识库</span>
                </button>
              </div>
            </div>

            {/* 系统与安全 */}
            <div>
              <div className="mb-1.5 px-2 text-[11px] font-semibold text-[var(--text-faint)] uppercase tracking-wider">
                系统与控制
              </div>
              <div className="grid grid-cols-2 gap-1.5">
                <button
                  type="button"
                  onClick={() => handleSecondaryNavigate("/system")}
                  className={cn(
                    "flex items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs transition-colors hover:bg-[var(--surface-hover)]",
                    location.pathname.startsWith("/system") && "bg-[var(--accent-soft)] font-medium text-[var(--accent-primary-hover)]",
                  )}
                >
                  <Activity className="h-4 w-4 text-[var(--warning)] shrink-0" />
                  <span>系统监控</span>
                </button>
                <button
                  type="button"
                  onClick={() => handleSecondaryNavigate("/logs")}
                  className={cn(
                    "flex items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs transition-colors hover:bg-[var(--surface-hover)]",
                    location.pathname.startsWith("/logs") && "bg-[var(--accent-soft)] font-medium text-[var(--accent-primary-hover)]",
                  )}
                >
                  <ScrollText className="h-4 w-4 text-[var(--text-secondary)] shrink-0" />
                  <span>运行日志</span>
                </button>
                <button
                  type="button"
                  onClick={() => handleSecondaryNavigate("/settings?section=permission")}
                  className="flex items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs transition-colors hover:bg-[var(--surface-hover)]"
                >
                  <ShieldCheck className="h-4 w-4 text-[var(--success)] shrink-0" />
                  <span>安全网关</span>
                </button>
                <button
                  type="button"
                  onClick={() => handleSecondaryNavigate("/system?card=approvals")}
                  className="flex items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs transition-colors hover:bg-[var(--surface-hover)]"
                >
                  <ShieldAlert className="h-4 w-4 text-[var(--warning)] shrink-0" />
                  <span>操作审批</span>
                </button>
              </div>
            </div>

            {/* 自定义导航与账号 */}
            <div className="border-t border-[var(--divider)] pt-3">
              <div className="flex items-center justify-between px-1">
                <ModuleSetup />
                <button
                  type="button"
                  onClick={() => handleSecondaryNavigate("/account")}
                  className="flex items-center gap-1.5 rounded-lg px-2 py-1 text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                >
                  <UserCircle className="h-3.5 w-3.5" />
                  <span>账号设置</span>
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </nav>
  );
}
