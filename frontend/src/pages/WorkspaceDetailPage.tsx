import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, BookOpen, Edit3, FolderOpen, MoreHorizontal, Palette, Plus, Send, Trash2 } from "lucide-react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
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
  ListRow,
  Tabs,
  FormField,
  showToast,
} from "../components/ui";
import {
  formatTaskDate,
  getTaskStatusLabel,
  getTaskStatusTone,
} from "../features/tasks/taskPresentation";
import { deriveTaskDisplayTitle } from "../components/chat/taskTitle";
import WorkspaceFilesPanel from "../features/workspace-preview/WorkspaceFilesPanel";
import WorkspaceArtifacts from "../features/workspace-preview/WorkspaceArtifacts";
import GitNodeManager from "../features/git/GitNodeManager";
import { createConversation, setConversationWorkspaceMode } from "../api/conversations";
import { requestResult } from "../core/api/http";

export default function WorkspaceDetailPage() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [tasks, setTasks] = useState<Task[] | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [taskError, setTaskError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [tab, setTab] = useState("files");
  const [showGit, setShowGit] = useState(false);
  const [starting, setStarting] = useState(false);
  const [startError, setStartError] = useState("");
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
    setTab("files"); setShowGit(false); setStartError("");
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

  useEffect(() => {
    if (workspace && searchParams.get("edit") === "1") {
      handleOpenEdit();
      const next = new URLSearchParams(searchParams); next.delete("edit"); setSearchParams(next, { replace: true });
    }
  }, [workspace, searchParams, setSearchParams]);

  async function startConversation() {
    if (!workspace || starting) return;
    setStarting(true); setStartError("");
    try {
      const conversation = await createConversation(workspace.name);
      const binding = await requestResult("PUT", `/api/conversations/${encodeURIComponent(conversation.id)}/workspace`, { workspace_id: id });
      if (!binding.ok) throw new Error(binding.error);
      await setConversationWorkspaceMode(conversation.id, "work");
      navigate(`/chat/${encodeURIComponent(conversation.id)}`);
    } catch (cause) { setStartError(cause instanceof Error ? cause.message : "对话创建失败"); }
    finally { setStarting(false); }
  }

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
      title={<div className="workspace-breadcrumb"><Link to="/workspaces">工作空间</Link><span>/</span><h1>{workspace?.name || "工作空间"}</h1></div>}
      variant="workspace"
      maxWidth="none"
      className="page-canvas ui-opt-workspaces workspace-preview-page"
      actions={<div className="workspace-detail-actions">
        <Button type="button" onClick={() => void startConversation()} disabled={!workspace || workspace.status === "archived"} loading={starting}><Send size={16} />开始对话</Button>
        <details className="workspace-row-menu workspace-detail-menu"><summary aria-label="工作空间操作"><MoreHorizontal size={19} /></summary><div>
          <button type="button" onClick={handleOpenEdit} disabled={!workspace}><Edit3 size={14} />编辑空间</button>
          {workspace?.status !== "archived" && <button type="button" onClick={() => setShowDeleteConfirm(true)} disabled={!workspace}><Trash2 size={14} />归档空间</button>}
          <Link to="/workspaces"><ArrowLeft size={14} />返回空间列表</Link>
        </div></details>
      </div>}
    >
      {error ? <div className="p-4"><ErrorState title="工作空间暂时无法加载" description={error} onRetry={() => void reload()} /></div> : !workspace ? <div className="p-5" role="status">正在读取工作空间…</div> : <>
        <div className="workspace-detail-navigation">
          <Tabs tabs={[{ id: "files", label: "文件" }, { id: "tasks", label: "任务", badge: tasks?.length }, { id: "artifacts", label: "成果" }]} activeId={tab} onChange={setTab} aria-label="工作空间内容" />
          <nav aria-label="空间专项功能"><Link to={`/workspaces/${encodeURIComponent(id)}/learning`}><BookOpen size={15} />资料学习</Link><Link to={`/workspaces/${encodeURIComponent(id)}/creation`}><Palette size={15} />创作</Link></nav>
        </div>
        {startError && <div className="px-4 pt-3"><ErrorState title="对话未能开始" description={startError} /></div>}
        <div className="workspace-active-panel" id={`panel-${tab}`} role="tabpanel" aria-labelledby={`tab-${tab}`}>
          {tab === "files" && <WorkspaceFilesPanel key={`${id}:${workspace.root_path}`} workspaceId={id} rootPath={workspace.root_path} onEdit={handleOpenEdit} onShowChanges={() => setShowGit(true)} />}
          {tab === "artifacts" && <div className="workspace-results-scroll scrollbar-thin"><WorkspaceArtifacts workspaceId={id} /></div>}
          {tab === "tasks" && <>
            <div className="workspace-tasks-toolbar"><p>当前空间的任务与执行记录</p><Button size="sm" onClick={() => setShowCreate(true)} disabled={workspace.status === "archived"}><Plus size={15} />新建任务</Button></div>
            {taskError && <ErrorState title="任务列表暂时无法加载" description={taskError} onRetry={() => void reload()} />}
            <div className="workspace-preview-tasks">
              <Panel padding={false} className="workspace-task-list-panel">
                <div className="workspace-task-list scrollbar-thin">
                  {tasks === null ? taskError ? <p className="workspace-tree-note">任务列表读取失败，请重新加载。</p> : <div className="space-y-3 p-4"><Skeleton className="h-12 w-full" /><Skeleton className="h-12 w-full" /><Skeleton className="h-12 w-full" /></div> : tasks.length === 0 ? <EmptyState icon={<FolderOpen size={22} />} title="还没有关联任务" description="创建任务后，执行记录和成果会保存在当前空间。" /> : <div role="listbox" aria-label="工作空间任务列表">
                    {tasks.map(task => <ListRow key={task.id} selected={selectedId === task.id} onSelect={() => setSelectedId(task.id)} title={deriveTaskDisplayTitle(task.title)} meta={formatTaskDate(task.updated_at) || "时间未知"} trailing={<Badge tone={getTaskStatusTone(task.status)}>{getTaskStatusLabel(task.status)}</Badge>} className="min-h-[64px]" />)}
                  </div>}
                </div>
              </Panel>
              <Panel padding={false} className="workspace-task-detail scrollbar-thin">{selectedTask ? <div className="p-4"><TaskDetailPanel task={selectedTask} workspaceName={workspace.name} onChanged={() => void reload()} /></div> : <EmptyState icon={<FolderOpen size={24} />} title="选择一个任务" description="从左侧选择任务，查看执行记录与真实成果。" />}</Panel>
            </div>
          </>}
        </div>
      </>}
      {/* New Task Modal */}
      <Modal open={showGit} onClose={() => setShowGit(false)} title="修改对比与 Git 操作" size="xl">
        <div className="workspace-git-dialog"><GitNodeManager visible={showGit} workspaceId={id} /></div>
      </Modal>
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
