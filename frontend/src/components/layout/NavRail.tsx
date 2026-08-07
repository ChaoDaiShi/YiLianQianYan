import { NavLink, useNavigate, useParams } from "react-router-dom";
import {
  MessageSquare,
  Activity,
  ScrollText,
  Sparkles,
  Puzzle,
  GitBranch,
  BookOpen,
  Settings,
} from "lucide-react";
import { cn } from "../ui/cn";
import { useTheme } from "../../theme";

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

export default function NavRail() {
  const navigate = useNavigate();
  const params = useParams();
  const { theme } = useTheme();
  const hasActiveChat = Boolean(params.id);

  return (
    <nav className="w-[72px] flex-shrink-0 flex flex-col items-center py-3 gap-0.5 border-r border-[var(--border)] bg-[var(--panel)]/80 backdrop-blur-md z-20">
      <button
        onClick={() => navigate("/chat")}
        className="mb-2 p-1 rounded-xl hover:bg-[var(--panel-hover)] transition-colors"
        title="忆涟千言 · 新对话"
      >
        <img src="/favicon.png" alt="忆涟千言" className="w-9 h-9 rounded-lg object-cover" />
      </button>

      <div className="w-8 border-t border-[var(--border)] mb-2" />

      {NAV_ITEMS.map((item) => {
        const Icon = item.icon;
        return (
          <NavLink
            key={item.id}
            to={item.to}
            className={({ isActive }) =>
              cn(
                "relative w-[60px] flex flex-col items-center gap-0.5 py-2 rounded-xl text-[10px] transition-all duration-150",
                isActive
                  ? "bg-[var(--accent)]/15 text-[var(--accent)] font-medium"
                  : "text-[var(--text-muted)] hover:bg-[var(--panel-hover)] hover:text-[var(--text)]"
              )
            }
            title={item.label}
          >
            {({ isActive }) => (
              <>
                <Icon className="w-5 h-5" strokeWidth={isActive ? 2.2 : 1.8} />
                <span className={cn(theme.monoTitles && "font-mono")}>
                  {item.label}
                </span>
                {isActive && (
                  <span className="absolute left-0 top-1/2 -translate-y-1/2 w-0.5 h-7 bg-[var(--accent)] rounded-r-full" />
                )}
              </>
            )}
          </NavLink>
        );
      })}

      {hasActiveChat && (
        <div className="mt-auto mb-2">
          <span className="w-2 h-2 rounded-full bg-[var(--success)] block animate-pulse" title="对话进行中" />
        </div>
      )}
    </nav>
  );
}
