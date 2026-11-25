import { Activity, FileCode, Folder, ListChecks, type LucideIcon } from "lucide-react";
import type { AgentRunState } from "../../features/execution/model";
import type { PendingApproval } from "../../types/approval";
import {
  AGENT_STATUS_LABELS,
  resolveAgentStatus,
  type AgentStatus,
} from "./agentStatus";
import ChatInput, { type RestoredComposerDraft } from "./ChatInput";
import type { ChatRequestOptions, WorkspaceMode } from "./chatRequestOptions";
import type { ChatVoiceControls } from "../../features/voice/ChatVoiceInput";

interface WorkbenchHomeProps {
  conversationId?: string | null;
  connection: AgentRunState["connection"];
  pendingApprovals: PendingApproval[];
  isLoading: boolean;
  finished?: boolean;
  onSend: (text: string, resourceIds?: string[], options?: ChatRequestOptions) => void | boolean | Promise<void | boolean>;
  onStop: () => void;
  suggestedText?: string;
  onTextUsed?: () => void;
  restoredDraft?: RestoredComposerDraft | null;
  inputDisabled?: boolean;
  onHint?: (text: string) => void;
  onOpenCurrentTask: () => void;
  chatVoiceControls?: ChatVoiceControls | null;
  onRequestGlobalVoiceSession?: () => void;
  workspaceMode?: WorkspaceMode;
  onWorkspaceModeChange?: (mode: WorkspaceMode) => Promise<void>;
  modeChanging?: boolean;
  modeError?: string | null;
}

interface QuickAction {
  icon: LucideIcon;
  title: string;
  prompt: string;
}

const QUICK_ACTIONS: QuickAction[] = [
  {
    icon: Folder,
    title: "查看文件目录",
    prompt: "列出当前目录的文件",
  },
  {
    icon: ListChecks,
    title: "查找 TODO",
    prompt: "搜索包含 TODO 的文件",
  },
  {
    icon: FileCode,
    title: "编写脚本",
    prompt: "创建一个 Python 脚本",
  },
  {
    icon: Activity,
    title: "查看系统进程",
    prompt: "查看系统进程",
  },
];

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
  conversationId,
  connection,
  pendingApprovals,
  isLoading,
  finished = false,
  onSend,
  onStop,
  suggestedText,
  onTextUsed,
  restoredDraft,
  inputDisabled,
  onHint,
  onOpenCurrentTask,
  chatVoiceControls,
  onRequestGlobalVoiceSession,
  workspaceMode,
  onWorkspaceModeChange,
  modeChanging,
  modeError,
}: WorkbenchHomeProps) {
  const status = resolveAgentStatus({
    connection,
    hasApproval: pendingApprovals.length > 0,
    isLoading,
    finished,
  });
  const statusMeta = STATUS_META[status];
  const taskShortcutLabel = conversationId ? "打开当前任务" : "打开任务工作流";

  return (
    <div className="scrollbar-thin relative flex min-h-0 flex-1 flex-col overflow-y-auto">
      <div className="home-content relative z-10 flex min-h-full flex-1 flex-col items-center justify-center px-6 py-8">
        <div className="home-hero">
          <button
            type="button"
            onClick={onOpenCurrentTask}
            aria-label={taskShortcutLabel}
            title={taskShortcutLabel}
            className="home-brand-button"
          >
            <img src="/favicon.png" alt="" width={40} height={40} />
          </button>
          <h1>今天想做什么？</h1>
        </div>

        {status !== "idle" && <div
          className="home-status flex items-center gap-2 rounded-full border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3.5 py-1.5 text-xs text-[var(--text-secondary)] shadow-[var(--shadow-card)]"
          aria-live="polite"
        >
          <span
            className={`inline-block h-2 w-2 rounded-full ${statusMeta.dotClass} ${
              statusMeta.pulse ? "animate-pulse" : ""
            }`}
          />
          {statusMeta.label}
        </div>}

        <ChatInput
          workspaceMode={workspaceMode}
          onWorkspaceModeChange={onWorkspaceModeChange}
          modeChanging={modeChanging}
          modeError={modeError}
          conversationId={conversationId}
          onSend={onSend}
          isLoading={isLoading}
          onStop={onStop}
          suggestedText={suggestedText}
          onTextUsed={onTextUsed}
          restoredDraft={restoredDraft}
          disabled={inputDisabled}
          chatVoiceControls={chatVoiceControls}
          onRequestGlobalVoiceSession={onRequestGlobalVoiceSession}
        />

        <div className="home-quick-actions" aria-label="快捷任务" role="group">
          {QUICK_ACTIONS.map((action) => {
            const Icon = action.icon;
            return (
              <button
                key={action.title}
                type="button"
                onClick={() => onHint?.(action.prompt)}
                className="home-quick-action"
              >
                <Icon size={15} aria-hidden="true" />
                {action.title}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
