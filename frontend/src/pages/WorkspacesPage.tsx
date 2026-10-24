import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { FolderOpen, Plus, Clipboard, ExternalLink, MoreHorizontal } from "lucide-react";
import {
  Button,
  Input,
  Textarea,
  Modal,
  EmptyState,
  ErrorState,
  Skeleton,
  showToast,
} from "../components/ui";
import PageShell from "../components/layout/PageShell";
import {
  type Workspace,
  listWorkspaces,
  createWorkspace,
} from "../api/client";
import { formatWorkspacePath } from "../features/tasks/workspacePresentation";
import { formatTaskDate } from "../features/tasks/taskPresentation";
import WorkspaceArtifacts from "../features/workspace-preview/WorkspaceArtifacts";

export default function WorkspacesPage() {
  const [workspaces, setWorkspaces] = useState<Workspace[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [rootPath, setRootPath] = useState("");
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  const reload = useCallback(async () => {
    const res = await listWorkspaces();
    if (res.ok) {
      setWorkspaces(res.data);
      setError(null);
    } else {
      setError(res.error);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const handleOpenCreate = () => {
    setName("");
    setDescription("");
    setRootPath("");
    setCreateError(null);
    setShowCreate(true);
  };

  const handleCreate = async () => {
    if (!name.trim()) return;
    setCreating(true);
    setCreateError(null);
    const res = await createWorkspace({
      name: name.trim(),
      description: description.trim() || undefined,
      root_path: rootPath.trim() || undefined,
    });
    setCreating(false);
    if (res.ok) {
      setShowCreate(false);
      setName("");
      setDescription("");
      setRootPath("");
      void reload();
    } else {
      setCreateError(res.error);
    }
  };

  const handleCopyPath = async (e: React.MouseEvent, id: string, path: string) => {
    e.preventDefault();
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(path);
      setCopiedId(id);
      window.setTimeout(() => setCopiedId((current) => (current === id ? null : current)), 1500);
    } catch {
      showToast("路径复制失败，请从空间详情查看完整路径", "error");
    }
  };

  const filteredWorkspaces = (workspaces || []).filter((w) => {
    if (!query.trim()) return true;
    const q = query.toLowerCase();
    return (
      w.name.toLowerCase().includes(q) ||
      (w.description && w.description.toLowerCase().includes(q)) ||
      (w.root_path && w.root_path.toLowerCase().includes(q))
    );
  });

  return (
    <PageShell
      title="工作空间"
      description="把对话、资料和成果放在一起。"
      variant="workspace"
      maxWidth="none"
      className="page-canvas ui-opt-workspaces workspace-preview-page"
      actions={
        <Button onClick={handleOpenCreate}>
          <Plus className="h-4 w-4" />
          新建空间
        </Button>
      }
    >
      <div className="workspace-overview-scroll scrollbar-thin"><div className="workspace-overview-content">
        <Input aria-label="搜索工作空间" value={query} onChange={event => setQuery(event.target.value)} placeholder="搜索工作空间" className="workspace-search" />

        {error && (
          <ErrorState
            className="mb-3"
            title="工作空间暂时无法加载"
            description={error}
            action={
              <Button size="sm" variant="secondary" onClick={() => void reload()}>
                重新加载
              </Button>
            }
          />
        )}

        <section aria-label="我的工作空间">
          <div className="workspace-section-heading"><h2>我的工作空间</h2>{workspaces && <span>{query ? filteredWorkspaces.length : workspaces.length} 个空间</span>}</div>
        {workspaces === null && error ? null : workspaces === null ? (
          <div className="workspace-folder-list workspace-list-loading">
            {Array.from({ length: 4 }).map((_, index) => (
              <div key={index} className="workspace-folder-row">
                <div className="flex items-center gap-3">
                  <Skeleton className="h-10 w-10 rounded-md" />
                  <div className="flex-1 space-y-2">
                    <Skeleton className="h-5 w-2/3" />
                    <Skeleton className="h-3 w-1/3" />
                  </div>
                </div>
              </div>
            ))}
          </div>
        ) : workspaces?.length === 0 ? (
          <EmptyState
            icon={<FolderOpen className="h-6 w-6" />}
            title="还没有工作空间"
            description="选择一个工作目录后，小昔涟可以在其中读取和处理文件。"
            action={
              <Button onClick={handleOpenCreate}>
                <Plus className="h-4 w-4" />
                创建工作空间
              </Button>
            }
          />
        ) : filteredWorkspaces.length === 0 ? (
          <EmptyState
            variant="search"
            title="没有匹配的工作空间"
            description="尝试调整搜索词。"
            action={
              <Button variant="secondary" size="sm" onClick={() => setQuery("")}>
                清除筛选
              </Button>
            }
          />
        ) : (
          <div className="workspace-folder-list">
            {filteredWorkspaces.map((workspace) => (
              <article key={workspace.id} className="workspace-folder-row">
                <span className="workspace-list-folder"><FolderOpen size={30} strokeWidth={1.5} /></span>
                <div className="workspace-row-copy"><h3><Link to={`/workspaces/${workspace.id}`}>{workspace.name}</Link>{workspace.status === "archived" && <span className="workspace-archived-label">已归档</span>}</h3><p title={workspace.root_path || undefined}>{workspace.description || formatWorkspacePath(workspace.root_path) || "未绑定本地目录"}</p></div>
                <span className="workspace-row-date">{workspace.updated_at ? `更新于 ${formatTaskDate(workspace.updated_at)}` : ""}</span>
                <Link to={`/workspaces/${workspace.id}`} className="workspace-open-link"><ExternalLink size={16} />打开</Link>
                <details className="workspace-row-menu"><summary aria-label={`管理 ${workspace.name}`}><MoreHorizontal size={19} /></summary><div><Link to={`/workspaces/${workspace.id}?edit=1`}>编辑空间</Link>{workspace.root_path && <button type="button" onClick={event => void handleCopyPath(event, workspace.id, workspace.root_path!)}><Clipboard size={14} />{copiedId === workspace.id ? "已复制" : "复制目录路径"}</button>}</div></details>
              </article>
            ))}
          </div>
        )}
        </section>
        <WorkspaceArtifacts limit={6} />
      </div></div>

      <Modal open={showCreate} onClose={() => { setShowCreate(false); setCreateError(null); }} title="创建工作空间">
        <div className="space-y-3">
          {createError && (
            <p className="text-xs text-[var(--danger-fg)]" role="alert">
              {createError}
            </p>
          )}
          <Input
            label="名称"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="例如：YiLianQianYan 开发"
            autoFocus
          />
          <Textarea
            label="描述"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="可选描述信息"
            rows={3}
          />
          <Input
            label="本地根目录（可选）"
            value={rootPath}
            onChange={(e) => setRootPath(e.target.value)}
            placeholder="例如：C:\projects\my-app"
          />
          <div className="flex justify-end gap-2 pt-2">
            <Button variant="ghost" onClick={() => { setShowCreate(false); setCreateError(null); }}>
              取消
            </Button>
            <Button onClick={handleCreate} disabled={!name.trim() || creating} loading={creating}>
              创建
            </Button>
          </div>
        </div>
      </Modal>
    </PageShell>
  );
}
