import { useCallback, useEffect, useState } from "react";
import { ArrowUpRight, BookOpen, Plus } from "lucide-react";
import { Link } from "react-router-dom";
import {
  deleteKnowledgeEntry, getKnowledgeEntry, listKnowledgeEntries, saveKnowledgeEntry, searchKnowledge,
  type KnowledgeEntry, type KnowledgeEntryDraft, type KnowledgeEntrySummary, type KnowledgeSearchHit,
  type KnowledgeSourceAnchor,
} from "../../api/knowledge";
import { Badge, Button, ConfirmDialog, EmptyState, ErrorState, Input, Modal, Textarea } from "../../components/ui";
import { anchorForSearchHit, knowledgeSourceUrl } from "../knowledge/knowledgeAnchor";
import KnowledgeReferenceGraph from "./KnowledgeReferenceGraph";

type DraftSource = {
  anchor: KnowledgeSourceAnchor;
  documentName: string;
  excerpt: string | null;
  status: "available" | "changed" | "removed";
};
type Draft = Omit<KnowledgeEntryDraft, "sources"> & { sources: DraftSource[] };
const errorText = (cause: unknown) => cause instanceof Error ? cause.message : String(cause);
const sourceKey = (anchor: KnowledgeSourceAnchor) => [anchor.document_id, anchor.version, anchor.start_char, anchor.end_char].join(":");
const statusText = { available: "可定位原文", changed: "正文版本已变化", removed: "来源已移除" };
const emptyDraft: Draft = { topic: "", title: "", summary: "", sources: [] };

function toDraft(entry: KnowledgeEntry): Draft {
  return {
    topic: entry.topic,
    title: entry.title,
    summary: entry.summary,
    sources: entry.sources.map(source => ({
      anchor: { document_id: source.document_id, version: source.version, start_char: source.start_char, end_char: source.end_char },
      documentName: source.document_name,
      excerpt: source.excerpt,
      status: source.status,
    })),
  };
}

