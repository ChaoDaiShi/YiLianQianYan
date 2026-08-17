import { useCallback, useEffect, useMemo, useState } from "react";
import { ListTodo, RefreshCw, Search, Sparkles } from "lucide-react";
import { useNavigate } from "react-router-dom";
import {
  listTasks,
  listWorkspaces,
  type Task,
  type Workspace,
} from "../api/client";
import TaskDetailPanel from "../components/tasks/TaskDetailPanel";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Input,
  PageHeader,
  Panel,
  Skeleton,
} from "../components/ui";
import {
  filterTasks,
  formatTaskDate,
  getTaskStatusLabel,
  getTaskStatusTone,
  type TaskFilter,
} from "../features/tasks/taskPresentation";
import { deriveTaskDisplayTitle } from "../components/chat/taskTitle";

const FILTERS: Array<{ id: TaskFilter; label: string }> = [
  { id: "all", label: "全部" },
  { id: "running", label: "运行中" },
  { id: "completed", label: "已完成" },
  { id: "failed", label: "失败" },
  { id: "cancelled", label: "已取消" },
];

export default function TaskCenterPage() {
  const navigate = useNavigate();
  const [tasks, setTasks] = useState<Task[] | null>(null);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [filter, setFilter] = useState<TaskFilter>("all");
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [workspaceError, setWorkspaceError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    setError(null);
    const [taskResult, workspaceResult] = await Promise.all([
      listTasks({ limit: 200 }),
      listWorkspaces(),
    ]);
    if (taskResult.ok) {
      setTasks(taskResult.data);
    } else {
      setTasks(null);
      setError(taskResult.error);
    }
    if (workspaceResult.ok) {
      setWorkspaces(workspaceResult.data);
      setWorkspaceError(null);
    } else {
      setWorkspaceError(workspaceResult.error);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const visibleTasks = useMemo(
    () => filterTasks(tasks || [], filter, query, workspaces),
    [filter, query, tasks, workspaces]
  );

  useEffect(() => {
    if (!visibleTasks.some((task) => task.id === selectedId)) {
      setSelectedId(visibleTasks[0]?.id || null);
    }
  }, [selectedId, visibleTasks]);

  const selectedTask = visibleTasks.find((task) => task.id === selectedId) || null;
  const workspaceNames = new Map(workspaces.map((workspace) => [workspace.id, workspace.name]));

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader
        title="任务中心"
        description="管理小昔涟执行过的任务和当前进度"
        actions={
          <Button type="button" onClick={() => navigate("/chat")}>
            <Sparkles className="h-4 w-4" />
            开始一个任务
          </Button>
        }
      />

      <div className="flex min-h-0 flex-1 flex-col gap-3 px-4 pb-4 pt-4">
        <div className="flex flex-col gap-3 rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface)] p-3 sm:flex-row sm:items-center">
          <label className="relative min-w-0 flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--text-faint)]" />
            <Input
              aria-label="搜索任务"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="搜索任务、描述或工作空间"
              className="pl-9"
            />
          </label>
          <div className="flex min-w-0 flex-wrap gap-1" role="group" aria-label="任务状态筛选">
            {FILTERS.map((item) => (
              <button
                key={item.id}
                type="button"
                aria-pressed={filter === item.id}
                onClick={() => setFilter(item.id)}
                className={`rounded-[var(--radius-sm)] px-3 py-1.5 text-xs transition-colors focus-ring-token focus-visible:outline-none ${
                  filter === item.id
                    ? "bg-[var(--accent-soft)] font-medium text-[var(--accent-soft-fg)]"
                    : "text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
                }`}
              >
                {item.label}
              </button>
            ))}
          </div>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => void reload()}
            aria-label="刷新任务"
          >
            <RefreshCw className="h-3.5 w-3.5" />
            刷新
          </Button>
        </div>

        {error ? (
          <ErrorState
            title="任务记录暂时无法加载"
            description={error}
            action={
              <Button type="button" size="sm" variant="secondary" onClick={() => void reload()}>
                重新加载
              </Button>
            }
          />
        ) : tasks === null ? (
          <TaskCenterSkeleton />
        ) : tasks.length === 0 ? (
          <Panel className="min-h-0 flex-1">
            <EmptyState
              icon={<ListTodo className="h-6 w-6" />}
              title="还没有任务记录"
              description="告诉小昔涟你想完成什么，执行过的任务会逐渐出现在这里。"
              action={
                <Button type="button" onClick={() => navigate("/chat")}>
                  开始一个任务
                </Button>
              }
              className="py-20"
            />
          </Panel>
        ) : (
          <div className="task-center-grid min-h-0 flex-1">
            <Panel padding={false} className="min-h-0 overflow-hidden">
              <div className="flex h-full min-h-0 flex-col">
                <div className="flex shrink-0 items-center justify-between border-b border-[var(--border-soft)] px-4 py-3">
                  <div>
                    <h2 className="text-sm font-semibold text-[var(--text)]">任务列表</h2>
                    <p className="mt-0.5 text-xs text-[var(--text-faint)]">{visibleTasks.length} 个任务</p>
                  </div>
                </div>
                <div className="scrollbar-thin min-h-0 flex-1 overflow-y-auto p-2">
                  {visibleTasks.length === 0 ? (
                    <EmptyState
                      icon={<Search className="h-5 w-5" />}
                      title="没有匹配的任务"
                      description="尝试更换搜索词或状态筛选。"
                      className="py-16"
                    />
                  ) : (
                    <div className="space-y-1">
                      {visibleTasks.map((task) => (
                        <TaskRow
                          key={task.id}
                          task={task}
                          workspaceName={workspaceNames.get(task.workspace_id)}
                          selected={selectedId === task.id}
                          onSelect={() => setSelectedId(task.id)}
                        />
                      ))}
                    </div>
                  )}
                </div>
              </div>
            </Panel>

            <Panel padding={false} className="task-center-detail min-h-0 overflow-y-auto">
              {selectedTask ? (
                <div className="p-3 sm:p-4">
                  <TaskDetailPanel
                    task={selectedTask}
                    workspaceName={workspaceNames.get(selectedTask.workspace_id)}
                    onOpenWorkspace={() => navigate(`/workspaces/${selectedTask.workspace_id}`)}
                    onChanged={() => void reload()}
                  />
                </div>
              ) : (
                <EmptyState
                  icon={<ListTodo className="h-6 w-6" />}
                  title="选择一个任务"
                  description="在左侧选择任务查看真实详情。"
                  className="py-20"
                />
              )}
            </Panel>
          </div>
        )}

        {workspaceError && !error && (
          <p className="text-xs text-[var(--text-faint)]" role="status">
            工作空间名称暂时无法加载，任务详情将隐藏该关联。
          </p>
        )}
      </div>
    </div>
  );
}

