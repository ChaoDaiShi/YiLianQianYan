import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { getPersonalHistory, type AutomationOccurrence } from "../../api/automations";
import { Badge, Button, EmptyState, Select } from "../../components/ui";
import { assistantErrorText } from "./assistantErrorText";
import { automationStatusLabels, automationTriggerLabels, mergeOccurrencePages, occurrenceTime } from "./automationDisplay";

const historyFilters = [
  { value: "", label: "全部状态" }, { value: "completed", label: "本轮已汇报" },
  { value: "running", label: "运行中" }, { value: "waiting_approval", label: "等待审批" },
  { value: "waiting_input", label: "等待回答" }, { value: "failed", label: "失败" },
  { value: "interrupted", label: "中断" }, { value: "cancelled", label: "取消" },
  { value: "needs_review", label: "结果需核实" }, { value: "blocked", label: "等待人工检查" },
  { value: "skipped", label: "已跳过" },
];

/** Embeddable content; the caller supplies the one main dialog. */
export default function PersonalHistoryContent({ automationId, onNavigate, active = true }: { automationId: string; onNavigate?: () => void; active?: boolean }) {
  const navigate = useNavigate();
  const [filter, setFilter] = useState("");
  const [version, setVersion] = useState(0);
  const [items, setItems] = useState<AutomationOccurrence[]>([]);
  const [total, setTotal] = useState(0);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [loadedMore, setLoadedMore] = useState(false);
  const generation = useRef(0);
  const requestBusy = useRef(false);
  const paged = useRef(false);
  const mounted = useRef(false);
  const scope = useRef("");
  const failedCursor = useRef<string | undefined>();
  const visible = useRef(active);
  visible.current = active;

  const load = useCallback(async (cursor?: string) => {
    if (requestBusy.current) return;
    const current = generation.current;
    requestBusy.current = true;
    setBusy(true);
    try {
      const page = await getPersonalHistory(automationId, cursor, filter || undefined);
      if (!mounted.current || generation.current !== current) return;
      setItems(previous => cursor ? mergeOccurrencePages(previous, page.occurrences) : page.occurrences);
      setTotal(page.total);
      const canAdvance = page.has_more && Boolean(page.next_cursor) && page.next_cursor !== cursor;
      setHasMore(canAdvance); setNextCursor(canAdvance ? page.next_cursor : null);
      setError(page.has_more && !canAdvance ? "历史分页未返回有效游标，请刷新后重试。" : "");
      if (cursor) { paged.current = true; setLoadedMore(true); }
    } catch (cause) {
      if (mounted.current && generation.current === current) { failedCursor.current = cursor; setError(cause instanceof Error ? cause.message : String(cause)); }
    } finally {
      if (mounted.current && generation.current === current) { setLoading(false); setBusy(false); requestBusy.current = false; }
    }
  }, [automationId, filter]);

  useEffect(() => {
    mounted.current = true; generation.current += 1; requestBusy.current = false; paged.current = false;
    const nextScope = `${automationId}:${filter}`;
    const changed = scope.current !== nextScope;
    scope.current = nextScope;
    if (changed) { setItems([]); setTotal(0); setNextCursor(null); setHasMore(false); }
    setLoading(changed); setError(""); setLoadedMore(false);
    void load();
    // Keep the current first page live; after paging, explicit refresh preserves
    // the user's position instead of discarding older loaded records every tick.
    const timer = setInterval(() => { if (visible.current && !paged.current) void load(); }, 3000);
    return () => { mounted.current = false; generation.current += 1; requestBusy.current = false; clearInterval(timer); };
  }, [load, version]);

  return <div className="min-w-0 space-y-4">
    <div className="flex flex-wrap items-end justify-between gap-3">
      <div className="w-full min-w-0 sm:w-52"><Select label="历史状态" value={filter} onChange={event => setFilter(event.target.value)} options={historyFilters} /></div>
      <Button size="sm" variant="secondary" disabled={busy} loading={busy && !loadedMore} onClick={() => setVersion(value => value + 1)}>刷新历史</Button>
    </div>
    {error && <div role="alert" className="space-y-2 text-sm text-[var(--danger)]"><p className="[overflow-wrap:anywhere]">读取历史失败：{assistantErrorText(error)}{items.length ? " 当前保留上次已读取的记录。" : ""}</p><Button size="sm" variant="secondary" disabled={busy} onClick={() => void load(failedCursor.current)}>重试读取历史</Button></div>}
    {!loading && <p role="status" className="text-xs text-[var(--text-secondary)]">已显示 {items.length} / {total} 条记录{loadedMore ? "；已加载多页，刷新可查看最新状态。" : "；当前页每三秒更新。"}</p>}
    {loading ? <p role="status">正在读取运行历史…</p> : !items.length && !error ? <EmptyState title={filter ? "没有符合状态的记录" : "尚无运行记录"} description={filter ? "切换状态查看其他记录。" : "手动运行或触发后会保留执行记录。"} /> : <div className="space-y-3">{items.map(item => <article key={item.id} className="glass-card min-w-0 space-y-2 rounded-xl border border-[var(--border-soft)] p-3">
      <div className="flex flex-wrap items-center gap-2"><Badge tone={item.status === "completed" ? "success" : item.status === "failed" ? "danger" : item.status.startsWith("waiting") ? "warning" : "default"}>{automationStatusLabels[item.status] || item.status}</Badge><span className="text-sm">{automationTriggerLabels[item.trigger]}</span><time className="text-xs text-[var(--text-secondary)] [overflow-wrap:anywhere]">{occurrenceTime(item.scheduled_at, item.timezone)} · {item.timezone} · 版本 {item.automation_revision}</time></div>
      <p className="text-xs text-[var(--text-muted)] [overflow-wrap:anywhere]">轮次：{item.id}</p>
      {item.error && <p className="text-sm text-[var(--text-secondary)] [overflow-wrap:anywhere]">{assistantErrorText(item.error)}</p>}
      {item.report ? <p className="whitespace-pre-wrap text-sm [overflow-wrap:anywhere]">{item.report}</p> : item.status === "completed" && <p className="text-sm text-[var(--text-secondary)]">本轮结束时没有保存报告正文，请查看执行记录；不会使用其他轮次的报告代替。</p>}
      {item.conversation_id && <Button size="sm" variant="secondary" onClick={() => { onNavigate?.(); navigate(`/chat/${encodeURIComponent(item.conversation_id!)}`); }}>查看任务对话与确认事项</Button>}
    </article>)}</div>}
    {hasMore && nextCursor && <div className="flex justify-center"><Button variant="secondary" disabled={busy} loading={busy} onClick={() => void load(nextCursor)}>加载更多历史</Button></div>}
  </div>;
}