export default function MemoryKnowledgeReferences() {
  const [entries, setEntries] = useState<KnowledgeEntrySummary[]>([]);
  const [query, setQuery] = useState("");
  const [view, setView] = useState<"list" | "graph">("list");
  const [offset, setOffset] = useState(0);
  const [next, setNext] = useState<number | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [refresh, setRefresh] = useState(0);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<KnowledgeEntry | null>(null);
  const [detailError, setDetailError] = useState("");
  const [detailRefresh, setDetailRefresh] = useState(0);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [removing, setRemoving] = useState(false);
  const [removeError, setRemoveError] = useState("");
  const resetList = useCallback(() => { setOffset(0); setRefresh(value => value + 1); }, []);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError("");
    const timer = window.setTimeout(() => {
      void listKnowledgeEntries(query.trim(), offset).then(result => {
        if (!active) return;
        setEntries(current => offset === 0 ? result.entries : [...current, ...result.entries.filter(item => !current.some(entry => entry.id === item.id))]);
        setNext(result.next_offset);
      }).catch(cause => { if (active) setError(errorText(cause)); })
        .finally(() => { if (active) setLoading(false); });
    }, offset === 0 && query.trim() ? 180 : 0);
    return () => { active = false; window.clearTimeout(timer); };
  }, [query, offset, refresh]);

  useEffect(() => {
    let active = true;
    setDetail(null);
    setDetailError("");
    if (selectedId) void getKnowledgeEntry(selectedId).then(result => { if (active) setDetail(result); })
      .catch(cause => { if (active) setDetailError(errorText(cause)); });
    return () => { active = false; };
  }, [selectedId, detailRefresh]);

  function close() {
    if (saving) return;
    setDraft(null);
    setSelectedId(null);
    setSaveError("");
  }

  async function save() {
    if (!draft || saving) return;
    setSaving(true);
    setSaveError("");
    try {
      const entry = await saveKnowledgeEntry({ ...draft, sources: draft.sources.map(source => source.anchor) }, selectedId ?? undefined);
      setSelectedId(entry.id);
      setDetail(entry);
      setDraft(null);
      resetList();
    } catch (cause) {
      setSaveError(errorText(cause));
    } finally {
      setSaving(false);
    }
  }

  async function remove() {
    if (!selectedId) return;
    setRemoveError("");
    try {
      await deleteKnowledgeEntry(selectedId);
      setRemoving(false);
      close();
      resetList();
    } catch (cause) {
      setRemoveError(errorText(cause));
    }
  }

  const groups = new Map<string, KnowledgeEntrySummary[]>();
  for (const entry of entries) {
    const group = groups.get(entry.topic);
    if (group) group.push(entry);
    else groups.set(entry.topic, [entry]);
  }
  const valid = draft && [draft.title, draft.topic, draft.summary].every(value => value.trim()) && draft.sources.length > 0;

  return <section className={view === "graph" ? "memory-network-workspace" : "memory-layer-scroll"} aria-label="知识引用">
    <div className="knowledge-entries-toolbar">
      <Input aria-label="搜索知识条目" placeholder="搜索主题、标题或摘要…" value={query} onChange={event => { setQuery(event.target.value); setOffset(0); }} />
      <Button size="sm" onClick={() => setDraft(emptyDraft)}><Plus size={16} />新建知识条目</Button>
      <Link to="/knowledge" className="memory-reference-link">打开资料库 <ArrowUpRight size={14} /></Link>
      <div className="management-status-tabs" aria-label="知识引用显示方式">
        <button type="button" aria-pressed={view === "list"} onClick={() => setView("list")}>列表</button>
        <button type="button" aria-pressed={view === "graph"} onClick={() => setView("graph")}>引用关系图</button>
      </div>
    </div>
    {error ? <ErrorState title="知识条目无法加载" description={error} onRetry={() => setRefresh(value => value + 1)} />
      : loading && offset === 0 ? <p className="memory-layer-hint" role="status">正在读取知识条目…</p>
      : !entries.length ? <EmptyState icon={<BookOpen size={25} />} title={query.trim() ? "没有匹配的知识条目" : "暂无整理后的知识条目"} description={query.trim() ? undefined : "新建条目，填写摘要并引用资料库中的原文。"} />
      : <>
        {view === "graph" ? <KnowledgeReferenceGraph entries={entries} onEntry={setSelectedId} revision={refresh} /> : [...groups].map(([topic, items]) => <section className="knowledge-entry-topic" key={topic}>
          <h3>{topic}</h3>
          <div className="memory-reference-grid">{items.map(item =>
            <button className="glass-card knowledge-entry-card" key={item.id} type="button" onClick={() => setSelectedId(item.id)}>
              <strong>{item.title}</strong>
              <p>{item.summary}</p>
              <small>{item.source_count} 条引用</small>
            </button>
          )}</div>
        </section>)}
        {next !== null && <Button size="sm" variant="secondary" loading={loading} onClick={() => setOffset(next)}>加载更多</Button>}
      </>}
    <Modal open={Boolean((selectedId || draft) && !removing)} onClose={close}
      title={draft ? selectedId ? "编辑知识条目" : "新建知识条目" : detail?.title ?? "知识条目"} size="lg"
      footer={draft ? <>
        <Button variant="secondary" disabled={saving} onClick={() => { setDraft(null); setSaveError(""); }}>取消</Button>
        <Button loading={saving} disabled={!valid} onClick={() => void save()}>保存知识条目</Button>
      </> : detail && <>
        <Button variant="secondary" onClick={() => { setRemoveError(""); setRemoving(true); }}>删除条目</Button>
        <Button onClick={() => setDraft(toDraft(detail))}>编辑条目</Button>
      </>}>
      {draft ? <div className="knowledge-entry-detail">
        <Input label="主题" maxLength={120} value={draft.topic} onChange={event => setDraft({ ...draft, topic: event.target.value })} />
        <Input label="标题" maxLength={160} value={draft.title} onChange={event => setDraft({ ...draft, title: event.target.value })} />
        <Textarea label="摘要" rows={4} maxLength={8000} value={draft.summary} onChange={event => setDraft({ ...draft, summary: event.target.value })} />
        {saveError && <p className="text-[var(--danger)]" role="alert">{saveError}</p>}
        <SourceList sources={draft.sources} remove={key => setDraft(current => current && ({ ...current, sources: current.sources.filter(source => sourceKey(source.anchor) !== key) }))} />
        <SourcePicker sources={draft.sources} onAdd={source => setDraft(current => current && ({ ...current, sources: [...current.sources, source] }))} />
      </div> : detail ? <div className="knowledge-entry-detail">
        <Badge>{detail.topic}</Badge>
        <p>{detail.summary}</p>
        <SourceList sources={toDraft(detail).sources} />
      </div> : detailError ? <ErrorState title="知识条目无法打开" description={detailError} onRetry={() => setDetailRefresh(value => value + 1)} />
        : <p role="status">正在读取知识条目…</p>}
    </Modal>
    <ConfirmDialog open={removing} onCancel={() => setRemoving(false)} onConfirm={remove} tone="danger" title="删除知识条目"
      description={removeError || "条目、摘要和引用关联将被移除。引用的资料仍可在资料库查看。"} confirmLabel="删除条目" />
  </section>;
}

