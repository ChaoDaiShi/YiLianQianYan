import { Link, useParams } from "react-router-dom";
import type { ReactNode, RefObject } from "react";
import {
  ChevronDown,
  GitBranch,
  ListTree,
  Package,
  PanelLeft,
  PanelRight,
  Workflow as WorkflowIcon,
} from "lucide-react";
import type { Workflow } from "../../api/client";
import type { AgentRunState } from "../../features/execution/model";
import { Badge } from "../ui";
import { ComposerPopover } from "./ComposerPopover";
import { ComposerOption } from "./ComposerOption";

interface ChatHeaderProps {
  title: string;
  connection: AgentRunState["connection"];
  activeWorkflow: Workflow | null;
  workflows: Workflow[];
  executionCount: number;
  showConversationToggle: boolean;
  showExecutionToggle: boolean;
  conversationToggleRef?: RefObject<HTMLButtonElement>;
  executionToggleRef?: RefObject<HTMLButtonElement>;
  onToggleConversations: () => void;
  onToggleExecution: () => void;
  onSelectWorkflow: (workflow: Workflow | null) => void;
  finished?: boolean;
  panelControls?: ReactNode;
  home?: boolean;
  showCanvasLink?: boolean;
}

const CONNECTION_META: Record<
  AgentRunState["connection"],
  { label: string; color: string }
> = {
  idle: { label: "未连接", color: "bg-[var(--text-faint)]" },
  connecting: { label: "连接中", color: "bg-[var(--warning)]" },
  connected: { label: "已连接", color: "bg-[var(--success)]" },
  interrupted: { label: "已中断", color: "bg-[var(--warning)]" },
  error: { label: "连接失败", color: "bg-[var(--danger)]" },
};

export default function ChatHeader({
  title,
  connection,
  activeWorkflow,
  workflows,
  executionCount,
  showConversationToggle,
  showExecutionToggle,
  conversationToggleRef,
  executionToggleRef,
  onToggleConversations,
  onToggleExecution,
  onSelectWorkflow,
  finished = false,
  panelControls,
  home = false,
  showCanvasLink = true,
}: ChatHeaderProps) {
  const {id: conversationId} = useParams();
  const connectionMeta = CONNECTION_META[connection];
  const connectionNeedsAttention = connection === "error" || connection === "interrupted" || connection === "connecting";
  const statusMeta = finished && !connectionNeedsAttention
    ? { label: "已经处理好了", color: "bg-[var(--success)]" }
    : connectionMeta;

  return (
    <header className="glass-header chat-shell-header relative z-30 flex h-14 min-w-0 shrink-0 items-center gap-2 border-b border-[var(--border)] bg-[var(--titlebar-bg)] px-3 min-[960px]:px-4">
      {showConversationToggle && (
        <button
          ref={conversationToggleRef}
          type="button"
          onClick={onToggleConversations}
          className="shrink-0 rounded-lg p-2 text-[var(--text-muted)] transition-colors hover:bg-[var(--panel-hover)] hover:text-[var(--text)]"
          aria-label="打开任务列表"
          title="打开任务列表"
        >
          <PanelLeft className="h-4 w-4" />
        </button>
      )}

      <div className="titlebar-brand min-w-0">
        <h1 className="truncate text-sm font-semibold min-[960px]:text-base">
          {title}
        </h1>
      </div>

      <ComposerPopover label="切换工作流" className="chat-header-workflow"
        trigger={<><WorkflowIcon size={14} aria-hidden="true" /><span className="chat-header-action-label truncate">{activeWorkflow?.name || "无工作流"}</span><ChevronDown className="chat-header-action-label" size={13} aria-hidden="true" /></>}>
        {close => <div className="composer-menu-options" role="group" aria-label="可用工作流">
          <ComposerOption icon={ListTree} label="不使用工作流" description="直接在当前对话中执行" selected={!activeWorkflow} onClick={() => { onSelectWorkflow(null); close(); }} />
          {workflows.map(workflow => <ComposerOption key={workflow.id} icon={workflow.is_builtin ? Package : WorkflowIcon}
            label={workflow.name} description={workflow.nodes.length ? `${workflow.nodes.length} 个步骤` : "对话工作流"}
            selected={activeWorkflow?.id === workflow.id} onClick={() => { onSelectWorkflow(workflow); close(); }} />)}
        </div>}
      </ComposerPopover>

      <div className="chat-header-actions ml-auto flex shrink-0 items-center gap-2">
        {panelControls}
        {conversationId && showCanvasLink && <Link className="chat-header-canvas inline-flex items-center gap-1 text-xs text-[var(--text-muted)] hover:text-[var(--text)]" aria-label="在画布查看" title="在画布查看" to={`/connectors?conversation=${encodeURIComponent(conversationId)}`}><GitBranch size={14} aria-hidden="true" /><span className="chat-header-action-label">在画布查看</span></Link>}
        {(!home || connectionNeedsAttention) && (
          <div
            className={`chat-header-status items-center gap-1.5 whitespace-nowrap text-meta text-[var(--text-muted)] ${connectionNeedsAttention ? "flex" : "hidden min-[720px]:flex"}`}
            aria-label={`Agent 状态：${statusMeta.label}`}
            title={statusMeta.label}
          >
            <span className={`h-2 w-2 rounded-full ${statusMeta.color}`} />
            <span className="chat-header-action-label">{statusMeta.label}</span>
          </div>
        )}
        {showExecutionToggle && (
          <button
            ref={executionToggleRef}
            type="button"
            onClick={onToggleExecution}
            className="flex items-center gap-1.5 rounded-lg p-2 text-[var(--text-muted)] transition-colors hover:bg-[var(--panel-hover)] hover:text-[var(--text)]"
            aria-label="打开执行轨迹"
            title="打开执行轨迹"
          >
            <PanelRight className="h-4 w-4" />
            {executionCount > 0 && (
              <Badge tone="accent" className="px-1.5 py-0 text-caption">
                {executionCount}
              </Badge>
            )}
          </button>
        )}
      </div>
    </header>
  );
}
