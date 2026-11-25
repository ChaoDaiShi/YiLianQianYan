import { useCallback, useEffect, useState } from "react";
import { ArrowUpRight, ListChecks } from "lucide-react";
import { Link } from "react-router-dom";
import { listConversations } from "../../api/conversations";
import { extractMemories, type MemoryExtractionMode } from "../../api/memory";
import { Badge, Button, EmptyState, ErrorState, Modal, Select, Textarea } from "../../components/ui";
import type { ConversationSummary } from "../../types";
import { formatMemoryTime, getMemoryCategoryPresentation, MEMORY_CATEGORY_OPTIONS } from "./memoryCenterModel";
import { acceptMemoryCandidate, dismissMemoryCandidate, listMemoryCandidates, type CandidateScope, type MemoryCandidate, type ReviewedMemory } from "./memoryCandidateApi";
import RelationshipFields from "./RelationshipFields";
import { canSaveMemoryRelationship, relationshipKinds, relationshipMemory, type RelationshipKind } from "./relationship";

function sourceLabel(candidate: MemoryCandidate) {
  if (candidate.source_kind === "memory_version") return "来自已保存记忆";
  if (candidate.source_kind === "related_memory_correction") return "关联记忆纠正";
  if (candidate.relationship) return "AI 提议 · 来自用户消息";
  if (candidate.source_kind === "proofloop_acceptance") return "任务验收通过";
  return candidate.source_kind === "user_message" ? "从用户消息提取" : "从任务执行提议";
}

