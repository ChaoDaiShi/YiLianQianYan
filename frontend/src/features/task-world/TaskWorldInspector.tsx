import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import type {
  ApiError,
  TaskCheckpointSummary,
  TaskEdge,
  TaskNodeKind,
  TaskNodeExecutionSummary,
  TaskRevisionSummary,
} from "../../api/taskWorld";
import {
  executionStatusLabel,
  getExecutorAvailability,
  isActiveExecution,
  type TaskNodeProjection,
} from "./taskGraphProjection";
import { Badge, Button, EmptyState, Input, Panel, Textarea } from "../../components/ui";

interface TaskNodeDraft {
  kind: TaskNodeKind;
  title: string;
  input: Record<string, unknown>;
  retry_policy: { max_attempts: number };
}

interface TaskWorldInspectorProps {
  node: TaskNodeProjection | null;
  expectedRevision: number;
  revisions: TaskRevisionSummary[];
  checkpoints: TaskCheckpointSummary[];
  dependencyEdges: TaskEdge[];
  dependencyCandidates: TaskNodeProjection[];
  saving?: boolean;
  saveError?: ApiError | null;
  onSave: (draft: TaskNodeDraft) => Promise<void>;
  onStart: () => Promise<void>;
  onStartExecution?: () => Promise<void>;
  onCancelExecution?: () => Promise<void>;
  onRerun?: () => Promise<void>;
  onCheckpoint: () => Promise<void>;
  onRestore: (checkpointId: string) => Promise<void>;
  onAddDependency: (from: string, to: string) => Promise<void>;
  onRemoveDependency: (from: string, to: string) => Promise<void>;
  graphLocked: boolean;
}

