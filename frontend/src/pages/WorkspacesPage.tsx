import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { Clock3, FolderKanban, Plus, Clipboard } from "lucide-react";
import {
  Button,
  Badge,
  Input,
  Textarea,
  Modal,
  Panel,
  EmptyState,
  ErrorState,
  Skeleton,
  FilterBar,
} from "../components/ui";
import PageShell from "../components/layout/PageShell";
import {
  type Workspace,
  listWorkspaces,
  createWorkspace,
} from "../api/client";
import { formatWorkspacePath } from "../features/tasks/workspacePresentation";
import { formatTaskDate } from "../features/tasks/taskPresentation";

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
      if (navigator.clipboard) {
        await navigator.clipboard.writeText(path);
        setCopiedId(id);
        window.setTimeout(() => setCopiedId((current) => (current === id ? null : current)), 1500);
      }
    } catch {
      // ignore clipboard error
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
      description="查看任务使用的文件与上下文。"
      className="page-canvas ui-opt-workspaces flex h-full flex-col"
      actions={
        <Button onClick={handleOpenCreate}>
          <Plus className="h-4 w-4" />
          创建工作空间
        </Button>
      }
    >
      <div className="scrollbar-thin min-h-0 flex-1 overflow-y-auto px-4 pb-4 pt-4 space-y-4">
        {workspaces !== null && workspaces.length > 0 && (
          <FilterBar
            search={{
              value: query,
              onChange: setQuery,
              placeholder: "搜索工作空间名称、路径...",
              onClear: () => setQuery(""),
            }}
            count={{
              total: workspaces.length,
              filtered: filteredWorkspaces.length,
              label: "个空间",
            }}
            onRefresh={() => void reload()}
          />
        )}

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

        {workspaces === null ? (
          <div className="workspace-card-grid">
            {Array.from({ length: 4 }).map((_, index) => (
              <Panel key={index} className="space-y-3 min-h-[160px]">
                <div className="flex items-center gap-3">
                  <Skeleton className="h-10 w-10 rounded-md" />
                  <div className="flex-1 space-y-2">
                    <Skeleton className="h-5 w-2/3" />
                    <Skeleton className="h-3 w-1/3" />
                  </div>
                </div>
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-6 w-full" />
              </Panel>
            ))}
          </div>
        ) : workspaces.length === 0 ? (
          <EmptyState
            icon={<FolderKanban className="h-6 w-6" />}
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
          <div className="workspace-card-grid">
            {filteredWorkspaces.map((workspace) => (
              <Link key={workspace.id} to={`/workspaces/${workspace.id}`} className="block">
                <Panel className="flex h-full min-h-[160px] flex-col justify-between cursor-pointer transition-[background-color,border-color] hover:border-[var(--accent-border)] hover:bg-[var(--surface-hover)]">
                  <div>
                    <div className="flex items-start gap-3">
                      <div className="workspace-folder-icon shrink-0">
                        <FolderKanban className="h-5 w-5" />
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="flex items-start justify-between gap-2">
                          <h3 className="truncate font-medium text-[var(--text)]" title={workspace.name}>
                            {workspace.name}
                          </h3>
                          <Badge tone={workspace.status === "active" ? "success" : "default"}>
                            {workspace.status === "active" ? "活跃" : "已归档"}
                          </Badge>
                        </div>
                        <p className="mt-0.5 text-xs text-[var(--text-faint)]">
                          {workspace.updated_at ? `更新于 ${formatTaskDate(workspace.updated_at)}` : "未记录更新时间"}
                        </p>
                      </div>
                    </div>
                    {workspace.description && (
                      <p className="mt-2.5 line-clamp-2 text-sm text-[var(--text-muted)]">
                        {workspace.description}
                      </p>
                    )}
                  </div>

                  <div className="mt-3">
                    <div className="flex items-center justify-between gap-2 rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-2.5 py-1.5 text-xs">
                      <div className="flex min-w-0 items-center gap-1.5 font-mono text-[var(--text-secondary)]">
                        <span className="truncate" title={workspace.root_path || "未绑定本地目录"}>
                          {formatWorkspacePath(workspace.root_path) || "未绑定本地目录"}
                        </span>
                      </div>
                      {formatWorkspacePath(workspace.root_path) && (
                        <button
                          type="button"
                          onClick={(e) => void handleCopyPath(e, workspace.id, workspace.root_path!)}
                          className="shrink-0 rounded p-1 text-[var(--text-faint)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] focus-ring-token"
                          aria-label="复制本地路径"
                          title="复制本地路径"
                        >
                          {copiedId === workspace.id ? (
                            <span className="font-sans text-[10px] text-[var(--success-fg)]">已复制</span>
                          ) : (
                            <Clipboard className="h-3.5 w-3.5" />
                          )}
                        </button>
                      )}
                    </div>

                    {workspace.active_tasks !== undefined && (
                      <div className="mt-2 flex items-center justify-end text-xs text-[var(--text-faint)]">
                        <span className="inline-flex items-center gap-1 text-[var(--text-secondary)]">
                          <Clock3 className="h-3 w-3" />
                          进行中 {workspace.active_tasks}
                        </span>
                      </div>
                    )}
                  </div>
                </Panel>
              </Link>
            ))}
          </div>
        )}
      </div>

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
