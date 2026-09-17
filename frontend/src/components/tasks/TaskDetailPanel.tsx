import { useCallback, useEffect, useState } from "react";
import {
  CheckCircle2,
  Clock3,
  FileCode2,
  FileJson2,
  FileText,
  Image,
  Info,
  Play,
  RotateCcw,
  XCircle,
} from "lucide-react";
import {
  cancelTask,
  listTaskArtifacts,
  listTaskDecisions,
  listTaskExecutions,
  listTaskTimeline,
  retryTask,
  startTask,
  type Artifact,
  type Task,
  type TaskDecision,
  type TaskEvent,
  type TaskExecution,
} from "../../api/client";
import { deriveTaskDisplayTitle, looksLikeCommand } from "../chat/taskTitle";
import {
  Badge,
  Button,
  ErrorState,
  Panel,
  Skeleton,
} from "../ui";
import {
  formatTaskDate,
  getTaskStatusLabel,
  getTaskStatusTone,
  isTaskStartable,
} from "../../features/tasks/taskPresentation";
import {
  classifyArtifactType,
  type ArtifactDisplayKind,
} from "../../features/tasks/workspacePresentation";
import ResourceBindingPanel from "../../features/resources/ResourceBindingPanel";
import ArtifactResultsPanel from "../../features/tasks/ArtifactResultsPanel";
import MemorySkillCandidatePanel from "../../features/memory/MemorySkillCandidatePanel";

interface TaskDetailPanelProps {
  task: Task;
  workspaceName?: string;
  onOpenWorkspace?: () => void;
  onChanged?: () => void;
}

const TERMINAL_STATUSES = new Set(["completed", "failed", "cancelled"]);

function artifactIcon(kind: ArtifactDisplayKind) {
  if (kind === "markdown") return FileText;
  if (kind === "json") return FileJson2;
  if (kind === "code" || kind === "config") return FileCode2;
  if (kind === "image") return Image;
  return Info;
}

