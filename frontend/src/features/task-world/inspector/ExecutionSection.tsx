import { useState } from "react";
import { Button } from "../../../components/ui";
import type { TaskNodeExecutionSummary } from "../../../api/taskWorld";
import Identifier from "../../../components/Identifier";
import TaskExecutionDetails from "./TaskExecutionDetails";
import {
  executionStatusLabel,
  getExecutorAvailability,
  isActiveExecution,
  type TaskNodeProjection,
} from "../taskGraphProjection";

interface ExecutionSectionProps {
  graphId: string;
  node: TaskNodeProjection;
  saving: boolean;
  graphLocked: boolean;
  blockedReason?: string;
  onStartExecution?: () => Promise<void>;
  onCancelExecution?: () => Promise<void>;
  onRerun?: () => Promise<void>;
}

/**
 * The Task Harness execution controls and attempt history.
 *
 * Starting an execution is gated on a *configured* executor; an unconfigured
 * node keeps its semantic editing but cannot be run.
 */
export default function ExecutionSection({
  graphId,
  node,
  saving,
  graphLocked,
  onStartExecution,
  onCancelExecution,
  onRerun,
  blockedReason,
}: ExecutionSectionProps) {
  const [viewedExecution, setViewedExecution] = useState<string | null>(null);
  const latestExecution = node.latest_execution;
  const execution_status = latestExecution?.status;
  const executionBusy = isActiveExecution(execution_status);
  const executionHistory = node.execution_history || [];
  const selectedExecution = latestExecution?.execution_id === viewedExecution ? latestExecution :
    executionHistory.find(execution => execution.execution_id === viewedExecution);
  function openDetails(execution: TaskNodeExecutionSummary) { setViewedExecution(execution.execution_id); }

  return (
    <section
      className="mt-5 border-t border-[var(--border-soft)] pt-4"
      aria-labelledby="task-world-execution-heading"
      data-testid="task-execution-state"
    >
      <div className="flex items-center justify-between gap-2">
        <h3
          id="task-world-execution-heading"
          className="text-xs font-semibold text-[var(--text)]"
        >
          Task Harness 执行
        </h3>
        {execution_status && (
          <span className="text-meta text-[var(--text-secondary)]">
            {executionStatusLabel(execution_status)}
          </span>
        )}
      </div>
      {latestExecution ? (
        <dl className="mt-2 grid grid-cols-2 gap-2 text-xs">
          <div className="col-span-2 rounded-[var(--radius-sm)] bg-[var(--surface-muted)] p-2">
            <dt className="text-[var(--text-faint)]">执行编号</dt>
            <dd className="mt-1"><Identifier kind="execution" value={latestExecution.execution_id} /></dd>
          </div>
          <div className="rounded-[var(--radius-sm)] bg-[var(--surface-muted)] p-2">
            <dt className="text-[var(--text-faint)]">执行器</dt>
            <dd className="mt-1 break-all text-[var(--text-secondary)]">
              {latestExecution.executor_ref || "未配置"}
            </dd>
          </div>
          <div className="rounded-[var(--radius-sm)] bg-[var(--surface-muted)] p-2">
            <dt className="text-[var(--text-faint)]">校验</dt>
            <dd className="mt-1 text-[var(--text-secondary)]">
              {latestExecution.validation?.status || "待校验"}
            </dd>
          </div>
        </dl>
      ) : (
        <p className="mt-2 text-xs text-[var(--text-faint)]">
          尚无执行尝试。开始后，执行、校验与结果会在此处保留。
        </p>
      )}
      {latestExecution?.result_summary && (
        <p className="mt-2 rounded-[var(--radius-sm)] bg-[var(--success-soft)] px-2 py-1.5 text-xs text-[var(--success-fg)]">
          最新结果：{latestExecution.result_summary}
        </p>
      )}
      {latestExecution?.failure_code && (
        <p className="mt-2 rounded-[var(--radius-sm)] bg-[var(--danger-soft)] px-2 py-1.5 text-xs text-[var(--danger-fg)]">
          失败原因：{latestExecution.failure_code}
          {latestExecution.error ? ` · ${latestExecution.error}` : ""}
        </p>
      )}
      {blockedReason && <p className="mt-2 text-xs text-[var(--text-secondary)]" role="status">{blockedReason}</p>}
      <div className="mt-3 flex flex-wrap gap-2">
        {latestExecution && <Button type="button" size="sm" variant="secondary" onClick={() => openDetails(latestExecution)}>查看执行详情</Button>}
        {onStartExecution && !executionBusy && (
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={saving || graphLocked || Boolean(blockedReason) || getExecutorAvailability(node.executor_ref,node.capability).kind !== "configured"}
            onClick={() => void onStartExecution()}
          >
            {latestExecution ? "再次执行" : "开始执行"}
          </Button>
        )}
        {onCancelExecution && executionBusy && (
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={saving}
            onClick={() => void onCancelExecution()}
          >
            取消执行
          </Button>
        )}
        {onRerun && !executionBusy && (latestExecution || node.status === "invalidated") && (
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={saving || graphLocked}
            onClick={() => void onRerun()}
          >
            {node.status === "invalidated" ? "校验并准备重跑" : "从此节点重跑"}
          </Button>
        )}
      </div>
      {node.status === "invalidated" && (
        <p className="mt-2 text-xs text-[var(--text-faint)]">
          语义修改使原结果失效。校验并准备重跑后可开始新尝试；历史记录保留，不会撤销已发生的外部操作。
        </p>
      )}
      {executionHistory.length > 0 && (
        <ol className="mt-3 space-y-1.5" aria-label="执行尝试历史">
          {executionHistory.map((execution) => (
            <li
              key={execution.execution_id}
              className="flex flex-wrap items-center justify-between gap-2 rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-2 py-1.5 text-xs"
            >
              <span className="text-[var(--text-secondary)]">
                #{execution.attempt} · {executionStatusLabel(execution.status)}
                <Identifier kind="execution" value={execution.execution_id} className="ml-1" />
              </span>
              <Button size="sm" variant="ghost" type="button" onClick={() => openDetails(execution)} aria-label={`查看第 ${execution.attempt} 次执行详情`}>详情</Button>
            </li>
          ))}
        </ol>
      )}
      {selectedExecution && <TaskExecutionDetails key={selectedExecution.execution_id} graphId={graphId} nodeId={node.id} execution={selectedExecution} onClose={() => setViewedExecution(null)} />}
    </section>
  );
}