export default function MemoryCandidateReview({ onViewRelationship }: { onViewRelationship: (kind: RelationshipKind, scope?: string) => void }) {
  const [entries, setEntries] = useState<MemoryCandidate[]>([]);
  const [offset, setOffset] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");
  const [notice, setNotice] = useState("");
  const [savedKind, setSavedKind] = useState<RelationshipKind | null>(null);
  const [savedScope, setSavedScope] = useState<string>();
  const [selected, setSelected] = useState<MemoryCandidate | null>(null);
  const [draft, setDraft] = useState<ReviewedMemory | null>(null);
  const [extracting, setExtracting] = useState(false);
  const [sourceId, setSourceId] = useState("");
  const [mode, setMode] = useState<MemoryExtractionMode>("memories");
  const [conversations, setConversations] = useState<ConversationSummary[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState("");
  const linkedCorrection = selected?.source_kind === "related_memory_correction";

  const load = useCallback(async (start = 0) => {
    setLoading(true);
    setLoadError("");
    try {
      const result = await listMemoryCandidates(start);
      setEntries(current => start ? [...current, ...result.entries] : result.entries);
      setOffset(result.next_offset);
    } catch (cause) {
      setLoadError(String((cause as Error).message || cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void load(); }, [load]);

  async function openExtraction() {
    setActionError("");
    setSourceId("");
    setMode("memories");
    setConversations(null);
    setExtracting(true);
    try { setConversations(await listConversations()); }
    catch (cause) { setActionError(String((cause as Error).message || cause)); }
  }

  async function extract() {
    if (!sourceId || busy) return;
    setBusy(true);
    setActionError("");
    try {
      const result = await extractMemories(sourceId, mode);
      setExtracting(false);
      setSavedKind(null);
      setNotice(result.candidate_ids.length ? `已生成 ${result.candidate_ids.length} 条待审阅候选` : "没有新增候选");
      await load();
    } catch (cause) { setActionError(String((cause as Error).message || cause)); }
    finally { setBusy(false); }
  }

  function select(candidate: MemoryCandidate) {
    setActionError("");
    setSelected(candidate);
    const scope = candidate.source_kind === "user_message" && candidate.category === "preference" && candidate.source_conversation_id
      ? "conversation" : candidate.workspace_id ? "workspace" : candidate.source_conversation_id ? "conversation" : "profile";
    setDraft({ content: candidate.content, category: candidate.category, scope, ...(candidate.relationship ? { relationship: candidate.relationship } : {}) });
  }

  async function decide(accept: boolean) {
    if (!selected || !draft || busy) return;
    setBusy(true);
    setActionError("");
    try {
      const saved = accept ? await acceptMemoryCandidate(selected.id, {
        ...draft, content: draft.relationship ? relationshipMemory(draft.relationship).content : draft.content.trim(),
      }) : null;
      if (!accept) await dismissMemoryCandidate(selected.id);
      setSelected(null);
      setSavedKind(accept ? draft.relationship?.kind ?? null : null);
      setSavedScope(selected.source_kind === "memory_version" ? selected.source_memory_scope ?? undefined
        : draft.scope === "workspace" && selected.workspace_id ? `workspace:${selected.workspace_id}`
        : draft.scope === "conversation" && selected.source_conversation_id ? `conversation:${selected.source_conversation_id}` : "profile");
      setNotice(!accept ? "已忽略候选" : linkedCorrection
        ? saved?.content === selected.source_excerpt ? "内容没有变化，未新增版本" : "已更新关联记忆"
        : selected.source_kind === "memory_version" ? "已保存为图谱关系" : draft.relationship ? "已保存为关系记忆" : "已保存为长期记忆");
      await load();
    } catch (cause) { setActionError(String((cause as Error).message || cause)); }
    finally { setBusy(false); }
  }

  return <section className="memory-layer-scroll" aria-label="待审阅记忆候选">
    <div className="memory-reference-toolbar">
      <Button size="sm" onClick={() => void openExtraction()}>从对话提取</Button>
    </div>
    {notice && <div className="memory-layer-hint flex flex-wrap items-center gap-2"><p role="status">{notice}</p>
      {savedKind && <Button size="sm" variant="secondary" onClick={() => onViewRelationship(savedKind,savedScope)}>查看关系图</Button>}
    </div>}
    {loading && !entries.length && <p role="status" className="memory-layer-hint">正在读取候选…</p>}
    {loadError && <ErrorState title="读取候选失败" description={loadError} onRetry={() => void load()} />}
    {!loading && !loadError && !entries.length && <EmptyState icon={<ListChecks size={26} />} title="暂无待审阅候选" description="可以从对话提取候选，任务经验也会在这里等待审阅。" />}
    {entries.length > 0 && <ol className="memory-activity-timeline">
      {entries.map(candidate => <li key={candidate.id}>
        <span className="memory-activity-icon" aria-hidden="true"><ListChecks size={16} /></span>
        <button className="memory-activity-entry glass-card" type="button" onClick={() => select(candidate)}>
          <span className="memory-activity-heading"><strong>{candidate.source_kind === "related_memory_correction" ? sourceLabel(candidate) : candidate.relationship ? `AI 提议 · ${relationshipKinds[candidate.relationship.kind]}` : `${getMemoryCategoryPresentation(candidate.category).label} · ${sourceLabel(candidate)}`}</strong><time>{formatMemoryTime(candidate.created_at)}</time></span>
          <span className="memory-activity-excerpt">{candidate.content}</span>
          {candidate.source_removed && <span className="memory-activity-source">{candidate.source_kind === "related_memory_correction" ? "关系或目标版本已变化" : "来源已移除"}</span>}
        </button>
      </li>)}
    </ol>}
    {offset !== null && <Button variant="secondary" size="sm" loading={loading} onClick={() => void load(offset)}>加载更多</Button>}
    <Modal open={extracting} title="从对话提取记忆候选" onClose={() => { if (!busy) setExtracting(false); }} footer={<>
      <Button variant="secondary" disabled={busy} onClick={() => setExtracting(false)}>取消</Button>
      <Button loading={busy} disabled={!sourceId} onClick={() => void extract()}>生成候选</Button>
    </>}>
      <div className="memory-activity-detail">
        <Select label="来源对话" value={sourceId} disabled={!conversations || busy} options={[
          { value: "", label: conversations ? "选择一条对话" : "正在读取对话…" },
          ...(conversations || []).map(conversation => ({ value: conversation.id, label: conversation.title })),
        ]} onChange={event => setSourceId(event.target.value)} />
        <Select label="提取内容" value={mode} disabled={busy} options={[
          { value: "memories", label: "长期记忆" }, { value: "relationships", label: "关系图谱" },
        ]} onChange={event => setMode(event.target.value as MemoryExtractionMode)} />
        <p className="text-xs text-[var(--text-secondary)]">当前配置的模型会读取所选对话中最近的用户消息。{mode === "relationships" ? "模型可以从同一对话提出人物、亲属、事件、项目和概念关系。审阅并保存后才会出现在图中。" : "候选经确认后才会进入记忆检索。"}</p>
        {actionError && <p role="alert" className="text-[var(--danger)]">{actionError}</p>}
      </div>
    </Modal>
    <Modal open={Boolean(selected && draft)} title="审阅记忆候选" onClose={() => { if (!busy) setSelected(null); }} footer={<>
      <Button variant="secondary" disabled={busy} onClick={() => void decide(false)}>忽略候选</Button>
      <Button loading={busy} disabled={selected?.source_removed || (draft?.relationship ? !canSaveMemoryRelationship(draft.relationship) : !draft?.content.trim())} onClick={() => void decide(true)}>确认并保存</Button>
    </>}>
      {selected && draft && <div className="memory-activity-detail">
        <Badge>{selected.source_removed ? linkedCorrection ? "关系或目标版本已变化" : selected.source_kind === "memory_version" ? "来源版本已失效" : "来源已移除" : sourceLabel(selected)}</Badge>
        {linkedCorrection && selected.source_removed && <p>这条候选的依据已失效。可以忽略候选，再从当前关系发起新的纠正。</p>}
        {linkedCorrection && selected.correction
          ? <dl className="grid gap-2 break-words">
              <div><dt className="font-medium">对应关系</dt><dd>{selected.correction.label}</dd></div>
              <div><dt className="font-medium">目标记忆 · 版本 {selected.correction.target_revision}</dt><dd className="whitespace-pre-wrap">{selected.source_excerpt}</dd></div>
              {selected.correction.source_version_id && <div><dt className="font-medium">选定的关联版本</dt><dd>{selected.correction.source_version_id}</dd></div>}
              <div><dt className="font-medium">纠正依据</dt><dd className="whitespace-pre-wrap">{selected.correction.reason}</dd></div>
            </dl>
          : <>
              {selected.source_title && selected.source_title !== selected.source_excerpt && <p>{selected.source_title}</p>}
              {selected.source_excerpt && <blockquote className="whitespace-pre-wrap break-words border-l-2 border-[var(--border-soft)] pl-3">{selected.source_excerpt}</blockquote>}
            </>}
        {selected.source_conversation_id && <Link to={`/chat/${encodeURIComponent(selected.source_conversation_id)}`}>打开来源对话 <ArrowUpRight size={13} /></Link>}
        {selected.source_kind === "memory_version" && selected.source_memory_id && !selected.source_removed &&
          <Link to={`/memory?record=${encodeURIComponent(selected.source_memory_id)}`}>查阅来源记忆 · 版本 {selected.source_memory_revision} <ArrowUpRight size={13} /></Link>}
        {selected.source_task_id && selected.workspace_id && <Link to={`/workspaces/${encodeURIComponent(selected.workspace_id)}?tab=tasks`}>查看来源工作空间任务 <ArrowUpRight size={13} /></Link>}
        {selected.source_graph_id && selected.workspace_id && <Link to={`/workspaces/${encodeURIComponent(selected.workspace_id)}?tab=tasks&graph=${encodeURIComponent(selected.source_graph_id)}`}>查看来源任务图与验收 <ArrowUpRight size={13} /></Link>}
        {selected.source_agent && <p className="text-xs text-[var(--text-secondary)]">提议智能体 {selected.source_agent}</p>}
        {draft.relationship ? <>
          <p className="text-xs text-[var(--text-secondary)]">可修改节点、联系和方向。保持「推断 · 待确认」时，保存后图中仍用虚线标记。</p>
          <RelationshipFields value={draft.relationship} disabled={busy || selected.source_removed} onChange={relationship => setDraft({ ...draft, relationship })} />
        </> : <>
          <Textarea label={linkedCorrection ? "纠正后的目标记忆" : "确认后的记忆内容"} value={draft.content} rows={4} maxLength={2000} disabled={busy || selected.source_removed} onChange={event => setDraft({ ...draft, content: event.target.value })} />
          {!linkedCorrection && <Select label="分类" value={draft.category} disabled={busy || selected.source_removed} options={MEMORY_CATEGORY_OPTIONS.filter(option => option.value && (selected.source_kind === "user_message" || option.value === "note" || option.value === "knowledge"))} onChange={event => setDraft({ ...draft, category: event.target.value })} />}
        </>}
        {linkedCorrection && !selected.source_removed && <p className="text-xs text-[var(--text-secondary)]">保存仅更新这条目标记忆，保留原分类和适用范围。</p>}
        {selected.source_kind === "memory_version" && !selected.source_removed && <p className="text-xs text-[var(--text-secondary)]">关系绑定来源记忆的这一版，沿用原记忆的适用范围。</p>}
        {!linkedCorrection && selected.source_kind !== "memory_version" && <Select label="适用范围" value={draft.scope} disabled={busy || selected.source_removed} options={[
          ...(selected.source_conversation_id ? [{ value: "conversation", label: "来源对话" }] : []),
          ...(selected.workspace_id ? [{ value: "workspace", label: "来源工作空间" }] : []),
          { value: "profile", label: "当前档案" },
        ]} onChange={event => setDraft({ ...draft, scope: event.target.value as CandidateScope })} />}
        {actionError && <p role="alert" className="text-[var(--danger)]">{actionError}</p>}
      </div>}
    </Modal>
  </section>;
}
