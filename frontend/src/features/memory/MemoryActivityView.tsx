import { useCallback, useEffect, useRef, useState } from "react";
import { ArrowUpRight, History, MessageSquare, Trash2 } from "lucide-react";
import { Link } from "react-router-dom";
import { createMemory, deleteMemoryActivity, listMemoryActivity, type MemoryActivity } from "../../api/memory";
import { Badge, Button, ConfirmDialog, EmptyState, ErrorState, Input, Modal, Select, Textarea } from "../../components/ui";
import { useMemoryStore } from "../../stores/memoryStore";
import { MEMORY_CATEGORY_OPTIONS } from "./memoryCenterModel";
const activityTime = new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeStyle: "short" });

const actionLabel: Record<MemoryActivity["kind"], string> = {
  chat_message: "发送消息",
  memory_created: "添加记忆",
  memory_edited: "更新记忆",
  memory_withdrawn: "撤回记忆",
  memory_deleted: "永久清除记忆",
};
const originLabel = { user_message: "用户原话", manual: "用户确认", auto: "AI 提取 · 待核对", document: "资料导入" };

export default function MemoryActivityView({ recent, onPromoted, onPurged }: { recent: boolean; onPromoted: () => void; onPurged: (id: string) => void }) {
  const [entries, setEntries] = useState<MemoryActivity[]>([]);
  const [next, setNext] = useState<number | null>(null);
  const [pending, setPending] = useState(true);
  const [error, setError] = useState("");
  const [open, setOpen] = useState<MemoryActivity | null>(null);
  const [removing, setRemoving] = useState<MemoryActivity | null>(null);
  const [removeError, setRemoveError] = useState("");
  const [promotion, setPromotion] = useState<{ content: string; category: string } | null>(null);
  const [promoting, setPromoting] = useState(false);
  const [promotionError, setPromotionError] = useState("");
  const [query, setQuery] = useState("");
  const request = useRef(0);

  const load = useCallback(async (offset = 0) => {
    const id = ++request.current;
    setPending(true);
    setError("");
    try {
      const result = await listMemoryActivity(recent, offset, query.trim());
      if (id !== request.current) return;
      setEntries(current => offset ? [...new Map([...current, ...result.entries].map(entry => [entry.id, entry])).values()] : result.entries);
      setNext(result.next_offset);
    } catch (cause) {
      if (id === request.current) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (id === request.current) setPending(false);
    }
  }, [recent, query]);

  useEffect(() => {
    setPending(true);
    const timer = window.setTimeout(() => void load(), query.trim() ? 180 : 0);
    return () => { request.current++; window.clearTimeout(timer); };
  }, [load, query]);

  async function remove() {
    if (!removing) return;
    setRemoveError("");
    try {
      if (removing.memory_id) {
        const { removeMemory } = useMemoryStore.getState();
        if (!await removeMemory(removing.memory_id)) throw new Error(useMemoryStore.getState().error || "无法清除记忆");
        onPurged(removing.memory_id);
      } else {
        await deleteMemoryActivity(removing.id);
      }
      setRemoving(null);
      setOpen(null);
      await load();
    } catch (cause) {
      setRemoveError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function promote() {
    if (!promotion?.content.trim() || !open?.conversation_id || promoting) return;
    setPromoting(true);
    setPromotionError("");
    try {
      await createMemory({
        content: promotion.content.trim(),
        category: promotion.category,
        source: "manual",
        source_conversation_id: open.conversation_id,
      });
      setPromotion(null);
      setOpen(null);
      onPromoted();
    } catch (cause) {
      setPromotionError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setPromoting(false);
    }
  }

  function close() {
    if (promoting) return;
    setPromotion(null);
    setOpen(null);
  }

  return <section className="memory-layer-scroll" aria-label={recent ? "近期记忆" : "底层记忆"}>
    <div className="memory-reference-toolbar"><Input aria-label="搜索记忆记录" placeholder="搜索记录正文…" value={query} onChange={event => setQuery(event.target.value)} /></div>
    <p className="memory-layer-intro">{recent
      ? "最近 30 天仍可定位的对话消息。可以整理为长期记忆。"
      : "保留的消息摘录与记忆变更记录。此处的消息摘录不会进入记忆检索。"}</p>
    {error && <ErrorState title="读取记忆记录失败" description={error} onRetry={() => void load()} />}
    {pending && !entries.length && <p className="memory-layer-hint" role="status">正在读取记忆记录…</p>}
    {!pending && !error && !entries.length && <EmptyState icon={<History size={26} />} title={query.trim() ? "没有匹配的记忆记录" : recent ? "暂无近期对话记录" : "暂无底层记录"} />}
    {entries.length > 0 && <ol className="memory-activity-timeline">
      {entries.map(entry => <li key={entry.id}>
        <span className="memory-activity-icon" aria-hidden="true">{entry.kind === "chat_message" ? <MessageSquare size={16} /> : <History size={16} />}</span>
        <button type="button" className="memory-activity-entry glass-card" onClick={() => setOpen(entry)}>
          <span className="memory-activity-heading"><strong>{actionLabel[entry.kind]}</strong><time dateTime={new Date(entry.recorded_at).toISOString()}>{activityTime.format(entry.recorded_at)}</time></span>
          {entry.content && <span className="memory-activity-excerpt">{entry.content}</span>}
          {entry.source_removed && <span className="memory-activity-source">来源对话或消息已移除</span>}
        </button>
      </li>)}
    </ol>}
    {next !== null && <Button variant="secondary" size="sm" loading={pending} onClick={() => void load(next)}>加载更多</Button>}
    <Modal open={Boolean(open && !removing)} title={promotion ? "保存为长期记忆" : open ? actionLabel[open.kind] : "记忆记录"} onClose={close}
      footer={<>
        {promotion ? <>
          <Button variant="secondary" disabled={promoting} onClick={() => setPromotion(null)}>返回</Button>
          <Button loading={promoting} disabled={!promotion.content.trim()} onClick={() => void promote()}>保存长期记忆</Button>
        </> : <>
          {!recent && open && (open.removable || open.memory_id) && <Button variant="secondary" onClick={() => { setRemoveError(""); setRemoving(open); }}><Trash2 size={15} />{open.memory_id ? "永久清除记忆" : "清除底层记录"}</Button>}
          {recent && open?.conversation_id && <Button onClick={() => { setPromotionError(""); setPromotion({ content: open.content ?? "", category: "fact" }); }}>保存为长期记忆</Button>}
          <Button variant="secondary" onClick={close}>关闭</Button>
        </>}
      </>}>
      {promotion ? <div className="memory-activity-detail">
        <Select label="分类" value={promotion.category} options={MEMORY_CATEGORY_OPTIONS.filter(option => option.value)} onChange={event => setPromotion({ ...promotion, category: event.target.value })} />
        <Textarea label="要保存的内容" value={promotion.content} rows={5} maxLength={2000} onChange={event => setPromotion({ ...promotion, content: event.target.value })} />
        {promotionError && <p className="text-[var(--danger)]" role="alert">{promotionError}</p>}
      </div> : open && <div className="memory-activity-detail">
        <time dateTime={new Date(open.recorded_at).toISOString()}>{activityTime.format(open.recorded_at)}</time>
        <div className="flex flex-wrap gap-2">
          {open.origin && <Badge>{originLabel[open.origin]}</Badge>}
          {open.memory_status && <Badge tone={open.memory_status === "withdrawn" ? "warning" : "default"}>{open.memory_status === "withdrawn" ? "当时已撤回" : "当时可用于检索"}</Badge>}
        </div>
        {open.source_removed && <Badge tone="default">来源对话或消息已移除</Badge>}
        {open.content && <p>{open.content}</p>}
        <div className="flex flex-wrap gap-3">
          {open.conversation_id && <Link to={`/chat/${encodeURIComponent(open.conversation_id)}`}>打开来源对话 <ArrowUpRight size={13} /></Link>}
          {open.memory_id && <Link to={`/memory?record=${encodeURIComponent(open.memory_id)}`} onClick={() => setOpen(null)}>查看记忆和历史版本 <ArrowUpRight size={13} /></Link>}
        </div>
      </div>}
    </Modal>
    <ConfirmDialog open={Boolean(removing)} title={removing?.memory_id ? "永久清除记忆" : "清除底层记录"} description={removeError || (removing?.memory_id
      ? "这条记忆的正文、历史版本、检索索引与关系图引用将一并清除。来源对话及独立的消息摘录会保留。已同步的副本需等待同步完成，本地页面无法确认云端清除结果。"
      : "这条底层记录将永久清除。仍存在的来源对话和独立保存的长期记忆会保留。")}
      tone="danger" confirmLabel="永久清除" onCancel={() => setRemoving(null)} onConfirm={remove} />
  </section>;
}