export default function TaskWorldInspector({
  node,
  expectedRevision,
  revisions,
  checkpoints,
  dependencyEdges,
  dependencyCandidates,
  saving = false,
  saveError = null,
  onSave,
  onStart,
  onStartExecution,
  onCancelExecution,
  onRerun,
  onCheckpoint,
  onRestore,
  onAddDependency,
  onRemoveDependency,
  graphLocked,
}: TaskWorldInspectorProps) {
  const [title, setTitle] = useState("");
  const [instruction, setInstruction] = useState("");
  const [executorRef, setExecutorRef] = useState("");
  const [acceptanceCriteria, setAcceptanceCriteria] = useState("");
  const [dependencyTarget, setDependencyTarget] = useState("");
  const [restoringCheckpoint, setRestoringCheckpoint] = useState("");

  useEffect(() => {
    setTitle(node?.title || "");
    setInstruction(node?.instruction_summary || "");
    setExecutorRef(node?.executor_ref || "");
    setAcceptanceCriteria(node?.acceptance_criteria.join("\n") || "");
    setDependencyTarget("");
  }, [node?.id, node?.title, node?.instruction_summary, node?.executor_ref, node?.acceptance_criteria.join("\n")]);

  const availability = useMemo(
    () => getExecutorAvailability(executorRef.trim() || null),
    [executorRef],
  );
  const incomingDependencies = dependencyEdges.filter((edge) => edge.to === node?.id);
  const stale =
    saveError?.code === "stale_revision" || saveError?.code === "stale_view_revision";

  if (!node) {
    return (
      <Panel className="task-world-inspector h-full min-h-0 overflow-y-auto">
        <EmptyState
          title="选择一个节点"
          description="从画布或执行轨迹选择节点，查看状态并编辑任务语义。"
          className="py-16"
        />
      </Panel>
    );
  }

  const latestExecution = node.latest_execution;
  const execution_status = latestExecution?.status;
  const executionBusy = isActiveExecution(execution_status);
  const executionHistory: TaskNodeExecutionSummary[] = node.execution_history || [];

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const input: Record<string, unknown> = {
      instruction: instruction.trim(),
      acceptance_criteria: acceptanceCriteria
        .split("\n")
        .map((value) => value.trim())
        .filter(Boolean),
    };
    if (executorRef.trim()) input.executor_ref = executorRef.trim();
    await onSave({
      kind: node.kind,
      title: title.trim(),
      input,
      retry_policy: { max_attempts: 1 },
    });
  };

  const addDependency = async () => {
    if (!dependencyTarget || dependencyTarget === node.id) return;
    await onAddDependency(dependencyTarget, node.id);
    setDependencyTarget("");
  };

  return (
    <Panel className="task-world-inspector h-full min-h-0 overflow-y-auto" data-testid="task-world-inspector">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-[var(--text-muted)]">
            Inspector
          </p>
          <h2 className="mt-1 truncate text-base font-semibold text-[var(--text)]">{node.title}</h2>
        </div>
        <Badge tone={execution_status === "failed" || node.status === "failed" ? "danger" : node.isRunning ? "info" : "default"}>
          {execution_status ? executionStatusLabel(execution_status) : node.status}
        </Badge>
      </div>

      <dl className="mt-4 grid grid-cols-2 gap-2 text-xs">
        <div className="rounded-[var(--radius-md)] bg-[var(--surface-muted)] p-2">
          <dt className="text-[var(--text-faint)]">当前图版本</dt>
          <dd className="mt-1 font-mono text-[var(--text)]">r{expectedRevision}</dd>
        </div>
        <div className="rounded-[var(--radius-md)] bg-[var(--surface-muted)] p-2">
          <dt className="text-[var(--text-faint)]">尝试次数</dt>
          <dd className="mt-1 font-mono text-[var(--text)]">{latestExecution?.attempt ?? node.state.attempts}</dd>
        </div>
      </dl>

      {stale && (
        <p className="mt-3 rounded-[var(--radius-md)] border border-[var(--warning-border)] bg-[var(--warning-soft)] px-3 py-2 text-xs text-[var(--warning-fg)]" role="alert">
          图已被其他窗口更新，请先刷新后再保存；本次修改未覆盖最新语义。
        </p>
      )}
      {saveError && !stale && (
        <p className="mt-3 rounded-[var(--radius-md)] border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 py-2 text-xs text-[var(--danger-fg)]" role="alert">
          {saveError.message}
        </p>
      )}

      <form className="mt-4 space-y-3" onSubmit={(event) => void submit(event)}>
        <fieldset disabled={graphLocked || saving} className="space-y-3">
        <Input label="标题" value={title} onChange={(event) => setTitle(event.target.value)} required maxLength={200} />
        <Textarea
          label="任务说明"
          value={instruction}
          onChange={(event) => setInstruction(event.target.value)}
          rows={4}
        />
        <Input
          label="执行引用"
          value={executorRef}
          onChange={(event) => setExecutorRef(event.target.value)}
          placeholder="例如 skill://research 或 command://…"
        />
        <div className="rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-muted)] px-3 py-2 text-xs">
          <span className="text-[var(--text-faint)]">执行能力</span>
          <p className={availability.kind === "unavailable" ? "mt-1 text-[var(--warning-fg)]" : "mt-1 text-[var(--text-secondary)]"}>
            {availability.label}
          </p>
          <p className="mt-1 break-words text-[var(--text-faint)]">{availability.reason}</p>
        </div>
        <Textarea
          label="验收标准（每行一条）"
          value={acceptanceCriteria}
          onChange={(event) => setAcceptanceCriteria(event.target.value)}
          rows={4}
        />
        <Button type="submit" size="sm" disabled={saving || !title.trim()}>
          {saving ? "保存中…" : "保存语义"}
        </Button>
        </fieldset>
      </form>
      {graphLocked && <p className="mt-2 text-xs text-[var(--text-faint)]">有任务执行尚未结束，请先完成或取消执行后再修改任务图。</p>}

      <section className="mt-5 border-t border-[var(--border-soft)] pt-4" aria-labelledby="task-world-state-heading">
        <h3 id="task-world-state-heading" className="text-xs font-semibold text-[var(--text)]">状态与结果</h3>
        <p className="mt-2 text-xs leading-5 text-[var(--text-secondary)]">{node.instruction_summary}</p>
        {node.result_summary && (
          <p className="mt-2 rounded-[var(--radius-md)] bg-[var(--success-soft)] px-3 py-2 text-xs text-[var(--success-fg)]">
            结果：{node.result_summary}
          </p>
        )}
        {node.state.error && (
          <p className="mt-2 rounded-[var(--radius-md)] bg-[var(--danger-soft)] px-3 py-2 text-xs text-[var(--danger-fg)]">
            错误：{node.state.error}
          </p>
        )}
        {node.resources.length > 0 && (
          <div className="mt-3">
            <p className="text-[11px] text-[var(--text-faint)]">资源引用</p>
            <ul className="mt-1 space-y-1 text-xs text-[var(--text-secondary)]">
              {node.resources.map((resource) => (
                <li key={resource.id} className="truncate">{resource.name || resource.id}</li>
              ))}
            </ul>
          </div>
        )}
      </section>

      <section className="mt-5 border-t border-[var(--border-soft)] pt-4" aria-labelledby="task-world-execution-heading" data-testid="task-execution-state">
        <div className="flex items-center justify-between gap-2">
          <h3 id="task-world-execution-heading" className="text-xs font-semibold text-[var(--text)]">Task Harness 执行</h3>
          {execution_status && <span className="text-[11px] text-[var(--text-secondary)]">{executionStatusLabel(execution_status)}</span>}
        </div>
        {latestExecution ? (
          <dl className="mt-2 grid grid-cols-2 gap-2 text-xs">
            <div className="rounded-[var(--radius-sm)] bg-[var(--surface-muted)] p-2">
              <dt className="text-[var(--text-faint)]">执行器</dt>
              <dd className="mt-1 break-all text-[var(--text-secondary)]">{latestExecution.executor_ref || "未配置"}</dd>
            </div>
            <div className="rounded-[var(--radius-sm)] bg-[var(--surface-muted)] p-2">
              <dt className="text-[var(--text-faint)]">校验</dt>
              <dd className="mt-1 text-[var(--text-secondary)]">{latestExecution.validation?.status || "待校验"}</dd>
            </div>
          </dl>
        ) : (
          <p className="mt-2 text-xs text-[var(--text-faint)]">尚无执行尝试。开始后，执行、校验与结果会在此处保留。</p>
        )}
        {latestExecution?.result_summary && (
          <p className="mt-2 rounded-[var(--radius-sm)] bg-[var(--success-soft)] px-2 py-1.5 text-xs text-[var(--success-fg)]">最新结果：{latestExecution.result_summary}</p>
        )}
        {latestExecution?.failure_code && (
          <p className="mt-2 rounded-[var(--radius-sm)] bg-[var(--danger-soft)] px-2 py-1.5 text-xs text-[var(--danger-fg)]">失败原因：{latestExecution.failure_code}{latestExecution.error ? ` · ${latestExecution.error}` : ""}</p>
        )}
        <div className="mt-3 flex flex-wrap gap-2">
          {onStartExecution && !executionBusy && (
            <Button type="button" size="sm" variant="secondary" disabled={saving || graphLocked || availability.kind === "unavailable"} onClick={() => void onStartExecution()}>
              {latestExecution ? "再次执行" : "开始执行"}
            </Button>
          )}
          {onCancelExecution && executionBusy && (
            <Button type="button" size="sm" variant="ghost" disabled={saving} onClick={() => void onCancelExecution()}>
              取消执行
            </Button>
          )}
          {onRerun && !executionBusy && latestExecution && (
            <Button type="button" size="sm" variant="ghost" disabled={saving || graphLocked} onClick={() => void onRerun()}>
              从此节点重跑
            </Button>
          )}
        </div>
        {executionHistory.length > 0 && (
          <ol className="mt-3 space-y-1.5" aria-label="执行尝试历史">
            {executionHistory.map((execution) => (
              <li key={execution.execution_id} className="flex items-center justify-between gap-2 rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-2 py-1.5 text-xs">
                <span className="text-[var(--text-secondary)]">#{execution.attempt} · {executionStatusLabel(execution.status)}</span>
                <span className="truncate text-[var(--text-faint)]">{execution.failure_code || execution.result_summary || execution.executor_ref || "—"}</span>
              </li>
            ))}
          </ol>
        )}
      </section>

      {node.kind === "approval" && (
        <section className="mt-5 rounded-[var(--radius-md)] border border-[var(--warning-border)] bg-[var(--warning-soft)] p-3 text-xs">
          <p className="font-medium text-[var(--warning-fg)]">等待审批</p>
          <p className="mt-1 leading-5 text-[var(--text-secondary)]">审批动作沿用对话中的既有审批入口，不在 Task World 内创建第二套审批状态。</p>
          <Link className="mt-2 inline-flex text-[var(--accent-primary)] hover:underline" to="/chat">
            打开既有审批入口
          </Link>
        </section>
      )}

      <section className="mt-5 border-t border-[var(--border-soft)] pt-4" aria-labelledby="task-world-dependencies-heading">
        <div className="flex items-center justify-between gap-2">
          <h3 id="task-world-dependencies-heading" className="text-xs font-semibold text-[var(--text)]">依赖</h3>
          <span className="text-[11px] text-[var(--text-faint)]">{incomingDependencies.length} 条入边</span>
        </div>
        {incomingDependencies.length > 0 && (
          <ul className="mt-2 space-y-1">
            {incomingDependencies.map((edge) => (
              <li key={`${edge.from}->${edge.to}`} className="flex items-center justify-between gap-2 rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-2 py-1.5 text-xs">
                <span className="truncate text-[var(--text-secondary)]">{edge.from}</span>
                <button
                  type="button"
                  className="shrink-0 text-[var(--text-faint)] hover:text-[var(--danger)]"
                  disabled={saving || graphLocked}
                  onClick={() => void onRemoveDependency(edge.from, edge.to)}
                  aria-label={`删除依赖 ${edge.from}`}
                >
                  移除
                </button>
              </li>
            ))}
          </ul>
        )}
        <div className="mt-2 flex gap-2">
          <select
            className="min-w-0 flex-1 rounded-[var(--radius-sm)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-2 py-1.5 text-xs text-[var(--text)]"
            aria-label="选择依赖节点"
            value={dependencyTarget}
            onChange={(event) => setDependencyTarget(event.target.value)}
          >
            <option value="">添加前置节点</option>
            {dependencyCandidates
              .filter((candidate) => candidate.id !== node.id && !incomingDependencies.some((edge) => edge.from === candidate.id))
              .map((candidate) => <option key={candidate.id} value={candidate.id}>{candidate.title}</option>)}
          </select>
          <Button type="button" size="sm" variant="secondary" disabled={!dependencyTarget || saving || graphLocked} onClick={() => void addDependency()}>
            添加
          </Button>
        </div>
      </section>

      <section className="mt-5 border-t border-[var(--border-soft)] pt-4" aria-labelledby="task-world-history-heading">
        <div className="flex items-center justify-between gap-2">
          <h3 id="task-world-history-heading" className="text-xs font-semibold text-[var(--text)]">版本与检查点</h3>
          <Button type="button" size="sm" variant="ghost" disabled={saving || graphLocked} onClick={() => void onCheckpoint()}>
            建立检查点
          </Button>
        </div>
        <ul className="mt-2 space-y-1.5 text-xs">
          {revisions.slice(-4).reverse().map((revision) => (
            <li key={`${revision.graph_id}-${revision.revision}`} className="rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-2 py-1.5">
              <span className="font-mono text-[var(--text)]">r{revision.revision}</span>
              <span className="ml-2 text-[var(--text-secondary)]">{revision.change}</span>
            </li>
          ))}
        </ul>
        {checkpoints.length > 0 && (
          <div className="mt-3 flex gap-2">
            <select
              className="min-w-0 flex-1 rounded-[var(--radius-sm)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-2 py-1.5 text-xs text-[var(--text)]"
              aria-label="选择检查点"
              value={restoringCheckpoint}
              onChange={(event) => setRestoringCheckpoint(event.target.value)}
            >
              <option value="">选择检查点恢复</option>
              {checkpoints.map((checkpoint) => (
                <option key={checkpoint.checkpoint_id} value={checkpoint.checkpoint_id}>
                  r{checkpoint.graph_revision} · {checkpoint.checkpoint_id.slice(0, 8)}
                </option>
              ))}
            </select>
            <Button type="button" size="sm" variant="secondary" disabled={!restoringCheckpoint || saving || graphLocked} onClick={() => void onRestore(restoringCheckpoint)}>
              恢复
            </Button>
          </div>
        )}
        {node.status === "runnable" && (
          <Button type="button" size="sm" variant="ghost" className="mt-3" disabled={saving || graphLocked} onClick={() => void onStart()}>
            标记为运行中
          </Button>
        )}
      </section>
    </Panel>
  );
}
