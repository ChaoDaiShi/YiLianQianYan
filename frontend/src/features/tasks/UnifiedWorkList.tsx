import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowRight, RefreshCw } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { listWorkIndex, reconcileWorkIndex, type WorkIndex, type WorkIndexItem, type WorkKind } from "../../api/workIndex";
import { subscribeToEvents } from "../../api/events";
import { Badge, Button, EmptyState, Input, Modal } from "../../components/ui";
import type { Workspace } from "../../api/client";
import { formatTaskDate } from "./taskPresentation";
import ConversationContextManifest from "../execution/ContextManifestView";
import { matchesWork, workDisplayTitle, workKey, workKindLabels, workStatusLabels, type WorkFilter } from "./workIndexPresentation";

export default function UnifiedWorkList({ workspaceId, workspaces, onOpenTask }: { workspaceId?: string; workspaces: Workspace[]; onOpenTask?: (id: string) => void }) {
  const navigate = useNavigate();
  const [reconciling, setReconciling] = useState(false);
  const reconcileBusy = useRef(false);
  const [reconcileError, setReconcileError] = useState("");
  const [details, setDetails] = useState<WorkIndexItem | null>(null);
  const [contextOpen, setContextOpen] = useState(false);
  const [data, setData] = useState<WorkIndex | null>(null);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<WorkFilter>("all");
  const [kind, setKind] = useState<WorkKind | "all">("all");
  const [revision, setRevision] = useState(0);
  const reload = useCallback(() => setRevision(value => value + 1), []);
  const epoch = useRef(0);
  useEffect(() => {
    const version = ++epoch.current;
    setData(null); setError("");
    let active = true; let pending = false;
    async function read() {
      if (pending) return;
      pending = true;
      try { const next = await listWorkIndex(workspaceId); if (active && epoch.current === version) { setData(next); setError(""); } }
      catch (cause) { if (active && epoch.current === version) setError(cause instanceof Error ? cause.message : String(cause)); }
      finally { pending = false; }
    }
    void read();
    const timer = window.setInterval(() => void read(), 5000);
    let eventTimer: ReturnType<typeof setTimeout> | undefined;
    const events = subscribeToEvents(() => { clearTimeout(eventTimer); eventTimer = setTimeout(() => void read(), 150); }, undefined,
      state => { if (state === "connected") void read(); });
    return () => { active = false; clearInterval(timer); clearTimeout(eventTimer); events.abort(); };
  }, [workspaceId, revision]);
  const visible = useMemo(() => (data?.items ?? []).filter(item => matchesWork(item, query, filter, kind)), [data, query, filter, kind]);
  const workspaceNames = useMemo(() => new Map(workspaces.map(workspace => [workspace.id, workspace.name])), [workspaces]);
  const detailsTitle = details ? workDisplayTitle(details) : "";
  function openSource(item: WorkIndexItem) {
    if (item.kind === "legacy_task" && onOpenTask) onOpenTask(item.id);
    else navigate(item.href);
  }
  async function reconcile() {
    if (reconcileBusy.current) return;
    reconcileBusy.current = true; setReconciling(true); setReconcileError("");
    try { await reconcileWorkIndex(); reload(); }
    catch (cause) { setReconcileError(cause instanceof Error ? cause.message : String(cause)); }
    finally { reconcileBusy.current = false; setReconciling(false); }
  }
  return <section className="flex min-h-0 min-w-0 flex-1 flex-col gap-3" aria-label="工作记录">
    <div className="flex shrink-0 flex-wrap items-end gap-3">
      <div className="min-w-0 flex-1 sm:max-w-sm"><Input label="搜索工作" value={query} onChange={event => setQuery(event.target.value)} placeholder="目标、名称或来源" /></div>
      <label className="text-xs">工作来源<select className="ml-2 rounded border border-[var(--border)] bg-[var(--surface)] p-2" value={kind} onChange={event => setKind(event.target.value as WorkKind | "all")}><option value="all">全部来源</option>{Object.entries(workKindLabels).map(([key, value]) => <option key={key} value={key}>{value}</option>)}</select></label>
      <div role="group" aria-label="统一工作状态" className="flex flex-wrap gap-1">{([['all', '全部'], ['active', '进行中'], ['attention', '需要处理'], ['completed', '已完成']] as const).map(([key, label]) => <Button key={key} size="sm" variant={filter === key ? "primary" : "secondary"} aria-pressed={filter === key} onClick={() => setFilter(key)}>{label}</Button>)}</div>
      <Button size="sm" variant="ghost" onClick={reload} aria-label="刷新统一工作"><RefreshCw size={15} /></Button>
    </div>
    {data && <p className="shrink-0 text-xs text-[var(--text-secondary)]">显示 {visible.length} / {data.items.length} 项。后台任务在离开页面后继续。{data.truncated && ` 列表最多显示 ${data.limit} 项，可按工作空间缩小范围。`}</p>}
    {error && <div role="alert" className="shrink-0 text-sm text-[var(--danger)]"><p>工作记录读取失败：{error}</p>{data && <p className="text-xs">当前显示上次读取的结果。</p>}<Button size="sm" variant="secondary" onClick={reload}>重新读取</Button></div>}
    {!!data?.projection_issues?.length && <div className="shrink-0 rounded border border-[var(--warning-border)] p-3 text-xs">
      <p role="alert">部分工作状态尚未同步。重建只更新本地记录。</p>
      <details className="my-2"><summary className="cursor-pointer">同步详情</summary><ul className="mt-2 max-h-24 overflow-y-auto">{data.projection_issues.map(issue => <li key={`${issue.kind}:${issue.id}`} className="break-words">{issue.kind} · {issue.id}：{issue.error}（尝试 {issue.attempts} 次）</li>)}</ul></details>
      <Button size="sm" variant="secondary" loading={reconciling} onClick={() => void reconcile()}>重建状态</Button>{reconcileError && <p role="alert" className="mt-2 text-[var(--danger)]">{reconcileError}</p>}
    </div>}
    <div className="scrollbar-thin min-h-0 min-w-0 flex-1 overflow-y-auto rounded-[var(--radius-lg)] border border-[var(--border)]">
      {!data ? (
        !error && <p role="status" className="p-5">正在读取工作记录…</p>
      ) : visible.length === 0 ? (
        <EmptyState title={data.items.length ? "没有匹配的工作" : "还没有工作记录"} description={data.items.length ? "更换搜索条件或来源筛选。" : "从对话、任务计划或个人委托开始，记录会在这里汇总。"} />
      ) : visible.map(item => {
        const title = workDisplayTitle(item);
        const kindLabel = workKindLabels[item.kind];
        const workspaceName = item.workspace_id ? workspaceNames.get(item.workspace_id) : undefined;
        const metadata = [kindLabel, formatTaskDate(item.updated_at), workspaceName].filter(Boolean).join(" · ");
        return <article key={workKey(item)} data-work-kind={item.kind} data-work-id={item.id} className="flex min-w-0 flex-wrap items-center gap-3 border-b border-[var(--border-soft)] p-4 last:border-0">
          <div className="min-w-0 flex-1 basis-48">
            <h3 className="break-words text-sm font-medium">{title}</h3>
            <p className="mt-1 break-words text-xs text-[var(--text-secondary)]">{metadata}</p>
          </div>
          <Badge tone={item.attention_required ? "warning" : ["completed", "succeeded", "reported"].includes(item.status) ? "success" : "default"}>{workStatusLabels[item.status] ?? "状态待确认"}</Badge>
          <Button size="sm" variant="ghost" onClick={() => { setContextOpen(false); setDetails(item); }}>详情</Button>
          <Button size="sm" variant="secondary" onClick={() => openSource(item)}>打开{kindLabel}<ArrowRight size={14} /></Button>
        </article>;
      })}
    </div>
    {details && <Modal open title="工作详情" size="lg" onClose={() => setDetails(null)}>
      <div className="space-y-3 text-sm">
        <h2 className="break-words font-medium">{detailsTitle}</h2>
        <dl className="space-y-2 text-[var(--text-secondary)]">
          <div><dt>记录编号</dt><dd className="break-all font-mono text-xs">{details.id}</dd></div>
          {details.workspace_id && <div><dt>工作空间编号</dt><dd className="break-all font-mono text-xs">{details.workspace_id}</dd></div>}
          {details.title && details.title !== detailsTitle && <div><dt>原始名称</dt><dd className="break-words">{details.title}</dd></div>}
          {!workStatusLabels[details.status] && <div><dt>状态代码</dt><dd className="break-all font-mono text-xs">{details.status}</dd></div>}
        </dl>
        {details.source_ids.conversation_id && <details onToggle={event => setContextOpen(event.currentTarget.open)}>
          <summary className="cursor-pointer">上下文来源</summary>
          {contextOpen && <div className="mt-2"><ConversationContextManifest conversationId={details.source_ids.conversation_id} /></div>}
        </details>}
      </div>
    </Modal>}
  </section>;
}
