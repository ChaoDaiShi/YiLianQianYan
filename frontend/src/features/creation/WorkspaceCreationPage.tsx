import { useEffect, useRef, useState } from "react";
import { Link, useParams } from "react-router-dom";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { ArrowLeft, Bold, Download, FileText, Heading2, History, Image, Italic, Link2, List, Plus, Save, Sparkles } from "lucide-react";
import PageShell from "../../components/layout/PageShell";
import { Button, EmptyState, Input, Modal, Textarea } from "../../components/ui";
import { downloadArtifact, listWorkspaceArtifacts, previewArtifact, type ArtifactPreview, type ArtifactRecord } from "../../api/evidence";
import { createCreation, getCreation, getCreationVersion, listCreations, saveCreation, type CreationDetail, type CreationHistory, type WorkspaceCreation } from "./api";
import { clearSavedCreationDraft, readCreationDrafts, writeCreationDrafts, type CreationDraft, type CreationDrafts } from "./drafts";

type Tab = "manuscripts" | "images" | "office";
const officeMimeTypes = new Set(["application/pdf", "application/vnd.openxmlformats-officedocument.wordprocessingml.document", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", "application/vnd.openxmlformats-officedocument.presentationml.presentation"]);

function beginDownload(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url; link.download = name; link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function WorkspaceCreation({ workspaceId }: { workspaceId: string }) {
  const [tab, setTab] = useState<Tab>("manuscripts");
  const [creations, setCreations] = useState<WorkspaceCreation[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [selectedId, setSelectedId] = useState("");
  const [detail, setDetail] = useState<CreationDetail | null>(null);
  const [drafts, setDrafts] = useState<CreationDrafts>(() => readCreationDrafts(workspaceId, window.sessionStorage));
  const [preview, setPreview] = useState(false);
  const [loading, setLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [pending, setPending] = useState<string | null>(null);
  const [dialog, setDialog] = useState<"create" | "history" | "export" | "asset" | null>(null);
  const [newTitle, setNewTitle] = useState("");
  const [newContent, setNewContent] = useState("");
  const [history, setHistory] = useState<CreationHistory | null>(null);
  const [artifacts, setArtifacts] = useState<ArtifactRecord[] | null>(null);
  const [assetError, setAssetError] = useState("");
  const [assetPreview, setAssetPreview] = useState<{ preview: ArtifactPreview; imageUrl: string | null } | null>(null);
  const editorRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => { writeCreationDrafts(workspaceId, drafts, window.sessionStorage); }, [workspaceId, drafts]);

  useEffect(() => {
    let active = true;
    listCreations(workspaceId).then((result) => {
      if (!active) return;
      setCreations(result.creations); setHasMore(result.has_more); setSelectedId(result.creations[0]?.id ?? "");
    }).catch((cause: unknown) => { if (active) setLoadError(String(cause)); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [workspaceId]);

  useEffect(() => {
    if (!selectedId) return;
    let active = true;
    setDetailLoading(true); setDetail(null); setError(""); setNotice(""); setHistory(null);
    getCreation(workspaceId, selectedId).then((next) => { if (active) setDetail(next); }).catch((cause: unknown) => { if (active) setError(String(cause)); }).finally(() => { if (active) setDetailLoading(false); });
    return () => { active = false; };
  }, [workspaceId, selectedId]);

  useEffect(() => {
    if (tab === "manuscripts") return;
    let active = true;
    setAssetError(""); setArtifacts(null);
    listWorkspaceArtifacts(workspaceId).then((result) => { if (active) setArtifacts(result); }).catch((cause: unknown) => { if (active) setAssetError(String(cause)); });
    return () => { active = false; };
  }, [tab, workspaceId]);

  useEffect(() => () => { if (assetPreview?.imageUrl) URL.revokeObjectURL(assetPreview.imageUrl); }, [assetPreview]);

  const draft = drafts[selectedId] ?? (detail ? { title: detail.creation.title, content: detail.content, baseRevision: detail.creation.revision } : null);
  const dirty = Boolean(detail && draft && (draft.title !== detail.creation.title || draft.content !== detail.content));
  function edit(patch: Partial<Pick<CreationDraft, "title" | "content">>) {
    if (!draft) return;
    setDrafts((current) => ({ ...current, [selectedId]: { ...draft, ...patch } })); setNotice("");
  }
  async function act(key: string, operation: () => Promise<void>) {
    if (pending) return;
    setPending(key); setError(""); setNotice("");
    try { await operation(); } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { setPending(null); }
  }
  function applyDetail(next: CreationDetail, submitted?: CreationDraft) {
    if (submitted) clearSavedCreationDraft(workspaceId, next.creation.id, window.sessionStorage, submitted);
    setDetail(next);
    setCreations((items) => [next.creation, ...items.filter((item) => item.id !== next.creation.id)]);
    setDrafts((current) => { const copy = { ...current }; delete copy[next.creation.id]; return copy; });
  }
  const create = () => act("create", async () => {
    const next = await createCreation(workspaceId, newTitle.trim(), newContent);
    applyDetail(next); setSelectedId(next.creation.id); setDialog(null); setNewTitle(""); setNewContent(""); setTab("manuscripts");
  });
  const save = () => act("save", async () => {
    if (!draft) return;
    applyDetail(await saveCreation(workspaceId, selectedId, draft.title.trim(), draft.content, draft.baseRevision), draft);
    setNotice("新版本已保存到本地资源库");
  });
  function format(prefix: string, suffix = "") {
    const editor = editorRef.current;
    if (!editor || !draft || pending) return;
    const start = editor.selectionStart, end = editor.selectionEnd;
    const replacement = `${prefix}${draft.content.slice(start, end)}${suffix}`;
    edit({ content: draft.content.slice(0, start) + replacement + draft.content.slice(end) });
    requestAnimationFrame(() => { editor.focus(); editor.setSelectionRange(start + prefix.length, end + prefix.length); });
  }
  function exportText(extension: "md" | "txt") {
    if (!draft) return;
    const filename = draft.title.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "-") || "文稿";
    beginDownload(new Blob([draft.content], { type: extension === "md" ? "text/markdown;charset=utf-8" : "text/plain;charset=utf-8" }), `${filename}.${extension}`);
    setDialog(null); setNotice(`已发起 ${extension === "md" ? "Markdown" : "TXT"} 浏览器下载`);
  }
  const filteredArtifacts = artifacts?.filter((artifact) => tab === "images" ? artifact.mime_type?.startsWith("image/") : Boolean(artifact.mime_type && officeMimeTypes.has(artifact.mime_type)));
  const inspectArtifact = (artifact: ArtifactRecord) => act("asset", async () => {
    const next = await previewArtifact(artifact.id);
    const imageUrl = artifact.mime_type?.startsWith("image/") ? URL.createObjectURL(await downloadArtifact(artifact.id)) : null;
    setAssetPreview({ preview: next, imageUrl }); setDialog("asset");
  });

  return <div className="lc-workbench creation-workbench">
    <div className="lc-workbench-heading"><p>写下想法，保留每一次修改。</p><Button size="sm" disabled={Boolean(pending) || Boolean(loadError)} onClick={() => { setError(""); setDialog("create"); }}><Plus size={16} />新建创作</Button></div>
    <div className="lc-tabs" role="tablist" aria-label="创作类型">{([{ id: "manuscripts", label: "文稿", icon: FileText }, { id: "images", label: "图片", icon: Image }, { id: "office", label: "办公文档", icon: FileText }] as const).map(({ id, label, icon: Icon }) => <button key={id} type="button" role="tab" id={`creation-tab-${id}`} aria-selected={tab === id} aria-controls="creation-panel" className={tab === id ? "is-selected" : ""} onClick={() => setTab(id)}><Icon size={17} />{label}</button>)}</div>
    {loadError && tab === "manuscripts" ? <p className="lc-error" role="alert">{loadError}</p> : null}
    {error && !dialog ? <p className="lc-error" role="alert">{error}</p> : null}
    {notice ? <p className="lc-muted" role="status">{notice}</p> : null}
    <div className="lc-workbench-columns" id="creation-panel" role="tabpanel" aria-labelledby={`creation-tab-${tab}`}>
      <aside className="lc-records" aria-label={tab === "manuscripts" ? "创作文稿" : "成果说明"}>
        <div className="lc-panel-heading"><span>{tab === "manuscripts" ? "我的文稿" : "工作空间成果"}</span><span>{tab === "manuscripts" ? `${creations.length} 份已加载` : "真实文件"}</span></div>
        <div className="lc-records-scroll">
          {tab === "manuscripts" ? <>{loading ? <p className="lc-muted lc-padding" role="status">正在读取文稿…</p> : null}{creations.map((item) => <button key={item.id} type="button" className={`lc-record ${selectedId === item.id ? "is-selected" : ""}`} aria-pressed={selectedId === item.id} disabled={Boolean(pending)} onClick={() => setSelectedId(item.id)}><span className="lc-record-icon"><FileText size={20} /></span><span className="lc-record-copy"><strong>{item.title}</strong><small>{new Date(item.updated_at).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })} · v{item.revision}{drafts[item.id] ? " · 有编辑" : ""}</small></span></button>)}
            {hasMore ? <Button size="sm" variant="ghost" disabled={Boolean(pending)} onClick={() => void act("more", async () => { const next = await listCreations(workspaceId, creations.length); setCreations((current) => [...current, ...next.creations]); setHasMore(next.has_more); })}>加载更多文稿</Button> : null}
            {!loading && !loadError && !creations.length ? <p className="lc-muted lc-padding">还没有文稿。新建后可在这里继续编辑。</p> : null}</> : <div className="lc-padding"><p className="lc-muted">这里展示当前工作空间已登记的{tab === "images" ? "图片" : "办公文档"}成果，保留原始产物和执行来源。</p><p className="lc-muted mt-3">生成与编辑服务的真实结果会由原任务登记；此处提供预览、来源与下载。</p></div>}
        </div><div className="lc-records-foot"><Sparkles size={16} /><span>{tab === "manuscripts" ? "正文与版本保存在本地" : "从已保存的成果继续"}</span></div>
      </aside>
      <section className="lc-detail" aria-label={tab === "manuscripts" ? "文稿编辑器" : "成果文件列表"}>
        {tab === "manuscripts" ? detail && draft ? <div className="lc-creation-editor">
          <div className="lc-creation-title"><input className="lc-title-input" aria-label="文稿标题" maxLength={200} value={draft.title} disabled={Boolean(pending)} onChange={(event) => edit({ title: event.target.value })} /><Button size="sm" variant="ghost" disabled={Boolean(pending)} onClick={() => { setHistory(null); setError(""); setDialog("history"); }}><History size={15} />版本历史</Button></div>
          <div className="lc-formatting"><span>Markdown</span>{!preview ? <><Button size="sm" variant="ghost" disabled={Boolean(pending)} aria-label="插入标题" onClick={() => format("## ")}><Heading2 size={16} /></Button><Button size="sm" variant="ghost" disabled={Boolean(pending)} aria-label="加粗" onClick={() => format("**", "**")}><Bold size={16} /></Button><Button size="sm" variant="ghost" disabled={Boolean(pending)} aria-label="斜体" onClick={() => format("*", "*")}><Italic size={16} /></Button><Button size="sm" variant="ghost" disabled={Boolean(pending)} aria-label="插入列表" onClick={() => format("- ")}><List size={16} /></Button><Button size="sm" variant="ghost" disabled={Boolean(pending)} aria-label="插入链接" onClick={() => format("[", "](https://)")}><Link2 size={16} /></Button></> : null}<Button size="sm" variant="ghost" onClick={() => setPreview((current) => !current)}>{preview ? "返回编辑" : "阅读预览"}</Button></div>
          <div className="lc-document-body">{preview ? <article className="lc-manuscript-preview"><ReactMarkdown remarkPlugins={[remarkGfm]}>{draft.content}</ReactMarkdown></article> : <textarea ref={editorRef} className="lc-manuscript-textarea" aria-label="文稿正文（Markdown）" value={draft.content} maxLength={50000} disabled={Boolean(pending)} onChange={(event) => edit({ content: event.target.value })} placeholder="从这里开始写作…" />}</div>
          <div className="lc-editor-footer"><span>版本 {detail.creation.revision} · {dirty ? "有未保存编辑" : "已保存"} · {draft.content.length} 字符</span><div className="lc-export-actions"><Button size="sm" variant="secondary" disabled={Boolean(pending)} onClick={() => { setError(""); setDialog("export"); }}><Download size={15} />导出</Button><Button size="sm" disabled={Boolean(pending) || !dirty || !draft.title.trim() || !draft.content.trim()} loading={pending === "save"} onClick={() => void save()}><Save size={15} />保存新版本</Button></div></div>
        </div> : <div className="lc-detail-scroll">{loading || detailLoading ? <p role="status" className="lc-muted">正在读取文稿…</p> : !loadError && !error ? <EmptyState icon={<FileText size={32} />} title="让想法有一个落点" description="新建一份文稿，用 Markdown 写作、查看版本，再导出为文件。" action={<Button size="sm" onClick={() => setDialog("create")}><Plus size={15} />新建文稿</Button>} /> : null}</div> : <div className="lc-detail-scroll">
          <div className="lc-detail-title"><div><span className="lc-eyebrow">已保存的工作空间成果</span><h2>{tab === "images" ? "图片" : "办公文档"}</h2></div></div>
          {assetError ? <p className="lc-error" role="alert">{assetError}</p> : artifacts === null ? <p role="status" className="lc-muted">正在读取成果文件…</p> : !filteredArtifacts?.length ? <EmptyState title={`还没有${tab === "images" ? "图片" : "办公文档"}成果`} description="原任务产生并保存真实文件后，会在这里显示。" /> : <div className="lc-asset-list">{filteredArtifacts.map((artifact) => <article key={artifact.id}><h3>{artifact.name}</h3><p className="lc-muted">{artifact.mime_type} · {new Date(artifact.updated_at).toLocaleString()}</p><div className="lc-export-actions"><Button size="sm" variant="secondary" disabled={Boolean(pending)} onClick={() => void inspectArtifact(artifact)}>预览与来源</Button><Button size="sm" variant="ghost" disabled={Boolean(pending)} onClick={() => void act("download", async () => { beginDownload(await downloadArtifact(artifact.id), artifact.name); setNotice("已发起原始文件的浏览器下载"); })}><Download size={15} />下载原文件</Button></div></article>)}</div>}
        </div>}
      </section>
    </div>
    <Modal title="新建创作" size="lg" open={dialog === "create"} onClose={() => { if (!pending) setDialog(null); }} footer={<><Button variant="secondary" disabled={Boolean(pending)} onClick={() => setDialog(null)}>取消</Button><Button disabled={Boolean(pending) || !newTitle.trim() || !newContent.trim()} loading={pending === "create"} onClick={() => void create()}>创建文稿</Button></>}><div className="space-y-4"><Input label="文稿名称" disabled={Boolean(pending)} value={newTitle} maxLength={200} onChange={(event) => setNewTitle(event.target.value)} placeholder="给这份想法起个名字" /><Textarea label="初稿内容（Markdown）" disabled={Boolean(pending)} value={newContent} maxLength={50000} rows={10} onChange={(event) => setNewContent(event.target.value)} placeholder="写下第一段内容，创建后可继续编辑。" /><p className="lc-muted">正文通过现有资源服务保存；每次保存产生独立版本，不覆盖历史内容。</p>{error ? <p className="lc-error" role="alert">{error}</p> : null}</div></Modal>
    <Modal title="文稿版本历史" size="xl" open={dialog === "history"} onClose={() => { if (!pending) setDialog(null); }} footer={<><Button variant="secondary" onClick={() => setDialog(null)}>关闭</Button>{history ? <Button disabled={Boolean(pending)} onClick={() => { if (!detail) return; setDrafts((current) => ({ ...current, [selectedId]: { title: history.version.title, content: history.content, baseRevision: detail.creation.revision } })); setDialog(null); setPreview(false); }}>载入为新版本草稿</Button> : null}</>}>
      <p className="lc-muted">选择真实保存版本与当前保存内容对照。载入草稿后需点击“保存新版本”，中间版本仍保留。</p><div className="lc-history-choices mt-4">{detail?.versions.map((version) => <Button key={version.id} size="sm" variant={history?.version.id === version.id ? "primary" : "secondary"} disabled={Boolean(pending)} onClick={() => void act("history", async () => setHistory(await getCreationVersion(workspaceId, selectedId, version.id)))}>v{version.revision}{version.current ? " · 当前" : ""} · {new Date(version.created_at).toLocaleString()}</Button>)}</div>
      {pending === "history" ? <p role="status" className="lc-muted">正在读取版本…</p> : null}{history && detail ? <><p className="lc-muted mt-4">{history.content === detail.content ? "这两个版本的正文相同。" : "这两个版本的正文不同，可在下方逐段对照。"}</p><div className="lc-compare"><section><h4>历史版本 v{history.version.revision} · {history.version.title}</h4><pre>{history.content}</pre></section><section><h4>当前保存 v{detail.creation.revision} · {detail.creation.title}</h4><pre>{detail.content}</pre></section></div></> : null}{error ? <p className="lc-error" role="alert">{error}</p> : null}
    </Modal>
    <Modal title="导出文稿" size="sm" open={dialog === "export"} onClose={() => setDialog(null)}><p className="lc-muted">{dirty ? "导出编辑器中的当前草稿，尚未保存的编辑也会包含在内。" : "导出当前已保存版本。"}文件通过浏览器下载；TXT 保留原始文字及 Markdown 标记。</p><div className="lc-export-actions mt-5"><Button onClick={() => exportText("md")}><Download size={15} />Markdown</Button><Button variant="secondary" onClick={() => exportText("txt")}><Download size={15} />TXT</Button></div></Modal>
    <Modal title={assetPreview?.preview.artifact.name ?? "成果预览"} size="lg" open={dialog === "asset"} onClose={() => setDialog(null)}>{assetPreview ? <div className="space-y-4">{assetPreview.imageUrl ? <img src={assetPreview.imageUrl} alt={assetPreview.preview.artifact.name} className="lc-asset-preview" /> : assetPreview.preview.text !== null ? <pre className="lc-history-text">{assetPreview.preview.text}</pre> : <p className="lc-muted">此格式没有文本预览，可下载原文件后在对应应用打开。</p>}{assetPreview.preview.truncated ? <p className="lc-muted">预览已截断，原文件保持完整。</p> : null}<div className="lc-asset-meta"><span>产物 ID：{assetPreview.preview.artifact.id}</span><span>来源执行：{assetPreview.preview.artifact.task_execution_id}</span><span>来源任务：{assetPreview.preview.artifact.task_id}</span>{assetPreview.preview.provenance ? <span>原产物版本：v{assetPreview.preview.provenance.version} · SHA256：{assetPreview.preview.provenance.sha256}</span> : <span>此历史成果没有登记版本溯源。</span>}</div></div> : null}</Modal>
  </div>;
}

export default function WorkspaceCreationPage() {
  const { id } = useParams<{ id: string }>();
  return <PageShell title={<div className="lc-page-title"><FileText size={22} /><h1>创作</h1></div>} variant="workspace" actions={<Link className="lc-back-link" to={id ? `/workspaces/${encodeURIComponent(id)}` : "/workspaces"}><ArrowLeft size={15} />工作空间</Link>}>{id ? <WorkspaceCreation key={id} workspaceId={id} /> : <EmptyState title="请选择工作空间" description="创作文稿需要归属于一个工作空间。" />}</PageShell>;
}
