import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { Clock3, FolderKanban, Plus } from "lucide-react";
import {
  Button,
  Badge,
  Input,
  Textarea,
  Modal,
  PageHeader,
  Panel,
  EmptyState,
  ErrorState,
  Skeleton,
} from "../components/ui";
import {
  Workspace,
  listWorkspaces,
  createWorkspace,
} from "../api/client";
import { formatWorkspacePath } from "../features/tasks/workspacePresentation";

export default function WorkspacesPage() {
  const [workspaces, setWorkspaces] = useState<Workspace[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [creating, setCreating] = useState(false);

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

  const handleCreate = async () => {
    if (!name.trim()) return;
    setCreating(true);
    const res = await createWorkspace({ name, description });
    setCreating(false);
    if (res.ok) {
      setShowCreate(false);
      setName("");
      setDescription("");
      void reload();
    } else {
      setError(res.error);
    }
  };

  return (
    <div className="page-canvas flex h-full flex-col">
      <PageHeader
        title="工作空间"
        description="查看任务使用的文件与上下文。"
        actions={
          <Button onClick={() => setShowCreate(true)}>
            <Plus className="h-4 w-4" />
            创建工作空间
          </Button>
        }
      />

      <div className="scrollbar-thin min-h-0 flex-1 overflow-y-auto px-4 pb-4 pt-4">
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
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {Array.from({ length: 4 }).map((_, index) => (
              <Panel key={index} className="space-y-3">
                <Skeleton className="h-5 w-2/3" />
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-3 w-1/2" />
              </Panel>
            ))}
          </div>
        ) : workspaces.length === 0 ? (
          <EmptyState
            icon={<FolderKanban className="h-6 w-6" />}
            title="还没有工作空间"
            description="选择一个工作目录后，小昔涟可以在其中读取和处理文件。"
            action={
              <Button onClick={() => setShowCreate(true)}>
                <Plus className="h-4 w-4" />
                创建工作空间
              </Button>
            }
          />
        ) : (
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {workspaces.map((workspace) => (
              <Link key={workspace.id} to={`/workspaces/${workspace.id}`}>
                <Panel className="h-full cursor-pointer transition-[background-color,border-color] hover:border-[var(--accent-border)] hover:bg-[var(--surface-hover)]">
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <FolderKanban className="h-4 w-4 text-[var(--accent)]" />
                      <h3 className="font-medium text-[var(--text)]">{workspace.name}</h3>
                    </div>
                    <Badge tone={workspace.status === "active" ? "success" : "default"}>
                      {workspace.status === "active" ? "活跃" : "已归档"}
                    </Badge>
                  </div>
                  {workspace.description && (
                    <p className="mt-2 line-clamp-2 text-sm text-[var(--text-muted)]">
                      {workspace.description}
                    </p>
                  )}
                  <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-[var(--text-faint)]">
                    {formatWorkspacePath(workspace.root_path) && (
                      <span className="max-w-full truncate font-mono" title={workspace.root_path || undefined}>
                        {workspace.root_path}
                      </span>
                    )}
                    {workspace.active_tasks !== undefined && (
                      <span className="inline-flex items-center gap-1">
                        <Clock3 className="h-3 w-3" />
                        进行中 {workspace.active_tasks}
                      </span>
                    )}
                  </div>
                </Panel>
              </Link>
            ))}
          </div>
        )}
      </div>

      <Modal open={showCreate} onClose={() => setShowCreate(false)} title="创建工作空间">
        <div className="space-y-3">
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
            placeholder="可选"
            rows={3}
          />
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setShowCreate(false)}>
              取消
            </Button>
            <Button onClick={handleCreate} disabled={!name.trim() || creating}>
              {creating ? "创建中…" : "创建"}
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
}
