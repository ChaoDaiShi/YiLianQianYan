import { PanelRightClose, X } from "lucide-react";
import type { PendingApproval } from "../../types/approval";
import CurrentActionCard from "./CurrentActionCard";
import ExecutionHistory from "./ExecutionHistory";
import type { AgentRunState } from "./model";
import { selectCurrentAction, selectExecutionHistory } from "./selectors";

interface ExecutionSidebarProps {
  state: AgentRunState;
  pendingApprovals: PendingApproval[];
  resolving: Record<string, boolean>;
  onApprove: (approval: PendingApproval) => void;
  onReject: (approval: PendingApproval) => void;
  onClose?: () => void;
  onToggleCollapse?: () => void;
}

export default function ExecutionSidebar({
  state,
  pendingApprovals,
  resolving,
  onApprove,
  onReject,
  onClose,
  onToggleCollapse,
}: ExecutionSidebarProps) {
  const history = selectExecutionHistory(state);
  const current = selectCurrentAction(history);
  const currentApproval = current
    ? pendingApprovals.find(
        (approval) =>
          approval.tool_call_id === current.toolCallId ||
          approval.approval_id === current.approvalId
      )
    : undefined;
  const completed = history.filter(
    (record) =>
      record.executionStatus === "succeeded" ||
      record.executionStatus === "rejected" ||
      record.executionStatus === "cancelled"
  ).length;
  const failures = history.filter(
    (record) =>
      record.executionStatus === "failed" ||
      record.verificationStatus === "failed"
  ).length;

  return (
    <aside className="glass-rail execution-sidebar flex h-full min-h-0 flex-col bg-[var(--surface-muted)] text-[var(--text)]">
      <header className="execution-header flex h-14 shrink-0 items-center justify-between border-b border-[var(--border)] px-4">
        <div>
          <h2 className="text-sm font-semibold">执行轨迹</h2>
          <span className="execution-header-motif" aria-hidden="true" />
          <p className="mt-0.5 text-caption text-[var(--text-muted)]">
            {history.length} 个动作 · {completed} 个完成
            {failures > 0 ? ` · ${failures} 个异常` : ""}
          </p>
        </div>
        <div className="flex items-center gap-1">
          {onToggleCollapse && (
            <button
              type="button"
              onClick={onToggleCollapse}
              className="rounded-md p-2 text-[var(--text-muted)] transition-colors hover:bg-[var(--panel-hover)] hover:text-[var(--text-primary)]"
              aria-label="收起执行轨迹"
              title="收起执行轨迹"
            >
              <PanelRightClose className="h-4 w-4" />
            </button>
          )}
          {onClose && (
            <button
              type="button"
              onClick={onClose}
              className="rounded-md p-2 text-[var(--text-muted)] transition-colors hover:bg-[var(--panel-hover)] hover:text-[var(--text-primary)]"
              aria-label="关闭执行轨迹"
            >
              <X className="h-4 w-4" />
            </button>
          )}
        </div>
      </header>

      <div className="execution-sidebar-content scrollbar-thin min-h-0 flex-1 overflow-y-auto px-3 py-4">
        <section aria-labelledby="current-action-heading">
          <h3
            id="current-action-heading"
            className="mb-2 text-meta font-semibold uppercase tracking-[0.12em] text-[var(--text-muted)]"
          >
            当前动作
          </h3>
          <CurrentActionCard
            record={current}
            connection={state.connection}
            approval={currentApproval}
            resolving={
              currentApproval ? resolving[currentApproval.approval_id] : false
            }
            onApprove={onApprove}
            onReject={onReject}
          />
        </section>

        <section className="mt-5" aria-labelledby="execution-history-heading">
          <h3
            id="execution-history-heading"
            className="mb-2 text-meta font-semibold uppercase tracking-[0.12em] text-[var(--text-muted)]"
          >
            历史记录
          </h3>
          <ExecutionHistory records={history} />
        </section>
      </div>
    </aside>
  );
}
