import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { Plus, Clipboard, MoreHorizontal, RotateCcw, Star, Trash2 } from "lucide-react";
import {
  Button,
  Input,
  Textarea,
  Modal,
  EmptyState,
  ErrorState,
  Skeleton,
  Tabs,
  ConfirmDialog,
  showToast,
} from "../components/ui";
import PageShell from "../components/layout/PageShell";
import {
  type Workspace,
  WORKSPACE_PROJECT_STAGES,
  listWorkspaces,
  createWorkspace,
  deleteWorkspace as archiveWorkspace,
  removeWorkspaceRegistration,
  restoreWorkspace,
  setDefaultWorkspace,
} from "../api/workspaces";
import { formatWorkspacePath } from "../features/tasks/workspacePresentation";
import { formatTaskDate } from "../features/tasks/taskPresentation";
import WorkspaceIcon from "../components/ui/WorkspaceIcon";
import WorkspaceFolderField from "../features/workspace-preview/WorkspaceFolderField";
import WorkspaceArtifacts from "../features/workspace-preview/WorkspaceArtifacts";

const batchActions = {
  archive: { label: "归档", apply: archiveWorkspace },
  restore: { label: "恢复", apply: restoreWorkspace },
  delete: { label: "删除", apply: removeWorkspaceRegistration },
};

type BatchAction = keyof typeof batchActions;
type BatchFailure = { id: string; name: string; reason: string };
type DeleteRequest = { kind: "single"; workspace: Workspace } | { kind: "batch"; targets: Workspace[] };

