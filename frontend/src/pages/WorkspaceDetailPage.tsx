import { useCallback, useEffect, useState } from "react";
import { Edit3, FolderOpen, MoreHorizontal, Plus, Send, SlidersHorizontal, Trash2 } from "lucide-react";
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
import { WORKSPACE_PROJECT_STAGES, type WorkspaceProject } from "../api/workspaces";
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
  Select,
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
import WorkspaceFolderField from "../features/workspace-preview/WorkspaceFolderField";
import WorkspaceArtifacts from "../features/workspace-preview/WorkspaceArtifacts";
import WorkspaceGoalTasks from "../features/workspace-preview/WorkspaceGoalTasks";
import WorkspaceSectionNav from "../features/workspace-preview/WorkspaceSectionNav";
import WorkspacePagesPanel from "../features/workspace-preview/WorkspacePagesPanel";
import { ResourceCard } from "../features/resources/ResourceCard";
import WorkspaceServiceOverridesDialog from "../features/settings/editor/WorkspaceServiceOverridesDialog";
import GitNodeManager from "../features/git/GitNodeManager";
import { createConversation, setConversationWorkspaceMode } from "../api/conversations";
import { requestResult } from "../core/api/http";

const emptyProject: WorkspaceProject = { stage: null, frontend: "", backend: "" };

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
  const [tab, setTab] = useState<"files" | "pages" | "tasks" | "artifacts">("files");
  const [taskView, setTaskView] = useState("goals");
  const [showGit, setShowGit] = useState(false);
  const [showServiceSettings, setShowServiceSettings] = useState(false);
  const [starting, setStarting] = useState(false);
  const [startError, setStartError] = useState("");
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);

  // Edit workspace modal state
  const [showEdit, setShowEdit] = useState(false);
  const [editName, setEditName] = useState("");
  const [editDescription, setEditDescription] = useState("");
  const [editRootPath, setEditRootPath] = useState("");
  const [editProject, setEditProject] = useState<WorkspaceProject>(emptyProject);
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
  }, [id]);

  useEffect(() => {
    setTab("files"); setTaskView("goals"); setSelectedId(null); setShowGit(false); setShowServiceSettings(false); setStartError("");
    void reload();
  }, [reload]);

  useEffect(() => {
    const section = searchParams.get("tab");
    setTab(section === "pages" || section === "tasks" || section === "artifacts" ? section : "files");
    if (section === "tasks") {
      const linkedTask = searchParams.get("task");
      setTaskView(linkedTask ? "records" : "goals");
      if (linkedTask) setSelectedId(linkedTask);
    }
  }, [searchParams]);

  const selectedTask = tasks?.find((task) => task.id === selectedId) || null;

  const handleCreate = async () => {
    if (!title.trim() || creating) return;
    setCreating(true);
    setCreateError(null);
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
      setCreateError(result.error);
    }
  };

  const handleOpenEdit = () => {
    if (!workspace) return;
    setEditName(workspace.name);
    setEditDescription(workspace.description || "");
    setEditRootPath(workspace.root_path || "");
    setEditProject(workspace.project ?? emptyProject);
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
      const conversation = await createConversation(workspace.name, { useDefaultWorkspace: false });
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
      description: editDescription.trim(),
      root_path: editRootPath.trim() || undefined,
      project: editProject,
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
      navigate("/workspaces?view=archived");
    } else {
      setError(result.error || "归档工作空间失败");
      setShowDeleteConfirm(false);
    }
  };

  return (
    <PageShell
      title={<div className="workspace-breadcrumb"><Link to={workspace?.status === "archived" ? "/workspaces?view=archived" : "/workspaces"}>工作空间</Link><span>/</span><h1>{workspace?.name || "工作空间"}</h1></div>}
      variant="workspace"
      maxWidth="none"
      className="page-canvas ui-opt-workspaces workspace-preview-page"
      actions={<div className="workspace-detail-actions">
        <Button type="button" onClick={() => void startConversation()} disabled={!workspace || workspace.status === "archived"} loading={starting}><Send size={16} />开始对话</Button>
        <details className="workspace-row-menu workspace-detail-menu"><summary aria-label="工作空间操作"><MoreHorizontal size={19} /></summary><div>
          <button type="button" onClick={handleOpenEdit} disabled={!workspace}><Edit3 size={14} />编辑空间</button>
          {workspace?.status === "active" && <button type="button" onClick={() => setShowServiceSettings(true)}><SlidersHorizontal size={14} />工作空间模型覆盖</button>}
          {workspace?.status !== "archived" && <button type="button" onClick={() => setShowDeleteConfirm(true)} disabled={!workspace}><Trash2 size={14} />归档空间</button>}
        </div></details>
      </div>}
    >
      {error ? <div className="p-4"><ErrorState title="工作空间暂时无法加载" description={error} onRetry={() => void reload()} /></div> : !workspace ? <div className="p-5" role="status">正在读取工作空间…</div> : <>
        {workspace.project && (workspace.project.stage || workspace.project.frontend || workspace.project.backend) && (
          <details className="workspace-project-summary">
            <summary>项目进度 {workspace.project.stage && <span>{WORKSPACE_PROJECT_STAGES[workspace.project.stage]}</span>}</summary>
            <dl>
              {workspace.project.frontend && <div><dt>前端</dt><dd>{workspace.project.frontend}</dd></div>}
              {workspace.project.backend && <div><dt>后端</dt><dd>{workspace.project.backend}</dd></div>}
            </dl>
          </details>
        )}
        <WorkspaceSectionNav workspaceId={id} active={tab} onChange={section => {
          if (section === "learning" || section === "creation") navigate(`/workspaces/${encodeURIComponent(id)}/${section}`);
          else { setTab(section); setSearchParams(current => { const next = new URLSearchParams(current); next.set("tab", section); return next; }, { replace: true }); }
        }} />
        {startError && <div className="px-4 pt-3"><ErrorState title="对话未能开始" description={startError} /></div>}
        <div className="workspace-active-panel" id={`panel-${tab}`} role="tabpanel" aria-labelledby={`tab-${tab}`}>
          {tab === "files" && <WorkspaceFilesPanel key={`${id}:${workspace.root_path}`} workspaceId={id} rootPath={workspace.root_path} readOnly={workspace.status === "archived"} onEdit={handleOpenEdit} onShowChanges={() => setShowGit(true)} />}
          {tab === "pages" && <WorkspacePagesPanel key={id} workspace={workspace} onTaskCreated={() => void reload()} renderResource={file => <ResourceCard file={file} />} />}
          {tab === "artifacts" && <div className="workspace-results-scroll scrollbar-thin"><WorkspaceArtifacts workspaceId={id} includeStudy /></div>}
          {tab === "tasks" && <>
            <div className="workspace-tasks-toolbar"><Tabs tabs={[{ id: "goals", label: "目标任务" }, { id: "records", label: "任务记录" }]} activeId={taskView} onChange={setTaskView} aria-label="工作空间任务类型" />{taskView === "records" && <Button size="sm" onClick={() => { setCreateError(null); setShowCreate(true); }} disabled={workspace.status === "archived"}><Plus size={15} />新建任务</Button>}</div>
            {taskView === "goals" ? <WorkspaceGoalTasks key={`${id}:${searchParams.get("graph") ?? ""}`} workspaceId={id} initialGraphId={searchParams.get("graph") ?? ""} onSelectGraph={graphId => setSearchParams(current => { const next = new URLSearchParams(current); next.set("tab", "tasks"); next.set("graph", graphId); return next; }, { replace: true })} /> : (
              <Panel padding={false} className="workspace-task-list-panel">
                <div className="workspace-task-list scrollbar-thin" role="region" aria-label="工作空间任务列表">
                  {taskError ? <ErrorState title="任务列表暂时无法加载" description={taskError} onRetry={() => void reload()} />
                    : tasks === null ? <div className="space-y-3 p-4" role="status" aria-label="正在读取任务记录"><Skeleton className="h-12 w-full" /><Skeleton className="h-12 w-full" /><Skeleton className="h-12 w-full" /></div>
                    : tasks.length === 0 ? <EmptyState icon={<FolderOpen size={22} />} title="还没有任务记录" />
                    : tasks.map(task => (
                      <ListRow
                        key={task.id}
                        role="button"
                        onSelect={() => setSelectedId(task.id)}
                        title={deriveTaskDisplayTitle(task.title)}
                        meta={formatTaskDate(task.updated_at) || "时间未知"}
                        trailing={<Badge tone={getTaskStatusTone(task.status)}>{getTaskStatusLabel(task.status)}</Badge>}
                        className="min-h-[64px]"
                      />
                    ))}
                </div>
              </Panel>
            )}
          </>}
        </div>
      </>}
      {showServiceSettings && workspace && <WorkspaceServiceOverridesDialog initialWorkspace={workspace} onClose={() => setShowServiceSettings(false)} />}
      <Modal open={selectedTask !== null} onClose={() => {
        setSelectedId(null);
        if (searchParams.has("task")) setSearchParams(current => { const next = new URLSearchParams(current); next.delete("task"); return next; }, { replace: true });
      }} title="任务详情" size="xl">
        {selectedTask && <TaskDetailPanel task={selectedTask} workspaceName={workspace?.name} onChanged={() => void reload()} />}
      </Modal>
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
          {createError && <p className="text-sm text-[var(--danger-fg)]" role="alert">{createError}</p>}
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
          <WorkspaceFolderField value={editRootPath} onChange={setEditRootPath} disabled={updating} />
          <FormField label="空间名称" required error={updateError || undefined}>
            <Input
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
              placeholder="例如：主开发工作区"
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
          <Select label="项目阶段" value={editProject.stage ?? ""} disabled={updating}
            options={[{ value: "", label: "未记录" }, ...Object.entries(WORKSPACE_PROJECT_STAGES).map(([value, label]) => ({ value, label }))]}
            onChange={event => setEditProject(project => ({ ...project, stage: (event.target.value || null) as WorkspaceProject["stage"] }))} />
          <div className="grid gap-3 sm:grid-cols-2">
            <FormField label="前端进度">{props => <Textarea {...props} rows={3} maxLength={1000} value={editProject.frontend} disabled={updating}
              onChange={event => setEditProject(project => ({ ...project, frontend: event.target.value }))} />}</FormField>
            <FormField label="后端进度">{props => <Textarea {...props} rows={3} maxLength={1000} value={editProject.backend} disabled={updating}
              onChange={event => setEditProject(project => ({ ...project, backend: event.target.value }))} />}</FormField>
          </div>
        </div>
      </Modal>

      {/* Archive Workspace Confirmation */}
      <ConfirmDialog
        open={showDeleteConfirm}
        title="归档工作空间"
        description="工作空间会移到「已归档」，可以随时恢复。任务历史和本地文件会保留。"
        confirmLabel="确认归档"
        tone="danger"
        loading={deleting}
        onConfirm={() => void handleDelete()}
        onCancel={() => setShowDeleteConfirm(false)}
      />
    </PageShell>
  );
}
