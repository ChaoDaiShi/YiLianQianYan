import { useState, type RefObject } from "react";
import {
  ChevronDown,
  ListTree,
  Package,
  PanelLeft,
  PanelRight,
  Workflow as WorkflowIcon,
} from "lucide-react";
import type { Workflow } from "../../api/client";
import type { AgentRunState } from "../../features/execution/model";
import { Badge } from "../ui";

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
}: ChatHeaderProps) {
  const [workflowMenuOpen, setWorkflowMenuOpen] = useState(false);
  const connectionMeta = CONNECTION_META[connection];
  const statusMeta = finished
    ? { label: "已经处理好了", color: "bg-[var(--success)]" }
    : connectionMeta;

  const selectWorkflow = (workflow: Workflow | null) => {
    onSelectWorkflow(workflow);
    setWorkflowMenuOpen(false);
  };

  return (
    <header className="glass-header relative z-30 flex h-14 shrink-0 items-center gap-2 border-b border-[var(--border)] bg-[var(--titlebar-bg)] px-3 min-[960px]:px-4">
      {showConversationToggle && (
        <button
          ref={conversationToggleRef}
          type="button"
          onClick={onToggleConversations}
          className="rounded-lg p-2 text-[var(--text-muted)] transition-colors hover:bg-[var(--panel-hover)] hover:text-[var(--text)]"
          aria-label="打开任务列表"
          title="打开任务列表"
        >
          <PanelLeft className="h-4 w-4" />
        </button>
      )}

      <div className="min-w-0">
        <h1 className="truncate text-sm font-semibold min-[960px]:text-base">
          {title}
        </h1>
      </div>

      <div className="relative ml-1">
        <button
          type="button"
          onClick={() => setWorkflowMenuOpen((open) => !open)}
          className="flex max-w-[150px] items-center gap-1.5 rounded-lg px-2 py-1.5 text-xs text-[var(--text-muted)] transition-colors hover:bg-[var(--panel-hover)] hover:text-[var(--text)]"
          aria-expanded={workflowMenuOpen}
          title="切换工作流"
        >
          <WorkflowIcon className="h-3.5 w-3.5 shrink-0 text-[var(--accent)]" />
          <span className="truncate">
            {activeWorkflow?.name || "无工作流"}
          </span>
          <ChevronDown className="h-3.5 w-3.5 shrink-0" />
        </button>

        {workflowMenuOpen && (
          <>
            <button
              type="button"
              className="fixed inset-0 z-40 cursor-default"
              onClick={() => setWorkflowMenuOpen(false)}
              aria-label="关闭工作流菜单"
            />
            <div className="scrollbar-thin absolute left-0 top-full z-50 mt-1 max-h-80 w-64 overflow-y-auto rounded-xl border border-[var(--border)] bg-[var(--panel)] py-1 shadow-2xl">
              <button
                type="button"
                onClick={() => selectWorkflow(null)}
                className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-[var(--text-muted)] transition-colors hover:bg-[var(--panel-hover)]"
              >
                <ListTree className="h-4 w-4" />
                不使用工作流
              </button>
              {workflows.map((workflow) => (
                <button
                  type="button"
                  key={workflow.id}
                  onClick={() => selectWorkflow(workflow)}
                  className={`flex w-full items-center gap-2 px-3 py-2 text-left text-sm transition-colors hover:bg-[var(--panel-hover)] ${
                    activeWorkflow?.id === workflow.id
                      ? "text-[var(--accent)]"
                      : "text-[var(--text-muted)]"
                  }`}
                >
                  {workflow.is_builtin ? (
                    <Package className="h-4 w-4 shrink-0" />
                  ) : (
                    <WorkflowIcon className="h-4 w-4 shrink-0" />
                  )}
                  <span className="truncate">{workflow.name}</span>
                  {workflow.nodes.length > 0 && (
                    <span className="ml-auto shrink-0 text-[10px] text-[var(--text-faint)]">
                      {workflow.nodes.length} 步
                    </span>
                  )}
                </button>
              ))}
            </div>
          </>
        )}
      </div>

      <div className="ml-auto flex items-center gap-2">
        <div
          className="hidden items-center gap-1.5 text-[11px] text-[var(--text-muted)] min-[720px]:flex"
          aria-label={`Agent 状态：${statusMeta.label}`}
        >
          <span className={`h-2 w-2 rounded-full ${statusMeta.color}`} />
          <span>{statusMeta.label}</span>
        </div>
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
              <Badge tone="accent" className="px-1.5 py-0 text-[10px]">
                {executionCount}
              </Badge>
            )}
          </button>
        )}
      </div>
    </header>
  );
}