export default function WorkspacesPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const viewingArchive = searchParams.get("view") === "archived";
  const activeTab = viewingArchive ? "workspaces-archived" : "workspaces-active";
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
  const [changingDefault, setChangingDefault] = useState(false);
  const [defaultError, setDefaultError] = useState("");
  const [restoringId, setRestoringId] = useState<string | null>(null);
  const [deleteRequest, setDeleteRequest] = useState<DeleteRequest | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [selection, setSelection] = useState<{ tab: string; ids: string[] } | null>(null);
  const [selectionReview, setSelectionReview] = useState<{ clearedIds: string[] } | null>(null);
  const selectionReviewDoneRef = useRef<HTMLButtonElement>(null);
  const [batchAction, setBatchAction] = useState<BatchAction | null>(null);
  const [batchError, setBatchError] = useState<{ summary: string; failures: BatchFailure[] } | null>(null);
  const organizing = selection?.tab === activeTab;
  const batchBusy = batchAction !== null;

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
      if (viewingArchive) changeTab("workspaces-active");
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

  async function changeDefault(id: string) {
    setChangingDefault(true);
    setDefaultError("");
    const result = await setDefaultWorkspace(id);
    setChangingDefault(false);
    if (!result.ok) {
      setDefaultError(result.error);
      return;
    }
    setWorkspaces(current => current?.map(workspace => ({
      ...workspace,
      is_default: workspace.id === result.data.workspace_id,
    })) ?? null);
  }

  async function handleRestore(workspace: Workspace) {
    setRestoringId(workspace.id);
    const result = await restoreWorkspace(workspace.id);
    setRestoringId(null);
    if (!result.ok) {
      showToast(result.error, "error");
      return;
    }
    showToast("工作空间已恢复到使用中", "success");
    await reload();
  }

  async function handleDelete() {
    if (!deleteRequest) return;
    if (deleteRequest.kind === "batch") {
      await runBatch("delete", deleteRequest.targets);
      return;
    }
    const { workspace } = deleteRequest;
    setDeleting(true);
    const result = await removeWorkspaceRegistration(workspace.id);
    setDeleting(false);
    if (!result.ok) {
      showToast(result.error, "error");
      return;
    }
    setWorkspaces(current => current?.filter(item => item.id !== workspace.id) ?? null);
    setDeleteRequest(null);
    showToast("工作空间已删除", "success");
  }

  function changeTab(tab: string) {
    if (tab === activeTab) return;
    setSelection(null);
    setSelectionReview(null);
    setBatchError(null);
    setSearchParams(tab === "workspaces-archived" ? { view: "archived" } : {});
  }

  function updateSelection(ids: string[]) {
    setSelection({ tab: activeTab, ids });
  }

  async function runBatch(action: BatchAction, targets: Workspace[]) {
    setBatchAction(action);
    setBatchError(null);
    const { apply, label } = batchActions[action];
    const succeeded = new Set<string>();
    const failures: BatchFailure[] = [];
    for (const workspace of targets) {
      const result = await apply(workspace.id);
      if (result.ok) {
        succeeded.add(workspace.id);
      } else {
        failures.push({ id: workspace.id, name: workspace.name, reason: result.error });
      }
    }

    setWorkspaces(current => current?.flatMap(workspace => {
      if (!succeeded.has(workspace.id)) return [workspace];
      if (action === "delete") return [];
      return [{ ...workspace, status: action === "archive" ? "archived" : "active", is_default: false }];
    }) ?? null);
    updateSelection(failures.map(item => item.id));
    if (action === "delete") setDeleteRequest(null);

    if (failures.length) {
      setBatchError({ summary: `${label}成功 ${succeeded.size} 个，失败 ${failures.length} 个`, failures });
    } else {
      showToast(`已${label} ${succeeded.size} 个工作空间`, "success");
    }
    await reload();
    setBatchAction(null);
  }

  const activeWorkspaces = (workspaces ?? []).filter(workspace => workspace.status === "active");
  const archivedWorkspaces = (workspaces ?? []).filter(workspace => workspace.status === "archived");
  const availableWorkspaces = viewingArchive ? archivedWorkspaces : activeWorkspaces;
  const q = query.trim().toLowerCase();
  const filteredWorkspaces = availableWorkspaces.filter(w => {
    if (!q) return true;
    return (
      w.name.toLowerCase().includes(q) ||
      (w.description && w.description.toLowerCase().includes(q)) ||
      (w.root_path && w.root_path.toLowerCase().includes(q))
    );
  });
  const orderedWorkspaces = [...filteredWorkspaces].sort((first, second) => Number(Boolean(second.is_default)) - Number(Boolean(first.is_default)));
  const selectedIds = new Set(organizing ? selection.ids : []);
  const selectedWorkspaces = availableWorkspaces.filter(workspace => selectedIds.has(workspace.id));
  const filteredIds = new Set(filteredWorkspaces.map(workspace => workspace.id));
  const visibleSelectedCount = selectedWorkspaces.filter(workspace => filteredIds.has(workspace.id)).length;
  const hiddenSelectedCount = selectedWorkspaces.length - visibleSelectedCount;
  const allSelected = filteredIds.size > 0 && visibleSelectedCount === filteredIds.size;
  const batchDelete = deleteRequest?.kind === "batch";
  const deleteTarget = batchDelete ? `所选 ${deleteRequest.targets.length} 个工作空间` : `「${deleteRequest?.workspace.name ?? ""}」`;

  const renderWorkspace = (workspace: Workspace) => {
    const path = `/workspaces/${encodeURIComponent(workspace.id)}`;
    const folder = formatWorkspacePath(workspace.root_path);
    const showShortcuts = !organizing && workspace.status === "active";
    return (
    <article key={workspace.id} className="workspace-project-card" data-default={workspace.is_default || undefined} data-selected={selectedIds.has(workspace.id) || undefined}>
      <div className="workspace-project-card-header">
        <span className="workspace-project-card-icon" aria-hidden="true"><WorkspaceIcon name="workspace-switch" size={22} variant="mono" /></span>
        <div className="workspace-project-card-heading">
          <h3>{organizing ? workspace.name : <Link to={path}>{workspace.name}</Link>}</h3>
          {(workspace.is_default || workspace.project?.stage) && <div className="workspace-project-card-tags">
            {workspace.is_default && <span className="workspace-default-label"><Star size={12} aria-hidden="true" />默认工作空间</span>}
            {workspace.project?.stage && <span className="workspace-project-stage">{WORKSPACE_PROJECT_STAGES[workspace.project.stage]}</span>}
          </div>}
        </div>
        {organizing ? <input
          type="checkbox"
          className="workspace-select-checkbox"
          aria-label={"选择 " + workspace.name}
          checked={selectedIds.has(workspace.id)}
          disabled={batchBusy}
          onChange={event => updateSelection(event.target.checked
            ? [...selectedIds, workspace.id]
            : [...selectedIds].filter(id => id !== workspace.id))}
        /> : <details className="workspace-row-menu">
        <summary aria-label={"管理 " + workspace.name}><MoreHorizontal size={19} /></summary>
        <div>
          <Link to={`${path}?edit=1`}>编辑空间</Link>
          {workspace.status === "active" && !workspace.is_default && <button type="button" disabled={changingDefault}
            onClick={() => void changeDefault(workspace.id)}>
            <Star size={14} />设为默认工作空间
          </button>}
          {folder && <button type="button" onClick={event => void handleCopyPath(event, workspace.id, folder)}>
            <Clipboard size={14} />{copiedId === workspace.id ? "已复制" : "复制目录路径"}
          </button>}
          {workspace.status === "archived" && <>
            <button type="button" disabled={restoringId !== null} onClick={() => void handleRestore(workspace)}>
              <RotateCcw size={14} />{restoringId === workspace.id ? "恢复中…" : "恢复空间"}
            </button>
            <button type="button" className="workspace-delete-action" disabled={restoringId === workspace.id} onClick={() => setDeleteRequest({ kind: "single", workspace })}>
              <Trash2 size={14} />删除空间
            </button>
          </>}
        </div>
        </details>}
      </div>
      <div className="workspace-project-card-context">
        <p className="workspace-project-card-path" title={folder ?? undefined}>{folder ?? "未绑定本地目录"}</p>
        {workspace.description && <p className="workspace-project-card-description" title={workspace.description}>{workspace.description}</p>}
      </div>
      {(showShortcuts || workspace.updated_at > 0) && <div className="workspace-project-card-footer">
        {showShortcuts && <nav aria-label={`${workspace.name} 快捷入口`}>
          <Link to={`${path}?tab=files`}>文件</Link>
          <Link to={`${path}/learning`}>学习</Link>
          <Link to={`${path}/creation`}>创作</Link>
        </nav>}
        {workspace.updated_at > 0 && <span className="workspace-project-card-date">更新于 {formatTaskDate(workspace.updated_at)}</span>}
      </div>}
    </article>
    );
  };

  return (
    <PageShell
      title="工作空间"
      variant="workspace"
      maxWidth="none"
      className="page-canvas ui-opt-workspaces workspace-preview-page"
      actions={
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="secondary" disabled={batchBusy || restoringId !== null || changingDefault || deleting || creating || (!organizing && availableWorkspaces.length === 0)}
            onClick={() => {
              setSelection(organizing ? null : { tab: activeTab, ids: [] });
              setBatchError(null);
            }}>
            {organizing ? "完成整理" : "批量整理"}
          </Button>
          <Button onClick={handleOpenCreate} disabled={batchBusy}>
            <Plus className="h-4 w-4" />
            新建空间
          </Button>
        </div>
      }
    >
      <div className="workspace-overview-scroll scrollbar-thin"><div className="workspace-overview-content">
        <div className="workspace-overview-toolbar">
        <Tabs
          tabs={[
            { id: "workspaces-active", label: "使用中", badge: workspaces && <span aria-hidden="true">{activeWorkspaces.length}</span>, panelId: "workspaces-panel", disabled: batchBusy },
            { id: "workspaces-archived", label: "已归档", badge: workspaces && <span aria-hidden="true">{archivedWorkspaces.length}</span>, panelId: "workspaces-panel", disabled: batchBusy },
          ]}
          activeId={activeTab}
          onChange={changeTab}
          aria-label="工作空间状态"
          variant="segment"
        />
        <div className="workspace-overview-tools">
          <Input aria-label="搜索工作空间" value={query} onChange={event => setQuery(event.target.value)} placeholder="搜索工作空间" className="workspace-search" disabled={batchBusy} />
          <Link to="/workspaces?view=outputs" className="workspace-global-outputs">跨空间成果</Link>
        </div>
        </div>
        {organizing && <div className="workspace-batch-toolbar" role="group" aria-label="工作空间批量操作">
          <div role="status" className="workspace-batch-count">
            <span>已选 {selectedWorkspaces.length} 个</span>
            {hiddenSelectedCount > 0 && <span className="ml-2">其中 {hiddenSelectedCount} 个不在当前结果</span>}
          </div>
          <Button variant="ghost" size="sm" aria-haspopup="dialog" disabled={batchBusy}
            onClick={() => setSelectionReview({ clearedIds: [] })}>查看已选</Button>
          <Button variant="ghost" size="sm" disabled={batchBusy || filteredWorkspaces.length === 0}
            onClick={() => updateSelection(allSelected
              ? [...selectedIds].filter(id => !filteredIds.has(id))
              : [...new Set([...selectedIds, ...filteredIds])])}>
            {allSelected ? "取消全选当前结果" : "全选当前结果"}
          </Button>
          {viewingArchive ? <>
            <Button variant="secondary" size="sm" disabled={batchBusy || selectedWorkspaces.length === 0} loading={batchAction === "restore"}
              onClick={() => void runBatch("restore", selectedWorkspaces)}>批量恢复</Button>
            <Button variant="danger" size="sm" disabled={batchBusy || selectedWorkspaces.length === 0} loading={batchAction === "delete"}
              onClick={() => setDeleteRequest({ kind: "batch", targets: selectedWorkspaces })}>批量删除</Button>
          </> : <Button variant="secondary" size="sm" disabled={batchBusy || selectedWorkspaces.length === 0} loading={batchAction === "archive"}
            onClick={() => void runBatch("archive", selectedWorkspaces)}>批量归档</Button>}
        </div>}
        {organizing && batchError && <div role="alert" className="space-y-1 break-words text-sm text-[var(--danger-fg)]">
          <p>{batchError.summary}</p>
          <ul className="list-disc pl-5">
            {batchError.failures.map(item => <li key={item.id}>{item.name} · {item.reason}</li>)}
          </ul>
        </div>}

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

        {defaultError && <p className="text-sm text-[var(--danger-fg)]" role="alert">{defaultError}</p>}
        <section role="tabpanel" id="workspaces-panel" aria-labelledby={`tab-${activeTab}`}>
        {workspaces === null && error ? null : workspaces === null ? (
          <div className="workspace-project-grid" aria-busy="true">
            {Array.from({ length: 4 }).map((_, index) => (
              <div key={index} className="workspace-project-card">
                <Skeleton className="h-8 w-8 rounded-md" />
                <Skeleton className="h-5 w-2/3" />
                <Skeleton className="h-3 w-1/3" />
              </div>
            ))}
          </div>
        ) : availableWorkspaces.length === 0 ? (
          <EmptyState
            icon={<WorkspaceIcon name="workspace-switch" size={40} />}
            title={viewingArchive ? "还没有归档的工作空间" : "还没有使用中的工作空间"}
          />
        ) : filteredWorkspaces.length === 0 ? (
          <EmptyState
            variant="search"
            title="没有匹配的工作空间"
            description="尝试调整搜索词。"
            action={
              <Button variant="secondary" size="sm" disabled={batchBusy} onClick={() => setQuery("")}>
                清除筛选
              </Button>
            }
          />
        ) : (
          <div className="workspace-project-grid">{orderedWorkspaces.map(renderWorkspace)}</div>
        )}
        </section>
      </div></div>

      <Modal open={searchParams.get("view") === "outputs"} onClose={() => setSearchParams({}, { replace: true })} title="跨空间成果" size="xl">
        <WorkspaceArtifacts limit={100} embeddedPreview />
      </Modal>

      <Modal
        open={organizing && selectionReview !== null}
        onClose={() => setSelectionReview(null)}
        title="已选工作空间"
        size="md"
        footer={<>
          <Button variant="ghost" disabled={selectedWorkspaces.length === 0 && !selectionReview?.clearedIds.length}
            onClick={() => {
              if (selectionReview?.clearedIds.length) {
                updateSelection(selectionReview.clearedIds);
                setSelectionReview({ clearedIds: [] });
              } else {
                setSelectionReview({ clearedIds: selectedWorkspaces.map(workspace => workspace.id) });
                updateSelection([]);
              }
            }}>
            {selectionReview?.clearedIds.length ? "撤销清空" : "清空已选"}
          </Button>
          <Button ref={selectionReviewDoneRef} onClick={() => setSelectionReview(null)}>完成</Button>
        </>}
      >
        <p role="status" className="mb-3 text-sm text-[var(--text-secondary)]">已选 {selectedWorkspaces.length} 个</p>
        {selectedWorkspaces.length === 0 ? (
          <p className="text-sm text-[var(--text-secondary)]">还没有选中的工作空间</p>
        ) : (
          <ul aria-label="已选工作空间" className="divide-y divide-[var(--border-soft)]">
            {selectedWorkspaces.map(workspace => (
              <li key={workspace.id} className="flex items-start gap-3 py-2">
                <div className="min-w-0 flex-1 break-words">
                  <p className="text-sm font-medium">{workspace.name}</p>
                  <p className="text-xs text-[var(--text-secondary)]">{formatWorkspacePath(workspace.root_path) || "未绑定本地目录"}</p>
                  {!filteredIds.has(workspace.id) && <span className="text-xs text-[var(--text-muted)]">不在当前结果</span>}
                </div>
                <Button variant="ghost" size="sm" className="shrink-0" aria-label={`取消勾选 ${workspace.name}`}
                  onClick={event => {
                    const row = event.currentTarget.closest("li");
                    const neighbor = row?.nextElementSibling ?? row?.previousElementSibling;
                    (neighbor?.querySelector("button") ?? selectionReviewDoneRef.current)?.focus();
                    updateSelection([...selectedIds].filter(id => id !== workspace.id));
                  }}>取消勾选</Button>
              </li>
            ))}
          </ul>
        )}
      </Modal>

      <ConfirmDialog
        open={deleteRequest !== null}
        title={batchDelete ? "批量删除工作空间" : "删除工作空间"}
        description={`删除${deleteTarget}的入口。关联的本地文件夹、对话和任务历史会保留。`}
        tone="danger"
        confirmLabel={batchDelete ? "确认删除" : "删除空间"}
        loading={deleting || batchAction === "delete"}
        onConfirm={handleDelete}
        onCancel={() => { if (!deleting && !batchBusy) setDeleteRequest(null); }}
      >
        {batchDelete && <ul aria-label="将删除的工作空间" className="list-disc space-y-1 break-words pl-5 text-sm">
          {deleteRequest.targets.map(workspace => <li key={workspace.id}>{workspace.name}</li>)}
        </ul>}
      </ConfirmDialog>
      <Modal open={showCreate} onClose={() => { setShowCreate(false); setCreateError(null); }} title="创建工作空间">
        <div className="space-y-3">
          {createError && (
            <p className="text-xs text-[var(--danger-fg)]" role="alert">
              {createError}
            </p>
          )}
          <WorkspaceFolderField value={rootPath} onChange={setRootPath} disabled={creating}
            onPick={path => setName(current => current || path.split(/[\\/]/).filter(Boolean).pop() || "")} />
          <Input
            label="名称"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="例如：YiLianQianYan 开发"
          />
          <Textarea
            label="描述"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="可选描述信息"
            rows={3}
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
