import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, FileText, Layers3 } from "lucide-react";
import { listKnowledgeChunks, type KnowledgeChunk, type KnowledgeDetail, type KnowledgeSourceAnchor } from "../../api/knowledge";
import { Button, EmptyState, ErrorState, Input, Tabs } from "../../components/ui";
import { resolveKnowledgeAnchor } from "./knowledgeAnchor";

export default function KnowledgeDocumentReader({ detail, anchor }: { detail: KnowledgeDetail; anchor?: KnowledgeSourceAnchor | null }) {
  const [view, setView] = useState("document");
  const [chunkAnchor, setChunkAnchor] = useState<KnowledgeSourceAnchor | null>(null);
  const [query, setQuery] = useState("");
  const [openChunk, setOpenChunk] = useState<string | null>(null);
  const [offsets, setOffsets] = useState([0]);
  const [next, setNext] = useState<number | null>(null);
  const [chunks, setChunks] = useState<KnowledgeChunk[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [reload, setReload] = useState(0);
  const highlight = useRef<HTMLElement>(null);
  const currentAnchor = chunkAnchor ?? anchor;
  const anchored = useMemo(() => currentAnchor ? resolveKnowledgeAnchor(detail, currentAnchor) : null, [detail, currentAnchor]);
  const offset = offsets[offsets.length - 1] ?? 0;

  useEffect(() => {
    if (view === "document" && anchored) highlight.current?.scrollIntoView({ block: "center" });
  }, [anchored, view]);

  useEffect(() => {
    if (view !== "chunks") return;
    let active = true;
    setLoading(true);
    setError("");
    const timer = window.setTimeout(() => {
      void listKnowledgeChunks(detail.document.id, query.trim(), offset)
        .then(result => { if (active) { setChunks(result.chunks); setNext(result.next_offset); } })
        .catch(cause => { if (active) setError(cause instanceof Error ? cause.message : String(cause)); })
        .finally(() => { if (active) setLoading(false); });
    }, query.trim() ? 180 : 0);
    return () => { active = false; window.clearTimeout(timer); };
  }, [detail.document.id, offset, query, reload, view]);

  return <div className="knowledge-reader">
    <div className="knowledge-reader-toolbar">
      <Tabs activeId={view} onChange={setView} variant="segment" size="sm" aria-label="资料阅读方式" tabs={[
        { id: "document", label: <><FileText size={15} />全文</>, panelId: false },
        { id: "chunks", label: <><Layers3 size={15} />切片</>, panelId: false },
      ]} />
      <span className="knowledge-library-hint">{detail.document.char_count.toLocaleString()} 字符 · {detail.document.chunk_count} 个切片</span>
    </div>
    {view === "document" ? <>
      {detail.memberships.map(membership => <p className="knowledge-document-folders" key={`${membership.collection_id}:${membership.relative_path}`}>{membership.collection_name} · {membership.relative_path}</p>)}
      {currentAnchor && <p className="knowledge-library-hint" role="status">
        {anchored ? `已定位原文第 ${currentAnchor.start_char + 1}–${currentAnchor.end_char} 个字符` : "引用所对应的正文版本或位置已失效。下方显示当前正文。"}
      </p>}
      <pre className="knowledge-document-text" aria-label="知识文档全文">{anchored
        ? <>{anchored.before}<mark className="knowledge-source-highlight" ref={highlight}>{anchored.excerpt}</mark>{anchored.after}</>
        : detail.text}</pre>
    </> : <section className="knowledge-chunks" aria-label="文档切片">
      <Input aria-label="搜索切片正文" placeholder="在这份文档的切片中查找…" value={query} onChange={event => { setQuery(event.target.value); setOffsets([0]); setOpenChunk(null); }} />
      {error ? <ErrorState title="切片无法加载" description={error} onRetry={() => setReload(value => value + 1)} />
        : loading ? <p role="status">正在读取切片…</p>
        : !chunks.length ? <EmptyState title={query.trim() ? "没有匹配的切片" : "这份文档暂无切片"} />
        : <ol className="knowledge-chunk-list">{chunks.map(chunk => {
          const expanded = openChunk === chunk.memory_id;
          return <li className="glass-card knowledge-chunk-card" key={chunk.memory_id}>
            <button type="button" aria-expanded={expanded} onClick={() => setOpenChunk(expanded ? null : chunk.memory_id)}>
              <strong>切片 {chunk.chunk_index + 1}</strong><span>{Array.from(chunk.content).length.toLocaleString()} 字符</span><ChevronDown size={16} />
            </button>
            {expanded ? <pre>{chunk.content}</pre> : <p>{chunk.content}</p>}
            {expanded && chunk.start_char != null && chunk.end_char != null && chunk.version && <Button variant="ghost" size="sm" onClick={() => {
              setChunkAnchor({ document_id: chunk.document_id, version: chunk.version!, start_char: chunk.start_char!, end_char: chunk.end_char! });
              setView("document");
            }}>定位原文第 {chunk.start_char + 1}–{chunk.end_char} 个字符</Button>}
          </li>;
        })}</ol>}
      {(offset > 0 || next !== null) && <div className="knowledge-chunk-pagination">
        <Button variant="secondary" size="sm" disabled={loading || offset === 0} onClick={() => { setOffsets(current => current.slice(0, -1)); setOpenChunk(null); }}>上一页</Button>
        <span>第 {offsets.length} 页</span>
        <Button variant="secondary" size="sm" disabled={loading || next === null} onClick={() => { if (next !== null) { setOffsets(current => [...current, next]); setOpenChunk(null); } }}>下一页</Button>
      </div>}
    </section>}
  </div>;
}