function TaskRow({
  task,
  workspaceName,
  selected,
  onSelect,
}: {
  task: Task;
  workspaceName?: string;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      aria-selected={selected}
      onClick={onSelect}
      className={`group relative flex min-h-[72px] w-full items-center gap-3 rounded-[var(--radius-md)] border px-3 py-2.5 text-left transition-colors focus-ring-token focus-visible:outline-none ${
        selected
          ? "border-[var(--accent-border)] bg-[var(--accent-soft)]"
          : "border-transparent hover:border-[var(--border-soft)] hover:bg-[var(--surface-hover)]"
      }`}
    >
      {selected && <span className="absolute inset-y-3 left-0 w-0.5 rounded-r-full bg-[var(--accent-primary)]" />}
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium text-[var(--text)]">
          {deriveTaskDisplayTitle(task.title)}
        </p>
        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-[var(--text-faint)]">
          <span>{formatTaskDate(task.updated_at) || "时间未知"}</span>
          {workspaceName && <span className="truncate">{workspaceName}</span>}
        </div>
      </div>
      <Badge tone={getTaskStatusTone(task.status)}>{getTaskStatusLabel(task.status)}</Badge>
    </button>
  );
}

function TaskCenterSkeleton() {
  return (
    <div className="task-center-grid min-h-0 flex-1">
      <Panel padding={false} className="space-y-2 p-3">
        {Array.from({ length: 5 }).map((_, index) => (
          <div key={index} className="flex min-h-[72px] items-center gap-3 rounded-[var(--radius-md)] border border-[var(--border-soft)] px-3">
            <div className="min-w-0 flex-1 space-y-2">
              <Skeleton className="h-4 w-2/3" />
              <Skeleton className="h-3 w-1/2" />
            </div>
            <Skeleton className="h-5 w-14" />
          </div>
        ))}
      </Panel>
      <Panel className="space-y-3">
        <Skeleton className="h-5 w-1/2" />
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-4 w-2/3" />
      </Panel>
    </div>
  );
}
