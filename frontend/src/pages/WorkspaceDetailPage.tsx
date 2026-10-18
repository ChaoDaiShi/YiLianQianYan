import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, Clock3, Edit3, FolderOpen, Plus, Trash2 } from "lucide-react";
import { Link, useNavigate, useParams } from "react-router-dom";
import {
  createTask,
  deleteWorkspace,
  getWorkspace,
  listTasks,
  updateWorkspace,
  type Task,
  type Workspace,
} from "../api/client";
import TaskDetailPanel from "../components/tasks/TaskDetailPanel";
import PageShell from "../components/layout/PageShell";
import {
  Badge,
  Button,
  ConfirmDialog,
  EmptyState,
  ErrorState,
  Input,
  Modal,
  Panel,
  Skeleton,
  Textarea,
  KeyValueRow,
  StatTile,
  ListRow,
  Section,
  FormField,
  showToast,
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
  const navigate = useNavigate();
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [tasks, setTasks] = useState<Task[] | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [taskError, setTaskError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [creating, setCreating] = useState(false);

  // Edit workspace modal state
  const [showEdit, setShowEdit] = useState(false);
  const [editName, setEditName] = useState("");
  const [editDescription, setEditDescription] = useState("");
  const [editRootPath, setEditRootPath] = useState("");
  const [updating, setUpdating] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);

  // Delete workspace confirmation state
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [deleting, setDeleting] = useState(false);

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
    if (!title.trim() || creating) return;
    setCreating(true);
    const result = await createTask({
      workspace_id: id,
      title: title.trim(),
      description: description.trim() || undefined,
    });
    setCreating(false);
    if (result.ok) {
      setShowCreate(false);
      setTitle("");
      setDescription("");
      showToast("任务创建成功", "success");
      void reload();
    } else {
      setTaskError(result.error);
    }
  };

  const handleOpenEdit = () => {
    if (!workspace) return;
    setEditName(workspace.name);
    setEditDescription(workspace.description || "");
    setEditRootPath(workspace.root_path || "");
    setUpdateError(null);
    setShowEdit(true);
  };

  const handleUpdate = async () => {
    if (!editName.trim() || updating) return;
    setUpdating(true);
    setUpdateError(null);
    const result = await updateWorkspace(id, {
      name: editName.trim(),
      description: editDescription.trim() || undefined,
      root_path: editRootPath.trim() || undefined,
    });
    setUpdating(false);
    if (result.ok) {
      setShowEdit(false);
      showToast("工作空间信息已保存", "success");
      void reload();
    } else {
      setUpdateError(result.error || "更新工作空间失败");
    }
  };

  const handleDelete = async () => {
    setDeleting(true);
    const result = await deleteWorkspace(id);
    setDeleting(false);
    if (result.ok) {
      showToast("工作空间已归档", "success");
      navigate("/workspaces");
    } else {
      setError(result.error || "删除工作空间失败");
      setShowDeleteConfirm(false);
    }
  };

  return (
    <PageShell
      title={workspace?.name || "工作空间"}
      description={workspace?.description || "查看任务使用的文件与上下文。"}
      className="page-canvas ui-opt-workspaces flex h-full min-h-0 flex-col"
      actions={
        <div className="flex items-center gap-2">
          <Link to="/workspaces">
            <Button type="button" variant="ghost">
              <ArrowLeft className="h-4 w-4" />
              返回
            </Button>
          </Link>
          <Button
            type="button"
            variant="secondary"
            onClick={handleOpenEdit}
            disabled={!workspace}
            title="编辑工作空间"
          >
            <Edit3 className="h-4 w-4" />
            编辑
          </Button>
          {workspace?.status !== "archived" && (
            <Button
              type="button"
              variant="danger"
              onClick={() => setShowDeleteConfirm(true)}
              disabled={!workspace}
              title="归档工作空间"
            >
              <Trash2 className="h-4 w-4" />
              归档
            </Button>
          )}
          <Button type="button" onClick={() => setShowCreate(true)} disabled={!workspace}>
            <Plus className="h-4 w-4" />
            新建任务
          </Button>
        </div>
      }
    >
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
          <div className="mx-4 mt-4 shrink-0">
            <Section title="空间概览">
              <div className="flex flex-wrap items-start justify-between gap-4">
                <div className="flex min-w-0 flex-1 items-start gap-3">
                  <div className="workspace-folder-icon shrink-0">
                    <FolderOpen className="h-5 w-5" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <h2 className="truncate text-base font-semibold text-[var(--text)]">
                        {workspace?.name || "加载中…"}
                      </h2>
                      {workspace && (
                        <Badge tone={workspace.status === "active" ? "success" : "default"}>
                          {workspace.status === "active" ? "活跃" : "已归档"}
                        </Badge>
                      )}
                    </div>
                    <div className="mt-2 max-w-lg">
                      <KeyValueRow
                        label="根路径"
                        value={formatWorkspacePath(workspace?.root_path) || "未绑定本地目录"}
                        copyable={workspace?.root_path || undefined}
                        mono
                      />
                    </div>
                  </div>
                </div>
                {workspace?.active_tasks !== undefined && (
                  <div className="w-48 shrink-0">
                    <StatTile
                      label="进行中任务"
                      value={workspace.active_tasks}
                      icon={Clock3}
                      tone={workspace.active_tasks > 0 ? "info" : "default"}
                    />
                  </div>
                )}
              </div>
            </Section>
          </div>

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
                        <div
                          key={index}
                          className="space-y-2 rounded-[var(--radius-md)] border border-[var(--border-soft)] p-3"
                        >
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
                      action={
                        <Button size="sm" onClick={() => setShowCreate(true)}>
                          新建任务
                        </Button>
                      }
                      className="py-16"
                    />
                  ) : (
                    <div
                      className="space-y-1"
                      role="listbox"
                      aria-label="工作空间任务列表"
                      aria-activedescendant={selectedId ? `ws-task-option-${selectedId}` : undefined}
                    >
                      {tasks.map((task) => (
                        <ListRow
                          key={task.id}
                          selected={selectedId === task.id}
                          onSelect={() => setSelectedId(task.id)}
                          title={deriveTaskDisplayTitle(task.title)}
                          meta={formatTaskDate(task.updated_at) || "时间未知"}
                          trailing={
                            <Badge tone={getTaskStatusTone(task.status)}>
                              {getTaskStatusLabel(task.status)}
                            </Badge>
                          }
                          statusBar={getTaskStatusTone(task.status)}
                          className="min-h-[64px]"
                        />
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

      {/* New Task Modal */}
      <Modal
        open={showCreate}
        onClose={() => setShowCreate(false)}
        title="新建任务"
        footer={
          <>
            <Button variant="secondary" onClick={() => setShowCreate(false)}>
              取消
            </Button>
            <Button
              onClick={() => void handleCreate()}
              disabled={!title.trim() || creating}
              loading={creating}
              loadingLabel="创建中…"
            >
              创建任务
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          <FormField label="任务标题" required>
            <Input
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="例如：整理下载目录并归档"
              autoFocus
            />
          </FormField>
          <FormField label="补充描述（可选）">
            <Textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="说明任务期望的成果和约束"
              rows={3}
            />
          </FormField>
        </div>
      </Modal>

      {/* Edit Workspace Modal */}
      <Modal
        open={showEdit}
        onClose={() => setShowEdit(false)}
        title="编辑工作空间"
        footer={
          <>
            <Button variant="secondary" onClick={() => setShowEdit(false)}>
              取消
            </Button>
            <Button
              onClick={() => void handleUpdate()}
              disabled={!editName.trim() || updating}
              loading={updating}
              loadingLabel="保存中…"
            >
              保存修改
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          <FormField label="空间名称" required error={updateError || undefined}>
            <Input
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
              placeholder="例如：主开发工作区"
              autoFocus
            />
          </FormField>
          <FormField label="本地根目录（可选）">
            <Input
              value={editRootPath}
              onChange={(e) => setEditRootPath(e.target.value)}
              placeholder="例如：C:\Projects\MyProject"
            />
          </FormField>
          <FormField label="空间描述（可选）">
            <Textarea
              value={editDescription}
              onChange={(e) => setEditDescription(e.target.value)}
              placeholder="描述此工作空间的用途"
              rows={3}
            />
          </FormField>
        </div>
      </Modal>

      {/* Archive Workspace Confirmation */}
      <ConfirmDialog
        open={showDeleteConfirm}
        title="归档工作空间"
        description="归档后将保留历史任务与文件记录，该工作空间将被移至归档状态。"
        confirmLabel="确认归档"
        tone="danger"
        loading={deleting}
        onConfirm={() => void handleDelete()}
        onCancel={() => setShowDeleteConfirm(false)}
      />
    </PageShell>
  );
}
