import { useCallback, useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { ArrowLeft, Play, Plus, RotateCcw, XCircle } from "lucide-react";
import {
  Button,
  Badge,
  Input,
  Textarea,
  Modal,
  PageHeader,
  Panel,
  EmptyState,
  Spinner,
} from "../components/ui";
import {
  Workspace,
  Task,
  TaskEvent,
  Artifact,
  TaskExecution,
  getWorkspace,
  listTasks,
  createTask,
  startTask,
  retryTask,
  cancelTask,
  listTaskTimeline,
  listTaskArtifacts,
  listTaskExecutions,
} from "../api/client";

const TASK_STATUS_TONE: Record<string, "default" | "success" | "warning" | "danger" | "info"> = {
  draft: "default",
  ready: "info",
  running: "info",
  waiting_approval: "warning",
  waiting_user: "warning",
  blocked: "warning",
  completed: "success",
  failed: "danger",
  cancelled: "default",
};

export default function WorkspaceDetailPage() {
  const { id = "" } = useParams();
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [tasks, setTasks] = useState<Task[] | null>(null);
  const [selected, setSelected] = useState<Task | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");

  const reload = useCallback(async () => {
    const ws = await getWorkspace(id);
    if (!ws.ok) {
      setError(ws.error);
      return;
    }
    setWorkspace(ws.data);
    const ts = await listTasks({ workspace_id: id, limit: 100 });
    if (ts.ok) {
      setTasks(ts.data);
      setSelected((prev) => prev && ts.data.find((t) => t.id === prev.id)?.id === prev.id
        ? ts.data.find((t) => t.id === prev.id)!
        : ts.data[0] ?? null);
    } else {
      setError(ts.error);
    }
  }, [id]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const handleCreate = async () => {
    if (!title.trim()) return;
    const res = await createTask({ workspace_id: id, title, description });
    if (res.ok) {
      setShowCreate(false);
      setTitle("");
      setDescription("");
      void reload();
    } else {
      setError(res.error);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title={workspace?.name ?? "工作空间"}
        description={workspace?.description}
        actions={
          <>
            <Link to="/workspaces">
              <Button variant="ghost">
                <ArrowLeft className="h-4 w-4" />
                返回
              </Button>
            </Link>
            <Button onClick={() => setShowCreate(true)}>
              <Plus className="h-4 w-4" />
              新建任务
            </Button>
          </>
        }
      />

      {error && (
        <p className="mx-4 mt-3 text-sm text-[var(--danger)]">{error}</p>
      )}

      <div className="mx-4 mt-4 flex flex-1 gap-3 overflow-hidden pb-4">
        {/* Task list */}
        <div className="w-72 shrink-0 overflow-y-auto rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--panel)] p-2">
          {tasks === null ? (
            <div className="flex justify-center py-8">
              <Spinner />
            </div>
          ) : tasks.length === 0 ? (
            <EmptyState title="暂无任务" description="新建第一个任务。" />
          ) : (
            tasks.map((task) => (
              <button
                key={task.id}
                onClick={() => setSelected(task)}
                className={`mb-1 w-full rounded-lg px-3 py-2 text-left transition-colors ${
                  selected?.id === task.id
                    ? "bg-[var(--panel-hover)]"
                    : "hover:bg-[var(--panel-hover)]"
                }`}
              >
                <div className="flex items-center justify-between gap-1">
                  <span className="truncate text-sm font-medium text-[var(--text)]">
                    {task.title}
                  </span>
                  <Badge tone={TASK_STATUS_TONE[task.status] ?? "default"}>
                    {task.status}
                  </Badge>
                </div>
              </button>
            ))
          )}
        </div>

        {/* Task detail */}
        <div className="flex-1 overflow-y-auto">
          {!selected ? (
            <EmptyState title="选择一个任务" description="在左侧选择任务查看详情。" />
          ) : (
            <TaskDetail key={selected.id} task={selected} onChanged={reload} />
          )}
        </div>
      </div>

      <Modal open={showCreate} onClose={() => setShowCreate(false)} title="新建任务">
        <div className="space-y-3">
          <Input label="标题" value={title} onChange={(e) => setTitle(e.target.value)} autoFocus />
          <Textarea
            label="描述"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={3}
          />
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setShowCreate(false)}>
              取消
            </Button>
            <Button onClick={handleCreate} disabled={!title.trim()}>
              创建
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
}

function TaskDetail({ task, onChanged }: { task: Task; onChanged: () => void }) {
  const [timeline, setTimeline] = useState<TaskEvent[] | null>(null);
  const [artifacts, setArtifacts] = useState<Artifact[] | null>(null);
  const [executions, setExecutions] = useState<TaskExecution[] | null>(null);

  const reloadDetail = useCallback(async () => {
    const [tl, arts, exes] = await Promise.all([
      listTaskTimeline(task.id),
      listTaskArtifacts(task.id),
      listTaskExecutions(task.id),
    ]);
    if (tl.ok) setTimeline(tl.data);
    if (arts.ok) setArtifacts(arts.data);
    if (exes.ok) setExecutions(exes.data);
  }, [task.id]);

  useEffect(() => {
    void reloadDetail();
  }, [reloadDetail]);

  const active = !["completed", "failed", "cancelled"].includes(task.status);

  const handleStart = async () => {
    await startTask(task.id);
    onChanged();
    void reloadDetail();
  };
  const handleRetry = async () => {
    await retryTask(task.id);
    onChanged();
    void reloadDetail();
  };
  const handleCancel = async () => {
    await cancelTask(task.id);
    onChanged();
    void reloadDetail();
  };

  return (
    <div className="space-y-3">
      <Panel>
        <div className="flex items-start justify-between gap-3">
          <div>
            <h3 className="text-base font-semibold text-[var(--text)]">{task.title}</h3>
            {task.description && (
              <p className="mt-1 text-sm text-[var(--text-muted)]">{task.description}</p>
            )}
            <div className="mt-2 flex items-center gap-2">
              <Badge tone={TASK_STATUS_TONE[task.status] ?? "default"}>{task.status}</Badge>
              <Badge tone="default">优先级：{task.priority}</Badge>
              {task.workflow_graph_id && <Badge tone="info">绑定工作流</Badge>}
              {task.agent_team_id && <Badge tone="info">绑定团队</Badge>}
            </div>
          </div>
          <div className="flex shrink-0 gap-2">
            {active && (
              <Button size="sm" onClick={handleStart}>
                <Play className="h-3.5 w-3.5" />
                启动
              </Button>
            )}
            {!active && task.status !== "completed" && (
              <Button size="sm" variant="secondary" onClick={handleRetry}>
                <RotateCcw className="h-3.5 w-3.5" />
                重试
              </Button>
            )}
            {active && (
              <Button size="sm" variant="danger" onClick={handleCancel}>
                <XCircle className="h-3.5 w-3.5" />
                取消
              </Button>
            )}
          </div>
        </div>
      </Panel>

      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        <Panel>
          <h4 className="mb-2 text-sm font-semibold text-[var(--text)]">时间线</h4>
          {timeline === null ? (
            <Spinner />
          ) : timeline.length === 0 ? (
            <p className="text-sm text-[var(--text-muted)]">暂无记录。</p>
          ) : (
            <ul className="space-y-2">
              {timeline.map((event) => (
                <li key={event.id} className="text-sm">
                  <span className="text-[var(--text-faint)]">
                    {new Date(event.created_at).toLocaleTimeString()}
                  </span>{" "}
                  <span className="text-[var(--text)]">{event.message}</span>
                </li>
              ))}
            </ul>
          )}
        </Panel>

        <Panel>
          <h4 className="mb-2 text-sm font-semibold text-[var(--text)]">执行历史</h4>
          {executions === null ? (
            <Spinner />
          ) : executions.length === 0 ? (
            <p className="text-sm text-[var(--text-muted)]">暂无执行。</p>
          ) : (
            <ul className="space-y-2">
              {executions.map((execution) => (
                <li key={execution.id} className="flex items-center justify-between text-sm">
                  <span className="text-[var(--text-muted)]">attempt #{execution.attempt}</span>
                  <Badge tone={execution.status === "completed" ? "success" : "default"}>
                    {execution.status}
                  </Badge>
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </div>

      <Panel>
        <h4 className="mb-2 text-sm font-semibold text-[var(--text)]">产物</h4>
        {artifacts === null ? (
          <Spinner />
        ) : artifacts.length === 0 ? (
          <p className="text-sm text-[var(--text-muted)]">暂无产物。</p>
        ) : (
          <ul className="space-y-2">
            {artifacts.map((artifact) => (
              <li key={artifact.id} className="text-sm">
                <span className="font-medium text-[var(--text)]">{artifact.name}</span>
                <span className="ml-2 text-xs text-[var(--text-faint)]">
                  {artifact.artifact_type}
                </span>
                {artifact.summary && (
                  <p className="mt-0.5 text-[var(--text-muted)]">{artifact.summary}</p>
                )}
              </li>
            ))}
          </ul>
        )}
      </Panel>
    </div>
  );
}