function formatSize(size: number | null | undefined): string | null {
  if (size === null || size === undefined) return null;
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

export default function TaskDetailPanel({
  task,
  workspaceName,
  onOpenWorkspace,
  onChanged,
}: TaskDetailPanelProps) {
  const [timeline, setTimeline] = useState<TaskEvent[] | null>(null);
  const [artifacts, setArtifacts] = useState<Artifact[] | null>(null);
  const [executions, setExecutions] = useState<TaskExecution[] | null>(null);
  const [decisions, setDecisions] = useState<TaskDecision[] | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionPending, setActionPending] = useState(false);

  const reloadDetail = useCallback(async () => {
    setTimeline(null);
    setArtifacts(null);
    setExecutions(null);
    setDecisions(null);
    const [timelineResult, artifactResult, executionResult, decisionResult] =
      await Promise.all([
        listTaskTimeline(task.id),
        listTaskArtifacts(task.id),
        listTaskExecutions(task.id),
        listTaskDecisions(task.id),
      ]);

    const failedResult = [timelineResult, artifactResult, executionResult].find(
      (result) => !result.ok
    );
    setDetailError(failedResult && !failedResult.ok ? failedResult.error : null);
    if (timelineResult.ok) setTimeline(timelineResult.data);
    if (artifactResult.ok) setArtifacts(artifactResult.data);
    if (executionResult.ok) setExecutions(executionResult.data);
    setDecisions(decisionResult.ok ? decisionResult.data : []);
  }, [task.id]);

  useEffect(() => {
    void reloadDetail();
  }, [reloadDetail]);

  const runAction = async (action: () => Promise<{ ok: boolean; error?: string }>) => {
    setActionPending(true);
    setActionError(null);
    const result = await action();
    setActionPending(false);
    if (!result.ok) {
      setActionError(result.error || "任务操作失败");
      return;
    }
    onChanged?.();
    void reloadDetail();
  };

  const isTerminal = TERMINAL_STATUSES.has(task.status);
  const canStart = isTaskStartable(task.status);
  const canRetry = task.status === "failed" || task.status === "cancelled";
  const displayTitle = deriveTaskDisplayTitle(task.title);
  const latestEvidenceExecution = executions?.find((execution) => execution.status === "completed");
  const evidenceSource = latestEvidenceExecution ? {
    kind: "task" as const,
    task_id: task.id,
    execution_id: latestEvidenceExecution.id,
  } : undefined;

  return (
    <div className="space-y-3">
      <Panel>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="min-w-0 text-base font-semibold text-[var(--text)]">
                {displayTitle}
              </h2>
              <Badge tone={getTaskStatusTone(task.status)}>
                {getTaskStatusLabel(task.status)}
              </Badge>
            </div>
            {task.description && (
              <p className="mt-2 whitespace-pre-wrap text-sm leading-6 text-[var(--text-secondary)]">
                {task.description}
              </p>
            )}
            {looksLikeCommand(task.title) && (
              <details className="mt-3 text-xs text-[var(--text-secondary)]">
                <summary className="cursor-pointer select-none hover:text-[var(--text-primary)]">
                  查看技术标题
                </summary>
                <code className="mt-2 block overflow-x-auto rounded-lg bg-[var(--surface-muted)] p-2 font-mono">
                  {task.title}
                </code>
              </details>
            )}
          </div>

          <div className="flex shrink-0 flex-wrap gap-2">
            {canStart && (
              <Button size="sm" onClick={() => void runAction(() => startTask(task.id))} disabled={actionPending}>
                <Play className="h-3.5 w-3.5" />
                启动
              </Button>
            )}
            {canRetry && (
              <Button size="sm" variant="secondary" onClick={() => void runAction(() => retryTask(task.id))} disabled={actionPending}>
                <RotateCcw className="h-3.5 w-3.5" />
                重试
              </Button>
            )}
            {!isTerminal && (
              <Button size="sm" variant="danger" onClick={() => void runAction(() => cancelTask(task.id))} disabled={actionPending}>
                <XCircle className="h-3.5 w-3.5" />
                取消
              </Button>
            )}
          </div>
        </div>

        {actionError && (
          <ErrorState
            className="mt-3"
            title="任务操作未完成"
            description={actionError}
          />
        )}
      </Panel>

      <Panel>
        <div className="flex items-center gap-2">
          <Clock3 className="h-4 w-4 text-[var(--accent-purple)]" />
          <h3 className="text-sm font-semibold text-[var(--text)]">概览</h3>
        </div>
        <dl className="mt-3 grid grid-cols-1 gap-3 text-sm sm:grid-cols-2">
          <InfoItem label="创建时间" value={formatTaskDate(task.created_at)} />
          <InfoItem label="更新时间" value={formatTaskDate(task.updated_at)} />
          <InfoItem label="优先级" value={task.priority} />
          <InfoItem label="完成时间" value={formatTaskDate(task.completed_at)} />
          {workspaceName && (
            <div>
              <dt className="text-xs text-[var(--text-faint)]">工作空间</dt>
              <dd className="mt-1 flex items-center gap-2 text-[var(--text-secondary)]">
                <span className="truncate">{workspaceName}</span>
                {onOpenWorkspace && (
                  <button
                    type="button"
                    onClick={onOpenWorkspace}
                    className="shrink-0 text-xs text-[var(--accent-primary)] underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring-soft)]"
                  >
                    打开
                  </button>
                )}
              </dd>
            </div>
          )}
        </dl>
        {(task.workflow_graph_id || task.agent_team_id) && (
          <details className="mt-4 text-xs text-[var(--text-secondary)]">
            <summary className="cursor-pointer select-none hover:text-[var(--text-primary)]">
              技术关联
            </summary>
            <dl className="mt-2 grid gap-2 rounded-lg bg-[var(--surface-muted)] p-3 font-mono">
              {task.workflow_graph_id && <InfoItem label="Workflow ID" value={task.workflow_graph_id} mono />}
              {task.agent_team_id && <InfoItem label="Agent Team ID" value={task.agent_team_id} mono />}
              <InfoItem label="Task ID" value={task.id} mono />
            </dl>
          </details>
        )}
      </Panel>

      <DetailSection title="执行活动" icon={<CheckCircle2 className="h-4 w-4 text-[var(--accent-blue)]" />}>
        {timeline === null || executions === null ? (
          <DetailSkeleton />
        ) : timeline.length === 0 && executions.length === 0 ? (
          <p className="text-sm text-[var(--text-secondary)]">暂无执行记录。</p>
        ) : (
          <div className="space-y-3">
            {timeline.map((event) => (
              <div key={event.id} className="border-l-2 border-[var(--border-soft)] pl-3">
                <p className="text-sm text-[var(--text)]">{event.message}</p>
                <p className="mt-1 text-xs text-[var(--text-faint)]">{formatTaskDate(event.created_at)}</p>
                {Object.keys(event.metadata || {}).length > 0 && (
                  <details className="mt-2 text-xs text-[var(--text-secondary)]">
                    <summary className="cursor-pointer select-none">技术详情</summary>
                    <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap rounded-lg bg-[var(--surface-muted)] p-2 font-mono">
                      {JSON.stringify(event.metadata, null, 2)}
                    </pre>
                  </details>
                )}
              </div>
            ))}
            {executions.map((execution) => (
              <div key={execution.id} className="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-[var(--surface-muted)] px-3 py-2 text-sm">
                <span className="text-[var(--text-secondary)]">第 {execution.attempt} 次执行</span>
                <Badge tone={execution.status === "completed" ? "success" : execution.status === "failed" ? "danger" : "default"}>
                  {execution.status}
                </Badge>
                {execution.error && <p className="basis-full text-xs text-[var(--danger-fg)]">{execution.error}</p>}
              </div>
            ))}
          </div>
        )}
      </DetailSection>

      <DetailSection title="任务产物" icon={<FileText className="h-4 w-4 text-[var(--accent-purple)]" />}>
        {artifacts === null ? (
          <DetailSkeleton />
        ) : artifacts.length === 0 ? (
          <p className="text-sm text-[var(--text-secondary)]">暂无可用任务产物。</p>
        ) : (
          <div className="space-y-2">
            {artifacts.map((artifact) => {
              const Icon = artifactIcon(classifyArtifactType(artifact.name));
              const size = formatSize(artifact.size);
              return (
                <div key={artifact.id} className="rounded-lg border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3 py-2">
                  <div className="flex items-start gap-2">
                    <Icon className="mt-0.5 h-4 w-4 shrink-0 text-[var(--accent-purple)]" />
                    <div className="min-w-0">
                      <p className="truncate text-sm font-medium text-[var(--text)]">{artifact.name}</p>
                      <p className="mt-0.5 text-xs text-[var(--text-faint)]">
                        {[artifact.mime_type, size, formatTaskDate(artifact.updated_at)].filter(Boolean).join(" · ") || "文件"}
                      </p>
                      {artifact.path && <p className="mt-1 break-all font-mono text-xs text-[var(--text-secondary)]">{artifact.path}</p>}
                      {artifact.summary && <p className="mt-1 text-xs leading-5 text-[var(--text-secondary)]">{artifact.summary}</p>}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </DetailSection>

      <ResourceBindingPanel target={{ kind: "task", task_id: task.id }} />
      <ArtifactResultsPanel source={evidenceSource} completed={Boolean(latestEvidenceExecution)} />
      <MemorySkillCandidatePanel source={evidenceSource} completed={Boolean(latestEvidenceExecution)} />

      {decisions && decisions.length > 0 && (
        <DetailSection title="用户决定" icon={<Info className="h-4 w-4 text-[var(--accent-gold)]" />}>
          <div className="space-y-2">
            {decisions.map((decision) => (
              <div key={decision.id} className="rounded-lg bg-[var(--surface-muted)] px-3 py-2 text-sm">
                <p className="text-[var(--text)]">{decision.prompt}</p>
                <p className="mt-1 text-xs text-[var(--text-faint)]">{decision.status === "pending" ? "等待选择" : "已处理"}</p>
              </div>
            ))}
          </div>
        </DetailSection>
      )}

      {detailError && (
        <ErrorState
          title="部分任务详情暂时无法加载"
          description={detailError}
          action={
            <Button size="sm" variant="secondary" onClick={() => void reloadDetail()}>
              重新加载
            </Button>
          }
        />
      )}
    </div>
  );
}

function InfoItem({
  label,
  value,
  mono = false,
}: {
  label: string;
  value?: string | null;
  mono?: boolean;
}) {
  if (!value) return null;
  return (
    <div>
      <dt className="text-xs text-[var(--text-faint)]">{label}</dt>
      <dd className={`mt-1 break-words text-[var(--text-secondary)] ${mono ? "font-mono text-xs" : ""}`}>
        {value}
      </dd>
    </div>
  );
}

function DetailSection({
  title,
  icon,
  children,
}: {
  title: string;
  icon: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <Panel>
      <div className="flex items-center gap-2">
        {icon}
        <h3 className="text-sm font-semibold text-[var(--text)]">{title}</h3>
      </div>
      <div className="mt-3">{children}</div>
    </Panel>
  );
}

function DetailSkeleton() {
  return (
    <div className="space-y-2">
      <Skeleton className="h-4 w-3/4" />
      <Skeleton className="h-4 w-1/2" />
      <Skeleton className="h-4 w-2/3" />
    </div>
  );
}
