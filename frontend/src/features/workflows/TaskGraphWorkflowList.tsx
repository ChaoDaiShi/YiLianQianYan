import { useEffect, useState } from "react";
import { listTaskGraphs, type TaskGraphSummary } from "../../api/taskWorld";
import { Button, EmptyState, ErrorState, Spinner } from "../../components/ui";
import { stopAndDeleteHistoricalGraph } from "./taskGraphActions";
import TaskWorldPage from "../task-world/TaskWorldPage";

export default function TaskGraphWorkflowList() {
  const [openedGraphId, setOpenedGraphId] = useState<string | null>(null);
  const [graphs, setGraphs] = useState<TaskGraphSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [selected, setSelected] = useState<string[]>([]);
  const [deleting, setDeleting] = useState<TaskGraphSummary[]>([]);
  const [error, setError] = useState("");
  const [results, setResults] = useState<string[]>([]);
  async function reload() {
    setLoading(true); setError("");
    try { const result = await listTaskGraphs(); if (!result.ok) throw new Error(result.error.message); setGraphs(result.data); }
    catch (cause) { setError(String(cause)); }
    finally { setLoading(false); }
  }
  useEffect(() => { void reload(); }, []);
  async function remove() {
    setBusy(true); setError("");
    const messages: string[] = []; const errors: string[] = [];
    for (const graph of deleting) {
      try { await stopAndDeleteHistoricalGraph(graph.id); setGraphs(current => current.filter(item => item.id !== graph.id)); setSelected(current => current.filter(id => id !== graph.id)); messages.push(`已删除 ${graph.id}`); }
      catch (cause) { errors.push(`${graph.id}：${String(cause)}`); }
    }
    setDeleting([]); setResults(messages); setError(errors.join("；")); setBusy(false);
  }
  if (loading) return <div className="flex justify-center py-12"><Spinner /></div>;
  if (openedGraphId) return <div className="h-[68dvh] min-h-[440px] overflow-hidden"><TaskWorldPage graphId={openedGraphId} onBack={() => setOpenedGraphId(null)} /></div>;
  return <div className="space-y-4">
    <div className="flex flex-wrap items-center justify-between gap-3"><div><h2 className="text-sm font-medium">任务图工作流</h2><p className="text-xs text-[var(--text-muted)] mt-1">已有 TaskGraph 画布归入工作流管理，保留原始节点、执行记录与画布。</p></div><div className="flex gap-2"><Button size="sm" variant="secondary" disabled={busy} onClick={() => void reload()}>刷新任务图</Button><Button size="sm" variant="danger" disabled={busy || !selected.length} onClick={() => setDeleting(graphs.filter(graph => selected.includes(graph.id)))}>删除选中任务图（{selected.length}）</Button></div></div>
    {error && <ErrorState title="任务图操作未完成" description={error} />}
    {deleting.length > 0 && <section aria-label="确认删除任务图" className="rounded-xl border border-[var(--danger-border)] p-4 space-y-3"><p className="text-sm break-words">确定删除 {deleting.map(graph => graph.id).join("、")}？将先停止活动执行，再删除任务图及其画布。</p><div className="flex justify-end gap-2"><Button size="sm" variant="secondary" disabled={busy} onClick={() => setDeleting([])}>取消</Button><Button size="sm" variant="danger" loading={busy} onClick={() => void remove()}>停止执行并确认删除</Button></div></section>}
    {graphs.length === 0 ? <EmptyState title="暂无任务图工作流" description="任务中心中建立的任务图会显示在这里。" /> : graphs.map(graph => <article key={graph.id} className="flex flex-wrap items-center gap-3 border border-[var(--border-soft)] rounded-xl p-3"><input type="checkbox" aria-label={`选择任务图 ${graph.id}`} checked={selected.includes(graph.id)} disabled={busy} onChange={event => setSelected(current => event.target.checked ? [...current, graph.id] : current.filter(id => id !== graph.id))} /><div className="mr-auto min-w-0"><p className="text-sm break-all">{graph.id}</p><p className="text-xs text-[var(--text-muted)]">节点 {graph.node_count} · 连线 {graph.edge_count} · 版本 {graph.revision}</p></div><Button size="sm" variant="secondary" disabled={busy} onClick={() => setOpenedGraphId(graph.id)}>打开任务图</Button><Button size="sm" variant="danger" disabled={busy} onClick={() => setDeleting([graph])}>删除任务图</Button></article>)}
    {results.map(message => <p key={message} role="status" className="text-sm break-words">{message}</p>)}
  </div>;
}
