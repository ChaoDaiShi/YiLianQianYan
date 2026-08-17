import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, Check, Clipboard, FolderOpen, Plus } from "lucide-react";
import { Link, useParams } from "react-router-dom";
import {
  createTask,
  getWorkspace,
  listTasks,
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
  Modal,
  PageHeader,
  Panel,
  Skeleton,
  Textarea,
} from "../components/ui";
import {
  formatTaskDate,
  getTaskStatusLabel,
  getTaskStatusTone,
} from "../features/tasks/taskPresentation";
import { deriveTaskDisplayTitle } from "../components/chat/taskTitle";
import { formatWorkspacePath } from "../features/tasks/workspacePresentation";

export default function WorkspaceDetailPage() {
  const { id = "" } = useParams();
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [tasks, setTasks] = useState<Task[] | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [taskError, setTaskError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [copied, setCopied] = useState(false);

  const reload = useCallback(async () => {
    setError(null);
    setTaskError(null);
    const workspaceResult = await getWorkspace(id);
    if (!workspaceResult.ok) {
      setWorkspace(null);
      setTasks(null);
      setError(workspaceResult.error);
      return;
    }

    setWorkspace(workspaceResult.data);
    const taskResult = await listTasks({ workspace_id: id, limit: 100 });
    if (!taskResult.ok) {
      setTasks(null);
      setTaskError(taskResult.error);
      return;
    }

    setTasks(taskResult.data);
    setSelectedId((current) =>
      current && taskResult.data.some((task) => task.id === current)
        ? current
        : taskResult.data[0]?.id || null
    );
  }, [id]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const selectedTask = tasks?.find((task) => task.id === selectedId) || null;

  const handleCreate = async () => {
    if (!title.trim()) return;
    const result = await createTask({ workspace_id: id, title, description });
    if (result.ok) {
      setShowCreate(false);
      setTitle("");
      setDescription("");
      void reload();
    } else {
      setTaskError(result.error);
    }
  };

  const copyPath = async () => {
    const path = formatWorkspacePath(workspace?.root_path);
    if (!path || !navigator.clipboard) return;
    await navigator.clipboard.writeText(path);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1400);
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader
        title={workspace?.name || "工作空间"}
        description={workspace?.description || "查看任务使用的文件与上下文。"}
        actions={
          <>
            <Link to="/workspaces">
              <Button type="button" variant="ghost">
                <ArrowLeft className="h-4 w-4" />
                返回
              </Button>
            </Link>
            <Button type="button" onClick={() => setShowCreate(true)} disabled={!workspace}>
              <Plus className="h-4 w-4" />
              新建任务
            </Button>
          </>
        }
      />

      {error ? (
        <div className="min-h-0 flex-1 p-4">
          <ErrorState
            title="工作空间暂时无法加载"
            description={error}
            action={
              <Button size="sm" variant="secondary" onClick={() => void reload()}>
                重新加载
              </Button>
            }
          />
        </div>
      ) : (
        <>
          <Panel className="mx-4 mt-4 shrink-0">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="flex min-w-0 items-start gap-3">
                <div className="rounded-[var(--radius-md)] bg-[var(--accent-soft)] p-2 text-[var(--accent-primary)]">
                  <FolderOpen className="h-5 w-5" />
                </div>
                <div className="min-w-0">
                  <p className="text-xs text-[var(--text-faint)]">当前工作空间</p>
                  <h2 className="mt-0.5 truncate text-base font-semibold text-[var(--text)]">
                    {workspace?.name || "加载中…"}
                  </h2>
                  {formatWorkspacePath(workspace?.root_path) && (
                    <div className="mt-1 flex max-w-full items-center gap-2">
                      <p className="truncate font-mono text-xs text-[var(--text-secondary)]" title={workspace?.root_path || undefined}>
                        {workspace?.root_path}
                      </p>
                      <button
                        type="button"
                        onClick={() => void copyPath()}
                        className="shrink-0 rounded p-1 text-[var(--text-faint)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] focus-ring-token focus-visible:outline-none"
                        aria-label="复制工作空间路径"
                      >
                        {copied ? <Check className="h-3.5 w-3.5" /> : <Clipboard className="h-3.5 w-3.5" />}
                      </button>
                    </div>
                  )}
                </div>
              </div>
              {workspace && <Badge tone={workspace.status === "active" ? "success" : "default"}>{workspace.status === "active" ? "活跃" : "已归档"}</Badge>}
            </div>
          </Panel>

          {taskError && (
            <div className="mx-4 mt-3">
              <ErrorState
                title="任务列表暂时无法加载"
                description={taskError}
                action={
                  <Button size="sm" variant="secondary" onClick={() => void reload()}>
                    重新加载
                  </Button>
                }
              />
            </div>
          )}

          <div className="workspace-detail-grid min-h-0 flex-1 gap-3 px-4 pb-4 pt-4">
            <Panel padding={false} className="min-h-0 overflow-hidden">
              <div className="flex h-full min-h-0 flex-col">
                <div className="shrink-0 border-b border-[var(--border-soft)] px-4 py-3">
                  <h2 className="text-sm font-semibold text-[var(--text)]">任务与产物</h2>
                  <p className="mt-0.5 text-xs text-[var(--text-faint)]">浏览这个工作空间关联的真实任务记录</p>
                </div>
                <div className="scrollbar-thin min-h-0 flex-1 overflow-y-auto p-2">
                  {tasks === null ? (
                    <div className="space-y-2 p-1">
                      {Array.from({ length: 4 }).map((_, index) => (
                        <div key={index} className="space-y-2 rounded-[var(--radius-md)] border border-[var(--border-soft)] p-3">
                          <Skeleton className="h-4 w-3/4" />
                          <Skeleton className="h-3 w-1/2" />
                        </div>
                      ))}
                    </div>
                  ) : tasks.length === 0 ? (
                    <EmptyState
                      icon={<FolderOpen className="h-5 w-5" />}
                      title="还没有关联任务"
                      description="创建一个任务后，执行记录和真实产物会显示在这里。"
                      action={<Button size="sm" onClick={() => setShowCreate(true)}>新建任务</Button>}
                      className="py-16"
                    />
                  ) : (
                    <div className="space-y-1">
                      {tasks.map((task) => (
                        <button
                          key={task.id}
                          type="button"
                          aria-selected={selectedId === task.id}
                          onClick={() => setSelectedId(task.id)}
                          className={`relative flex min-h-[68px] w-full items-center gap-3 rounded-[var(--radius-md)] border px-3 py-2 text-left transition-colors focus-ring-token focus-visible:outline-none ${
                            selectedId === task.id
                              ? "border-[var(--accent-border)] bg-[var(--accent-soft)]"
                              : "border-transparent hover:border-[var(--border-soft)] hover:bg-[var(--surface-hover)]"
                          }`}
                        >
                          {selectedId === task.id && <span className="absolute inset-y-3 left-0 w-0.5 rounded-r-full bg-[var(--accent-primary)]" />}
                          <div className="min-w-0 flex-1">
                            <p className="truncate text-sm font-medium text-[var(--text)]">{deriveTaskDisplayTitle(task.title)}</p>
                            <p className="mt-1 text-xs text-[var(--text-faint)]">{formatTaskDate(task.updated_at) || "时间未知"}</p>
                          </div>
                          <Badge tone={getTaskStatusTone(task.status)}>{getTaskStatusLabel(task.status)}</Badge>
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            </Panel>

            <Panel padding={false} className="workspace-detail-content min-h-0 overflow-y-auto">
              {selectedTask ? (
                <div className="p-3 sm:p-4">
                  <TaskDetailPanel
                    task={selectedTask}
                    workspaceName={workspace?.name}
                    onChanged={() => void reload()}
                  />
                </div>
              ) : (
                <EmptyState
                  icon={<FolderOpen className="h-6 w-6" />}
                  title="选择一个任务"
                  description="在左侧浏览任务及其真实产物。"
                  className="py-20"
                />
              )}
            </Panel>
          </div>
        </>
      )}

      <Modal open={showCreate} onClose={() => setShowCreate(false)} title="新建任务">
        <div className="space-y-3">
          <Input label="标题" value={title} onChange={(event) => setTitle(event.target.value)} autoFocus />
          <Textarea label="描述" value={description} onChange={(event) => setDescription(event.target.value)} rows={3} />
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={() => setShowCreate(false)}>取消</Button>
            <Button type="button" onClick={() => void handleCreate()} disabled={!title.trim()}>创建</Button>
          </div>
        </div>
      </Modal>
    </div>
  );
}
