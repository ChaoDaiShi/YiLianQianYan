import { useCallback, useEffect, useState } from "react";
import { BookOpen, Check, ExternalLink, History, RefreshCw, Sparkles, Square } from "lucide-react";
import { Badge, Button, EmptyState, Modal, Textarea } from "../../components/ui";
import {
  cancelKnowledgeBuild, createKnowledgeBuild, getKnowledgeBuild, importKnowledgeBuild, listKnowledgeBuilds,
  type KnowledgeBuild, type KnowledgeBuildMode, type KnowledgeBuildNode, type KnowledgeBuildSource,
} from "../../api/knowledgeBuild";

const statusLabels = { queued: "等待构建", researching: "检索来源", synthesizing: "整理与对比", completed: "已完成", failed: "构建失败", cancelled: "已停止", interrupted: "服务重启中断" };
const claimLabels = { corroborated: "跨来源支持", single_source: "单一来源", contradicted: "存在分歧", unverified: "尚未核实" };
const evidenceLabels = { content: "正文", snippet: "搜索摘录", title_only: "仅标题" };
const ongoing = (build: KnowledgeBuild) => ["queued", "researching", "synthesizing"].includes(build.status);
const errorMessage = (cause: unknown) => cause instanceof Error ? cause.message : String(cause);

