import { useEffect, useRef, useState } from "react";
import { useLocation, useNavigate, useSearchParams } from "react-router-dom";
import { GitFork, Plus, RefreshCw } from "lucide-react";
import { createTaskGraph, listTaskGraphs, type ApiError, type TaskGraphSummary } from "../../api/taskWorld";
import { Button, ConfirmDialog, EmptyState, ErrorState, Input, Spinner } from "../../components/ui";
import { createEmptyTaskGraph, stopAndDeleteHistoricalGraph } from "./taskGraphActions";
import { taskGraphPath } from "./taskGraphRoutes";
import TaskGraphPlanner from "./TaskGraphPlanner";

export default function TaskGraphWorkflowList() {
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const origin = useLocation().state?.attentionGoal;
  const sourceGoal = params.get("mode") === "goal" && typeof origin?.goal === "string"
    && typeof origin?.source?.automation_id === "string" && typeof origin?.source?.observation_id === "string"
    ? origin : undefined;
  const [planning, setPlanning] = useState(false);
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<ApiError | null>(null);
  const creationInFlight = useRef(false);
  const [graphs, setGraphs] = useState<TaskGraphSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");
  const [busy, setBusy] = useState(false);
  const [selected, setSelected] = useState<string[]>([]);
  const [deleting, setDeleting] = useState<TaskGraphSummary[]>([]);
  const [deleteError, setDeleteError] = useState("");
  const [deletedCount, setDeletedCount] = useState(0);
  const [search, setSearch] = useState("");
  const query = search.trim().toLowerCase();
  const matches = graphs.filter(graph => graph.id.toLowerCase().includes(query) || graph.preview_node_title?.toLowerCase().includes(query));

  async function reload() {
    setLoading(true);
    setLoadError("");
    try {
      const result = await listTaskGraphs();
      if (result.ok) {
        setGraphs(result.data);
        setSelected(current => current.filter(id => result.data.some(graph => graph.id === id)));
      } else setLoadError(result.error.message);
    } catch (cause) {
      setLoadError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => { void reload(); }, []);

  const closePlanner = () => {
    setPlanning(false);
    if (params.get("mode") === "goal") {
      const next = new URLSearchParams(params);
      next.delete("mode");
      setParams(next, { replace: true });
    }
  };

  async function remove() {
    setBusy(true);
    setDeleteError("");
    setDeletedCount(0);
    const removed: string[] = [];
    const errors: string[] = [];
    for (const graph of deleting) {
      try {
        await stopAndDeleteHistoricalGraph(graph.id);
        removed.push(graph.id);
      } catch (cause) {
        errors.push(`${graph.id} · ${cause instanceof Error ? cause.message : String(cause)}`);
      }
    }
    setGraphs(current => current.filter(graph => !removed.includes(graph.id)));
    setSelected(current => current.filter(id => !removed.includes(id)));
    setDeleting([]);
    setDeletedCount(removed.length);
    setDeleteError(errors.join("\n"));
    setBusy(false);
  }

  function create() {
    void createEmptyTaskGraph({ createGraph: createTaskGraph, navigate, setCreating, setError: setCreateError, inFlight: creationInFlight, makeGraphId: () => crypto.randomUUID() });
  }

  return <section className="flex min-h-0 min-w-0 flex-1 flex-col gap-3" aria-label="任务画布列表">
    <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-[var(--border-soft)] pb-3">
      <div className="min-w-0 flex-[1_1_175px]">
        <Input type="search" aria-label="搜索任务画布" placeholder="搜索首个节点或画布 ID" value={search} onChange={event => setSearch(event.target.value)} />
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" variant="ghost" disabled={busy || loading} onClick={() => void reload()}><RefreshCw size={14} aria-hidden="true" />刷新</Button>
        <Button size="sm" variant="secondary" disabled={busy || creating} onClick={() => setPlanning(true)}><GitFork size={14} aria-hidden="true" />规划任务画布</Button>
        <Button size="sm" loading={creating} loadingLabel="正在创建…" disabled={busy} onClick={create}><Plus size={14} aria-hidden="true" />新建任务画布</Button>
      </div>
    </div>
    {selected.length > 0 && <div className="flex shrink-0 flex-wrap items-center gap-2">
      <span className="mr-auto text-xs text-[var(--text-secondary)]">已选 {selected.length} 个画布</span>
      <Button size="sm" variant="ghost" disabled={busy} onClick={() => setSelected([])}>取消选择</Button>
      <Button size="sm" variant="danger" disabled={busy || loading} onClick={() => setDeleting(graphs.filter(graph => selected.includes(graph.id)))}>删除所选画布</Button>
    </div>}
    <div className="min-h-0 flex-1 space-y-2 overflow-y-auto p-1" aria-busy={loading}>
      {loadError && <ErrorState title="任务画布读取失败" description={loadError} onRetry={() => void reload()} />}
      {deleteError && <ErrorState title="任务画布删除未完成" description={deleteError} className="whitespace-pre-wrap break-words" />}
      {createError && <ErrorState title="任务画布创建失败" description={createError.message} />}
      {deletedCount > 0 && <p role="status" className="text-sm text-[var(--text-secondary)]">已删除 {deletedCount} 个任务画布。</p>}
      {loading && <div role="status" className="flex items-center justify-center gap-2 py-6 text-sm text-[var(--text-secondary)]"><Spinner />正在读取任务画布…</div>}
      {!loading && !loadError && graphs.length === 0 && <EmptyState title="还没有任务画布" description="新建空白画布，或描述任务目标生成规划。" />}
      {!loading && !loadError && graphs.length > 0 && matches.length === 0 && <EmptyState title="未找到匹配的画布" />}
      {matches.map(graph => <article key={graph.id} className="glass-card flex flex-wrap items-center gap-3 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-3">
        <input type="checkbox" className="focus-ring-token h-4 w-4 shrink-0" aria-label={`选择任务画布 ${graph.id}`} checked={selected.includes(graph.id)} disabled={busy} onChange={event => {
          const checked = event.target.checked;
          setSelected(current => checked ? [...current, graph.id] : current.filter(id => id !== graph.id));
        }} />
        <div className="min-w-0 flex-[1_1_180px]">
          <h3 className="break-words text-sm font-semibold">{graph.preview_node_title ? `包含「${graph.preview_node_title}」` : graph.node_count ? "任务画布" : "空白画布"}</h3>
          <p className="mt-1 text-xs text-[var(--text-secondary)]">{graph.node_count} 个节点 · {graph.edge_count} 条连线 · 修订 {graph.revision}</p>
          <p className="mt-0.5 break-all text-xs text-[var(--text-muted)]">{graph.id}</p>
        </div>
        <div className="ml-auto flex flex-wrap items-center gap-1">
          <Button size="sm" variant="ghost" disabled={busy || loading} onClick={() => setDeleting([graph])}>删除</Button>
          <Button size="sm" variant="secondary" disabled={busy} onClick={() => navigate(taskGraphPath(graph.id))}>打开画布</Button>
        </div>
      </article>)}
    </div>
    <ConfirmDialog open={deleting.length > 0} title="删除任务画布" description={`将先停止活动执行，再删除所选的 ${deleting.length} 个任务画布。`} tone="danger" confirmLabel="停止执行并删除" loading={busy}
      onCancel={() => { if (!busy) setDeleting([]); }} onConfirm={remove}>
      <ul className="max-h-28 list-inside list-disc overflow-y-auto text-xs text-[var(--text-secondary)]">
        {deleting.map(graph => <li key={graph.id} className="break-all">{graph.id}</li>)}
      </ul>
    </ConfirmDialog>
    {(planning || params.get("mode") === "goal") && <TaskGraphPlanner key={`${params.get("workspace")}:${params.get("mode")}:${sourceGoal?.source.observation_id ?? ""}`} workspaceId={params.get("workspace") || undefined} initialMode={params.get("mode") === "goal" ? "proofloop" : "standard"} initialGoal={sourceGoal?.goal} source={sourceGoal?.source} onClose={closePlanner} />}
  </section>;
}
