import { X } from "lucide-react";
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
}

export default function ExecutionSidebar({
  state,
  pendingApprovals,
  resolving,
  onApprove,
  onReject,
  onClose,
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
    <aside className="flex h-full min-h-0 flex-col bg-[var(--bg-2)] text-[var(--text)]">
      <header className="flex h-14 shrink-0 items-center justify-between border-b border-[var(--border)] px-4">
        <div>
          <h2 className="text-sm font-semibold">执行轨迹</h2>
          <p className="mt-0.5 text-[10px] text-[var(--text-muted)]">
            {history.length} 个动作 · {completed} 个完成
            {failures > 0 ? ` · ${failures} 个异常` : ""}
          </p>
        </div>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-2 text-[var(--text-muted)] transition-colors hover:bg-[var(--panel-hover)] hover:text-[var(--text)]"
            aria-label="关闭执行轨迹"
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </header>

      <div className="scrollbar-thin min-h-0 flex-1 overflow-y-auto px-3 py-4">
        <section aria-labelledby="current-action-heading">
          <h3
            id="current-action-heading"
            className="mb-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--text-muted)]"
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
            className="mb-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--text-muted)]"
          >
            历史记录
          </h3>
          <ExecutionHistory records={history} />
        </section>
      </div>
    </aside>
  );
}