export default function KnowledgeBuildPanel({ open, onClose, onImported, initialBuildId }: {
  initialBuildId?: string; open: boolean; onClose: () => void; onImported: (documentId: string) => void;
}) {
  const [topic, setTopic] = useState("");
  const [mode, setMode] = useState<KnowledgeBuildMode>("quick");
  const [builds, setBuilds] = useState<KnowledgeBuild[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [refreshVersion, setRefreshVersion] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [tab, setTab] = useState<"structure" | "claims" | "sources">("structure");
  const selected = builds.find(build => build.id === selectedId);
  const update = useCallback((build: KnowledgeBuild) => {
    setBuilds(current => [build, ...current.filter(item => item.id !== build.id)].sort((a, b) => b.created_at - a.created_at));
  }, []);

  useEffect(() => {
    if (!open) return;
    let active = true;
    setLoading(true); setError("");
    void listKnowledgeBuilds().then(async ({ builds: items }) => {
      if (initialBuildId && !items.some(item => item.id === initialBuildId)) {
        const { build } = await getKnowledgeBuild(initialBuildId);
        items = [build, ...items];
      }
      if (!active) return;
      setBuilds(items); setSelectedId(current => initialBuildId ?? current);
    }).catch(cause => { if (active) setError(errorMessage(cause)); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [open, initialBuildId]);

  const runningIds = builds.filter(ongoing).map(build => build.id).sort().join(",");
  useEffect(() => {
    if (!open || !runningIds) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      const responses = await Promise.allSettled(runningIds.split(",").map(getKnowledgeBuild));
      if (!active) return;
      const failures: string[] = [];
      for (const response of responses) {
        if (response.status === "fulfilled") update(response.value.build);
        else failures.push(errorMessage(response.reason));
      }
      setError(failures.join("；"));
      if (!failures.length) timer = setTimeout(() => void poll(), 1600);
    };
    timer = setTimeout(() => void poll(), 1000);
    return () => { active = false; clearTimeout(timer); };
  }, [open, runningIds, update, refreshVersion]);

  async function create() {
    setBusy(true); setError("");
    try {
      const { build } = await createKnowledgeBuild(topic.trim(), mode);
      update(build); setSelectedId(build.id); setTab("structure");
    } catch (cause) { setError(errorMessage(cause)); }
    finally { setBusy(false); }
  }
  async function refresh() {
    setLoading(true); setError("");
    try { const response = await listKnowledgeBuilds(); setBuilds(response.builds); setRefreshVersion(current => current + 1); }
    catch (cause) { setError(errorMessage(cause)); }
    finally { setLoading(false); }
  }
  async function cancel() {
    if (!selected) return;
    setBusy(true); setError("");
    try { update((await cancelKnowledgeBuild(selected.id)).build); }
    catch (cause) { setError(errorMessage(cause)); }
    finally { setBusy(false); }
  }
  async function importResult() {
    if (!selected) return;
    setBusy(true); setError("");
    try {
      const response = await importKnowledgeBuild(selected.id);
      update(response.build); onImported(response.document.id);
    } catch (cause) { setError(errorMessage(cause)); }
    finally { setBusy(false); }
  }

  return <><Modal open={open} onClose={onClose} title="AI 构建知识" size="xl" className="knowledge-build-dialog" footer={<>
    {selected && ongoing(selected) && <Button variant="secondary" disabled={busy} onClick={() => void cancel()}><Square size={14} />停止构建</Button>}
    {selected?.status === "completed" && <Button disabled={busy} loading={busy} onClick={() => selected.document_id ? onImported(selected.document_id) : void importResult()}>{selected.document_id ? <Check size={15} /> : <BookOpen size={15} />}{selected.document_id ? "查看已导入知识" : "导入知识库"}</Button>}
    <Button variant="secondary" onClick={onClose}>关闭</Button>
  </>}>
    <div className="knowledge-build">
      <div className="knowledge-build-compose">
        <div className="knowledge-build-compose-heading"><span>想建立哪方面的知识？</span><div className="knowledge-build-modes" role="group" aria-label="研究深度">{(["quick", "deep"] as const).map(value => <button type="button" key={value} aria-pressed={mode === value} onClick={() => setMode(value)}>{value === "quick" ? "快速" : "深度"}</button>)}</div></div>
        <Textarea aria-label="知识构建主题" value={topic} onChange={event => setTopic(event.target.value)} rows={4} placeholder="例如：构建数据结构知识体系，覆盖复杂度、线性结构、树和图，对比适用条件与常见误区" />
        <div className="knowledge-build-controls">
          <Button disabled={busy || !topic.trim()} loading={busy} onClick={() => void create()}><Sparkles size={15} />开始构建</Button>
        </div>
        <div className="knowledge-build-history-entry"><Button size="sm" variant="ghost" onClick={() => setHistoryOpen(true)}><History size={14} />构建历史</Button>{selected && <Button size="sm" variant="ghost" onClick={() => { setSelectedId(null); setTopic(""); setError(""); }}>新建主题</Button>}</div>
      </div>
      {error && <p role="alert" className="knowledge-library-error">{error}</p>}
      <div className="knowledge-build-workspace">
        <section className="knowledge-build-detail" aria-label="知识构建详情">
          {!selected ? <div className="knowledge-build-rest" /> : <>
            <div className="knowledge-build-heading"><h4>{selected.topic}</h4><Badge tone={selected.status === "completed" ? "success" : selected.status === "failed" ? "danger" : "accent"}>{statusLabels[selected.status]}</Badge></div>
            <div className="knowledge-build-progress" role="status"><span>{selected.progress.stage}</span><span>检索 {selected.progress.queries_completed}/{selected.progress.queries_total} · 来源 {selected.progress.sources_collected} · 研究 {selected.progress.rounds_completed}/{selected.progress.rounds_total} 轮</span></div>
            {selected.error && <p role="alert" className="knowledge-library-error">{selected.error}</p>}
            {selected.result && <p className="knowledge-build-summary">{selected.result.summary}</p>}
            <div className="knowledge-build-tabs" role="group" aria-label="构建结果视图">{([ ["structure", "知识结构"], ["claims", "对比与分歧"], ["sources", `来源 (${selected.sources.length})`] ] as const).map(([id, label]) => <Button key={id} size="sm" variant={tab === id ? "primary" : "ghost"} aria-pressed={tab === id} onClick={() => setTab(id)}>{label}</Button>)}</div>
            {tab === "structure" && (selected.result ? <>
              <KnowledgeHierarchy nodes={selected.result.nodes} sources={selected.sources} />
              {selected.result.edges.length > 0 && <details><summary>知识关联（{selected.result.edges.length}）</summary><ul className="knowledge-build-links">{selected.result.edges.map((edge, index) => <li key={index}>{selected.result!.nodes.find(node => node.id === edge.from)?.title ?? edge.from} → {selected.result!.nodes.find(node => node.id === edge.to)?.title ?? edge.to} · {edge.relation === "prerequisite" ? "前置知识" : "相关知识"}</li>)}</ul></details>}
            </> : <p className="knowledge-library-hint">{ongoing(selected) ? "知识结构将在来源研究与整理完成后显示。" : "本次构建未生成知识结构。"}</p>)}
            {tab === "claims" && (selected.result ? <div className="knowledge-build-claims">
              {selected.result.claims.map(claim => <article key={claim.id}><Badge tone={claim.status === "contradicted" ? "warning" : claim.status === "corroborated" ? "success" : "default"}>{claimLabels[claim.status]}</Badge><p>{claim.text}</p>{claim.evidence.map((evidence, index) => <blockquote key={index}><span>{evidence.stance === "supports" ? "支持" : "反对"} · </span><SourceReference source={selected.sources.find(source => source.id === evidence.source_id)} id={evidence.source_id} /><p>{evidence.quote}</p></blockquote>)}</article>)}
              {selected.result.contradictions.length > 0 && <section><h5>来源分歧</h5>{selected.result.contradictions.map((item, index) => <p key={index}>{item.description}</p>)}</section>}
            </div> : <p className="knowledge-library-hint">对比结论将在研究完成后显示。</p>)}
            {tab === "sources" && <div className="knowledge-build-sources">{selected.result && selected.result.limitations.length > 0 && <details><summary className="cursor-pointer text-xs text-[var(--text-secondary)]">来源详情与资料局限</summary><ul className="mt-2 space-y-1 text-xs text-[var(--text-secondary)]">{selected.result.limitations.map((item, index) => <li key={index}>{item}</li>)}</ul></details>}{selected.sources.length ? selected.sources.map(source => <article key={source.id} id={`knowledge-source-${source.id}`}><SourceReference source={source} id={source.id} /><div><Badge>{evidenceLabels[source.evidence_kind]}</Badge><span>{({preferred:"优先资料",supporting:"辅助参考",unclassified:"类型待确认"})[source.source_priority]}</span><span title={source.source_type_reason}>{source.source_type}</span></div><p>{source.excerpt}</p><small>检索：{source.query}</small><details><summary className="cursor-pointer text-xs">来源分类依据</summary><p>{source.source_type_reason}</p></details></article>) : <p className="knowledge-library-hint">暂未取得来源。</p>}</div>}
          </>}
        </section>
      </div>
    </div>
  </Modal><Modal open={open && historyOpen} onClose={() => setHistoryOpen(false)} title="构建历史" size="lg"><aside className="knowledge-build-history knowledge-build-history-dialog" aria-label="构建记录"><div className="flex items-center justify-between"><h4>全部构建</h4><Button size="sm" variant="ghost" disabled={loading} aria-label="刷新构建记录" onClick={() => void refresh()}><RefreshCw size={14} /></Button></div>{loading && <p role="status">正在读取…</p>}{!loading && !builds.length && <EmptyState title="暂无构建记录" />}{builds.map(build => <button type="button" key={build.id} onClick={() => {setSelectedId(build.id); setTopic(build.topic); setMode(build.mode); setHistoryOpen(false); setTab("structure");}}><strong>{build.topic}</strong><span>{build.mode === "deep" ? "深度" : "快速"} · {statusLabels[build.status]} · {new Date(build.created_at).toLocaleString("zh-CN")}</span></button>)}</aside></Modal></>;
}

function SourceReference({ source, id }: { source?: KnowledgeBuildSource; id: string }) {
  if (!source) return <span>{id}</span>;
  return <a href={source.url} target="_blank" rel="noreferrer" className="knowledge-build-source-link">{source.title || source.url}<ExternalLink size={12} /></a>;
}

function KnowledgeHierarchy({ nodes, sources }: { nodes: KnowledgeBuildNode[]; sources: KnowledgeBuildSource[] }) {
  const children = new Map<string | null, KnowledgeBuildNode[]>();
  for (const node of nodes) children.set(node.parent_id, [...(children.get(node.parent_id) ?? []), node]);
  function branch(parent: string | null): React.ReactNode {
    return <ul className="knowledge-build-tree">{(children.get(parent) ?? []).map(node => <li key={node.id}><details open><summary>{node.title}</summary><p>{node.summary}</p>{node.source_ids.length > 0 && <div className="knowledge-build-citations">{node.source_ids.map(id => <SourceReference key={id} id={id} source={sources.find(source => source.id === id)} />)}</div>}{children.has(node.id) && branch(node.id)}</details></li>)}</ul>;
  }
  return branch(null);
}
