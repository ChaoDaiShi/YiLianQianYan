import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { FolderKanban, Plus } from "lucide-react";
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
  listWorkspaces,
  createWorkspace,
} from "../api/client";

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
    <div className="flex h-full flex-col">
      <PageHeader
        title="工作空间"
        description="创建项目工作空间，组织任务、多智能体协作与产物。"
        actions={
          <Button onClick={() => setShowCreate(true)}>
            <Plus className="h-4 w-4" />
            创建工作空间
          </Button>
        }
      />

      <div className="mx-4 mt-4 flex-1 overflow-y-auto pb-4">
        {error && <p className="mb-3 text-sm text-[var(--danger)]">{error}</p>}
        {workspaces === null ? (
          <div className="flex justify-center py-12">
            <Spinner />
          </div>
        ) : workspaces.length === 0 ? (
          <EmptyState
            icon={<FolderKanban className="h-6 w-6" />}
            title="还没有工作空间"
            description="点击右上角创建第一个工作空间。"
          />
        ) : (
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {workspaces.map((workspace) => (
              <Link key={workspace.id} to={`/workspaces/${workspace.id}`}>
                <Panel className="h-full cursor-pointer transition-colors hover:border-[var(--accent)]">
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
                  <p className="mt-3 text-xs text-[var(--text-faint)]">
                    进行中任务：{workspace.active_tasks ?? 0}
                  </p>
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
