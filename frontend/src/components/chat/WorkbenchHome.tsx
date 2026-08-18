import { Activity, FileCode, Folder, ListChecks, type LucideIcon } from "lucide-react";
import type { AgentRunState } from "../../features/execution/model";
import type { PendingApproval } from "../../types/approval";
import {
  AGENT_STATUS_LABELS,
  resolveAgentStatus,
  type AgentStatus,
} from "./agentStatus";
import ChatInput from "./ChatInput";

interface WorkbenchHomeProps {
  connection: AgentRunState["connection"];
  pendingApprovals: PendingApproval[];
  isLoading: boolean;
  finished?: boolean;
  onSend: (text: string) => void;
  onStop: () => void;
  suggestedText?: string;
  onTextUsed?: () => void;
  onHint?: (text: string) => void;
}

interface QuickAction {
  icon: LucideIcon;
  title: string;
  subtitle: string;
  prompt: string;
}

const QUICK_ACTIONS: QuickAction[] = [
  {
    icon: Folder,
    title: "查看文件目录",
    subtitle: "整理文件",
    prompt: "列出当前目录的文件",
  },
  {
    icon: ListChecks,
    title: "查找 TODO",
    subtitle: "扫描代码标记",
    prompt: "搜索包含 TODO 的文件",
  },
  {
    icon: FileCode,
    title: "编写脚本",
    subtitle: "生成或修改脚本",
    prompt: "创建一个 Python 脚本",
  },
  {
    icon: Activity,
    title: "查看系统进程",
    subtitle: "分析当前运行状态",
    prompt: "查看系统进程",
  },
];

function greeting(): string {
  const hour = new Date().getHours();
  if (hour >= 5 && hour < 12) return "早上好，主人～";
  if (hour >= 12 && hour < 18) return "下午好，主人～";
  return "晚上好，主人～";
}

const STATUS_META: Record<
  AgentStatus,
  { label: string; dotClass: string; pulse?: boolean }
> = {
  idle: {
    label: AGENT_STATUS_LABELS.idle,
    dotClass: "bg-[var(--text-faint)]",
  },
  running: {
    label: AGENT_STATUS_LABELS.running,
    dotClass: "bg-[var(--accent-primary)]",
    pulse: true,
  },
  approval: {
    label: AGENT_STATUS_LABELS.approval,
    dotClass: "bg-[var(--accent-gold)]",
    pulse: true,
  },
  finished: {
    label: AGENT_STATUS_LABELS.finished,
    dotClass: "bg-[var(--success)]",
  },
  error: {
    label: AGENT_STATUS_LABELS.error,
    dotClass: "bg-[var(--danger)]",
  },
};

export default function WorkbenchHome({
  connection,
  pendingApprovals,
  isLoading,
  finished = false,
  onSend,
  onStop,
  suggestedText,
  onTextUsed,
  onHint,
}: WorkbenchHomeProps) {
  const status = resolveAgentStatus({
    connection,
    hasApproval: pendingApprovals.length > 0,
    isLoading,
    finished,
  });
  const statusMeta = STATUS_META[status];

  return (
    <div className="scrollbar-thin relative flex min-h-0 flex-1 flex-col overflow-y-auto">
      <div
        className="home-ambient home-ambient-strong pointer-events-none absolute inset-0"
        aria-hidden="true"
      />
      <div
        className="home-ambient-stars pointer-events-none absolute inset-0 opacity-70"
        aria-hidden="true"
      />

      <div className="home-content relative z-10 flex min-h-full flex-1 flex-col items-center justify-center px-6 py-8">
        <div className="home-hero">
          <div className="text-center">
            <h1 className="text-2xl font-semibold tracking-tight text-[var(--text)]">
              {greeting()}{" "}
              <span className="text-[var(--accent-gold)]" aria-hidden="true">
                ✦
              </span>
            </h1>
            <p className="mt-2 text-sm text-[var(--text-secondary)]">
              有什么想让我帮你完成的吗？
            </p>
          </div>

          <div className="home-character relative">
            <div
              className="absolute -inset-6 -z-10 rounded-full bg-[radial-gradient(circle,rgba(234,145,185,0.16),rgba(234,145,185,0)_70%)]"
              aria-hidden="true"
            />
            <img
              src="/cyrene-home-character.png"
              alt="小昔涟"
              className="home-character-image h-[168px] w-[168px] object-contain"
            />
          </div>
        </div>

        <div
          className="home-status flex items-center gap-2 rounded-full border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3.5 py-1.5 text-xs text-[var(--text-secondary)] shadow-[var(--shadow-card)]"
          aria-live="polite"
        >
          <span
            className={`inline-block h-2 w-2 rounded-full ${statusMeta.dotClass} ${
              statusMeta.pulse ? "animate-pulse" : ""
            }`}
          />
          {statusMeta.label}
        </div>

        <ChatInput
          onSend={onSend}
          isLoading={isLoading}
          onStop={onStop}
          suggestedText={suggestedText}
          onTextUsed={onTextUsed}
        />

        <div className="home-quick-actions">
          {QUICK_ACTIONS.map((action) => {
            const Icon = action.icon;
            return (
              <button
                key={action.title}
                type="button"
                onClick={() => onHint?.(action.prompt)}
                className="home-quick-action flex flex-col items-start gap-2 rounded-xl border border-[var(--border-soft)] bg-[var(--surface-solid)] p-3 text-left transition-[background-color,border-color] duration-[var(--motion-fast)] hover:border-[var(--accent-border)]"
              >
                <Icon className="h-4 w-4 text-[var(--accent-purple)]" />
                <span>
                  <span className="block text-sm font-medium text-[var(--text)]">
                    {action.title}
                  </span>
                  <span className="mt-0.5 block text-xs text-[var(--text-faint)]">
                    {action.subtitle}
                  </span>
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