function SourceList({ sources, remove }: { sources: DraftSource[]; remove?: (key: string) => void }) {
  return <section className="knowledge-entry-detail" aria-label="原文引用">
    <h4>原文引用</h4>
    {sources.map(source => {
      const key = sourceKey(source.anchor);
      return <article className="knowledge-entry-source" key={key}>
        <div className="knowledge-entry-source-heading">
          <strong>{source.documentName}</strong>
          <Badge tone={source.status === "available" ? "default" : "warning"}>{statusText[source.status]}</Badge>
        </div>
        {source.excerpt && <blockquote>{source.excerpt}</blockquote>}
        <div className="knowledge-entry-source-heading">
          {source.status === "available" && (remove
            ? <span>正文第 {source.anchor.start_char + 1}–{source.anchor.end_char} 个字符</span>
            : <Link className="memory-reference-link" to={knowledgeSourceUrl(source.anchor)}>定位原文 <ArrowUpRight size={14} /></Link>)}
          {remove && <Button size="sm" variant="ghost" onClick={() => remove(key)}>移除引用</Button>}
        </div>
      </article>;
    })}
  </section>;
}

function SourcePicker({ sources, onAdd }: { sources: DraftSource[]; onAdd: (source: DraftSource) => void }) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<KnowledgeSearchHit[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function search() {
    const text = query.trim();
    if (!text || loading) return;
    setLoading(true);
    setError("");
    try {
      const result = await searchKnowledge(text);
      setHits(result.hits.filter(hit => hit.content.trim().length > 0));
    } catch (cause) {
      setHits(null);
      setError(errorText(cause));
    } finally {
      setLoading(false);
    }
  }

  return <section className="knowledge-entry-detail" aria-label="添加原文引用">
    <form className="knowledge-entries-toolbar" onSubmit={event => { event.preventDefault(); void search(); }}>
      <Input label="从资料库引用" placeholder="输入原文关键词或资料名称" value={query} onChange={event => setQuery(event.target.value)} />
      <Button type="submit" variant="secondary" loading={loading} disabled={!query.trim()}>查找原文</Button>
    </form>
    {error && <p className="text-[var(--danger)]" role="alert">{error}</p>}
    {hits?.length === 0 && <p>没有匹配的原文。</p>}
    {hits?.map(hit => {
      const anchor = anchorForSearchHit(hit);
      const present = sources.some(source => sourceKey(source.anchor) === sourceKey(anchor));
      return <article className="knowledge-entry-source" key={sourceKey(anchor)}>
        <strong>{hit.document_name}</strong>
        <blockquote>{hit.content}</blockquote>
        <Button size="sm" variant="secondary" disabled={present || sources.length >= 16} onClick={() => onAdd({
          anchor, documentName: hit.document_name, excerpt: hit.content, status: "available",
        })}>{present ? "已引用" : "引用此片段"}</Button>
      </article>;
    })}
  </section>;
}
