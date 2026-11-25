import { useEffect, useRef, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { BookOpen, Bold, Download, FileText, Heading2, History, Image, Italic, Link2, List, Plus, Save } from "lucide-react";
import PageShell from "../../components/layout/PageShell";
import { Button, EmptyState, Input, Modal, Textarea } from "../../components/ui";
import { downloadArtifact, listWorkspaceArtifacts, previewArtifact, type ArtifactPreview, type ArtifactRecord } from "../../api/evidence";
import { createCreation, getCreation, getCreationVersion, listCreations, saveCreation, type CreationDetail, type CreationHistory, type WorkspaceCreation } from "./api";
import { clearSavedCreationDraft, readCreationDrafts, writeCreationDrafts, type CreationDraft, type CreationDrafts } from "./drafts";
import { getLearning, getLearningNote } from "../learning/api";
import { importKnowledge } from "../../api/knowledge";
import { markdownLineTailPlugins } from "../../utils/markdownLineTail";
import WorkspaceSectionShell from "../workspace-preview/WorkspaceSectionShell";

type Tab = "manuscripts" | "images" | "office";
const officeMimeTypes = new Set(["application/pdf", "application/vnd.openxmlformats-officedocument.wordprocessingml.document", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", "application/vnd.openxmlformats-officedocument.presentationml.presentation"]);

function beginDownload(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url; link.download = name; link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function manuscriptFilename(title: string) {
  return title.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "-") || "文稿";
}

function WorkspaceCreation({ workspaceId, readOnly }: { workspaceId: string; readOnly: boolean }) {
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const requestedCreation = params.get("creation") ?? "";
  const sourceSession = params.get("learning") ?? "";
  const sourceRevision = params.get("revision") ?? "";
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
  const [sourceLoading, setSourceLoading] = useState(false);
  const [sourceLabel, setSourceLabel] = useState("");
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
      setCreations(result.creations); setHasMore(result.has_more); setSelectedId(current => current || requestedCreation || result.creations[0]?.id || "");
    }).catch((cause: unknown) => { if (active) setLoadError(String(cause)); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [workspaceId]);

  useEffect(() => { if (requestedCreation) setSelectedId(requestedCreation); }, [requestedCreation]);

  useEffect(() => {
    if (!sourceSession || readOnly) return;
    const revision = Number(sourceRevision);
    setError(""); setSourceLabel(""); setNewTitle(""); setNewContent(""); setDialog("create");
    if (!Number.isSafeInteger(revision) || revision < 1) { setError("学习笔记版本无效"); return; }
    let active = true;
    setSourceLoading(true);
    void Promise.all([getLearning(workspaceId, sourceSession), getLearningNote(workspaceId, sourceSession, revision)]).then(([learning, note]) => {
      if (!active) return;
      const label = `${learning.session.title} · v${revision}`;
      const target = `/workspaces/${encodeURIComponent(workspaceId)}/learning?${new URLSearchParams({ session: sourceSession, note: String(revision) })}`;
      const linkLabel = label.replace(/[\[\]\\]/g, "\\$&").replace(/[\r\n]+/g, " ");
      setSourceLabel(label);
      setNewTitle(`${Array.from(learning.session.title).slice(0, 194).join("")} · 文稿`);
      setNewContent(`${note.content.trimEnd()}\n\n---\n\n来源：[${linkLabel}](${target})`);
    }).catch(cause => { if (active) setError(cause instanceof Error ? cause.message : String(cause)); })
      .finally(() => { if (active) setSourceLoading(false); });
    return () => { active = false; };
  }, [workspaceId, sourceSession, sourceRevision, readOnly]);

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
    if (!draft || readOnly) return;
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
    setParams({ creation: next.creation.id }, { replace: true });
  });
  function closeCreate() {
    if (pending) return;
    setDialog(null); setSourceLoading(false); setSourceLabel(""); setNewTitle(""); setNewContent(""); setError("");
    if (sourceSession) setParams(current => { const next = new URLSearchParams(current); next.delete("learning"); next.delete("revision"); return next; }, { replace: true });
  }
  const save = () => act("save", async () => {
    if (!draft) return;
    applyDetail(await saveCreation(workspaceId, selectedId, draft.title.trim(), draft.content, draft.baseRevision), draft);
    setNotice("新版本已保存到本地资源库");
  });
  const useForLearning = () => act("learning", async () => {
    if (!detail || !draft) return;
    const saved = dirty ? await saveCreation(workspaceId, selectedId, draft.title.trim(), draft.content, draft.baseRevision) : detail;
    if (dirty) applyDetail(saved, draft);
    const { document } = await importKnowledge(new File([saved.content], `${manuscriptFilename(saved.creation.title)}.md`, { type: "text/markdown" }));
    navigate(`/workspaces/${encodeURIComponent(workspaceId)}/learning?${new URLSearchParams({ document: document.id })}`);
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
    const filename = manuscriptFilename(draft.title);
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
    <div className="lc-creation-toolbar">
      <div className="lc-tabs" role="tablist" aria-label="创作类型">{([{ id: "manuscripts", label: "文稿", icon: FileText }, { id: "images", label: "图片", icon: Image }, { id: "office", label: "办公文档", icon: FileText }] as const).map(({ id, label, icon: Icon }) => <button key={id} type="button" role="tab" id={`creation-tab-${id}`} aria-selected={tab === id} aria-controls="creation-panel" className={tab === id ? "is-selected" : ""} onClick={() => setTab(id)}><Icon size={17} />{label}</button>)}</div>
      {tab === "manuscripts" && !readOnly && <Button size="sm" disabled={Boolean(pending) || Boolean(loadError)} onClick={() => { setError(""); setSourceLabel(""); setDialog("create"); }}><Plus size={16} />新建文稿</Button>}
    </div>
    {loadError && tab === "manuscripts" ? <p className="lc-error" role="alert">{loadError}</p> : null}
    {error && !dialog ? <p className="lc-error" role="alert">{error}</p> : null}
    {notice ? <p className="lc-muted" role="status">{notice}</p> : null}
    {tab === "manuscripts" ? <div className={creations.length ? "lc-workbench-columns" : "lc-creation-content"} id="creation-panel" role="tabpanel" aria-labelledby={`creation-tab-${tab}`}>
      {creations.length > 0 && <aside className="lc-records" aria-label="文稿列表">
        <div className="lc-records-scroll">
          {creations.map((item) => <button key={item.id} type="button" className={`lc-record ${selectedId === item.id ? "is-selected" : ""}`} aria-pressed={selectedId === item.id} disabled={Boolean(pending)} onClick={() => { setSelectedId(item.id); setParams({ creation: item.id }, { replace: true }); }}><span className="lc-record-copy"><strong>{item.title}</strong><small>{new Date(item.updated_at).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })} · v{item.revision}{drafts[item.id] ? " · 有编辑" : ""}</small></span></button>)}
            {hasMore ? <Button size="sm" variant="ghost" disabled={Boolean(pending)} onClick={() => void act("more", async () => { const next = await listCreations(workspaceId, creations.length); setCreations((current) => [...current, ...next.creations]); setHasMore(next.has_more); })}>加载更多文稿</Button> : null}
        </div>
      </aside>}
      <section className="lc-detail" aria-label="文稿编辑器">
        {detail && draft ? <div className="lc-creation-editor">
          <div className="lc-creation-title"><input className="lc-title-input" aria-label="文稿标题" maxLength={200} value={draft.title} disabled={Boolean(pending)} readOnly={readOnly} onChange={(event) => edit({ title: event.target.value })} /><Button size="sm" variant="ghost" disabled={Boolean(pending)} onClick={() => { setHistory(null); setError(""); setDialog("history"); }}><History size={15} />版本历史</Button></div>
          <div className="lc-formatting"><span>Markdown</span>{!preview && !readOnly ? <><Button size="sm" variant="ghost" disabled={Boolean(pending)} aria-label="插入标题" onClick={() => format("## ")}><Heading2 size={16} /></Button><Button size="sm" variant="ghost" disabled={Boolean(pending)} aria-label="加粗" onClick={() => format("**", "**")}><Bold size={16} /></Button><Button size="sm" variant="ghost" disabled={Boolean(pending)} aria-label="斜体" onClick={() => format("*", "*")}><Italic size={16} /></Button><Button size="sm" variant="ghost" disabled={Boolean(pending)} aria-label="插入列表" onClick={() => format("- ")}><List size={16} /></Button><Button size="sm" variant="ghost" disabled={Boolean(pending)} aria-label="插入链接" onClick={() => format("[", "](https://)")}><Link2 size={16} /></Button></> : null}<Button size="sm" variant="ghost" onClick={() => setPreview((current) => !current)}>{preview ? readOnly ? "查看正文" : "返回编辑" : "阅读预览"}</Button></div>
          <div className="lc-document-body">{preview ? <article className="lc-manuscript-preview"><ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={markdownLineTailPlugins} components={{
            a: ({ href, children }) => href?.startsWith("/") && !href.startsWith("//") ? <Link to={href}>{children}</Link> : <a href={href}>{children}</a>,
          }}>{draft.content}</ReactMarkdown></article> : <textarea ref={editorRef} className="lc-manuscript-textarea" aria-label="文稿正文（Markdown）" value={draft.content} maxLength={50000} disabled={Boolean(pending)} readOnly={readOnly} onChange={(event) => edit({ content: event.target.value })} placeholder="从这里开始写作…" />}</div>
          <div className="lc-editor-footer"><span>版本 {detail.creation.revision} · {dirty ? "有未保存编辑" : "已保存"} · {draft.content.length} 字符</span><div className="lc-export-actions">
            <Button size="sm" variant="secondary" disabled={Boolean(pending)} onClick={() => { setError(""); setDialog("export"); }}><Download size={15} />导出</Button>
            {!readOnly && <><Button size="sm" variant="secondary" disabled={Boolean(pending) || !draft.title.trim() || !draft.content.trim()} loading={pending === "learning"} onClick={() => void useForLearning()}><BookOpen size={15} />{dirty ? "保存并用于学习" : "作为学习资料"}</Button>
              <Button size="sm" disabled={Boolean(pending) || !dirty || !draft.title.trim() || !draft.content.trim()} loading={pending === "save"} onClick={() => void save()}><Save size={15} />保存新版本</Button></>}
          </div></div>
        </div> : <div className="lc-detail-scroll">{loading || detailLoading ? <p role="status" className="lc-muted">正在读取文稿…</p> : !loadError && !error ? <EmptyState icon={<FileText size={32} />} title="还没有文稿" /> : null}</div>}
      </section>
    </div> : <section className="lc-detail lc-assets-panel" id="creation-panel" role="tabpanel" aria-labelledby={`creation-tab-${tab}`}>
      <div className="lc-detail-scroll">
        {assetError ? <p className="lc-error" role="alert">{assetError}</p> : artifacts === null ? <p role="status" className="lc-muted">正在读取成果文件…</p> : !filteredArtifacts?.length ? <EmptyState title={`还没有${tab === "images" ? "图片" : "办公文档"}成果`} /> : <div className="lc-asset-list">
          {filteredArtifacts.map((artifact) => <article key={artifact.id}>
            <h3>{artifact.name}</h3>
            <p className="lc-muted">{new Date(artifact.updated_at).toLocaleString()}</p>
            <div className="lc-export-actions">
              <Button size="sm" variant="secondary" disabled={Boolean(pending)} onClick={() => void inspectArtifact(artifact)}>预览与来源</Button>
              <Button size="sm" variant="ghost" disabled={Boolean(pending)} onClick={() => void act("download", async () => { beginDownload(await downloadArtifact(artifact.id), artifact.name); setNotice("已发起原始文件的浏览器下载"); })}>下载原文件</Button>
            </div>
          </article>)}
        </div>}
      </div>
    </section>}
    <Modal title="新建文稿" size="lg" open={dialog === "create"} onClose={closeCreate} footer={<><Button variant="secondary" disabled={Boolean(pending)} onClick={closeCreate}>取消</Button><Button disabled={readOnly || sourceLoading || Boolean(pending) || !newTitle.trim() || !newContent.trim()} loading={pending === "create"} onClick={() => void create()}>创建文稿</Button></>}>
      <div className="space-y-4">
        {sourceLoading && <p role="status" className="lc-muted">正在读取学习笔记…</p>}
        {sourceLabel && <p className="lc-muted">根据学习笔记「{sourceLabel}」开始，可先编辑初稿。</p>}
        <Input label="文稿名称" disabled={sourceLoading || Boolean(pending)} value={newTitle} maxLength={200} onChange={(event) => setNewTitle(event.target.value)} placeholder="给这份想法起个名字" />
        <Textarea label="初稿内容（Markdown）" disabled={sourceLoading || Boolean(pending)} value={newContent} maxLength={50000} rows={10} onChange={(event) => setNewContent(event.target.value)} placeholder="写下第一段内容，创建后可继续编辑。" />
        {error && <p className="lc-error" role="alert">{error}</p>}
      </div>
    </Modal>
    <Modal title="文稿版本历史" size="xl" open={dialog === "history"} onClose={() => { if (!pending) setDialog(null); }} footer={<><Button variant="secondary" onClick={() => setDialog(null)}>关闭</Button>{history && !readOnly ? <Button disabled={Boolean(pending)} onClick={() => { if (!detail) return; setDrafts((current) => ({ ...current, [selectedId]: { title: history.version.title, content: history.content, baseRevision: detail.creation.revision } })); setDialog(null); setPreview(false); }}>载入为新版本草稿</Button> : null}</>}>
      <p className="lc-muted">选择真实保存版本与当前保存内容对照。载入草稿后需点击“保存新版本”，中间版本仍保留。</p><div className="lc-history-choices mt-4">{detail?.versions.map((version) => <Button key={version.id} size="sm" variant={history?.version.id === version.id ? "primary" : "secondary"} disabled={Boolean(pending)} onClick={() => void act("history", async () => setHistory(await getCreationVersion(workspaceId, selectedId, version.id)))}>v{version.revision}{version.current ? " · 当前" : ""} · {new Date(version.created_at).toLocaleString()}</Button>)}</div>
      {pending === "history" ? <p role="status" className="lc-muted">正在读取版本…</p> : null}{history && detail ? <><p className="lc-muted mt-4">{history.content === detail.content ? "这两个版本的正文相同。" : "这两个版本的正文不同，可在下方逐段对照。"}</p><div className="lc-compare"><section><h4>历史版本 v{history.version.revision} · {history.version.title}</h4><pre>{history.content}</pre></section><section><h4>当前保存 v{detail.creation.revision} · {detail.creation.title}</h4><pre>{detail.content}</pre></section></div></> : null}{error ? <p className="lc-error" role="alert">{error}</p> : null}
    </Modal>
    <Modal title="导出文稿" size="sm" open={dialog === "export"} onClose={() => setDialog(null)}><p className="lc-muted">{dirty ? "导出编辑器中的当前草稿，尚未保存的编辑也会包含在内。" : "导出当前已保存版本。"}文件通过浏览器下载；TXT 保留原始文字及 Markdown 标记。</p><div className="lc-export-actions mt-5"><Button onClick={() => exportText("md")}><Download size={15} />Markdown</Button><Button variant="secondary" onClick={() => exportText("txt")}><Download size={15} />TXT</Button></div></Modal>
    <Modal title={assetPreview?.preview.artifact.name ?? "成果预览"} size="lg" open={dialog === "asset"} onClose={() => setDialog(null)}>{assetPreview ? <div className="space-y-4">{assetPreview.imageUrl ? <img src={assetPreview.imageUrl} alt={assetPreview.preview.artifact.name} className="lc-asset-preview" /> : assetPreview.preview.text !== null ? <pre className="lc-history-text">{assetPreview.preview.text}</pre> : <p className="lc-muted">此格式没有文本预览，可下载原文件后在对应应用打开。</p>}{assetPreview.preview.truncated ? <p className="lc-muted">预览已截断，原文件保持完整。</p> : null}<details className="lc-citation-details"><summary>技术详情</summary><div className="lc-asset-meta mt-2">{assetPreview.preview.artifact.mime_type && <span>文件类型：{assetPreview.preview.artifact.mime_type}</span>}<span>产物 ID：{assetPreview.preview.artifact.id}</span><span>来源执行：{assetPreview.preview.artifact.task_execution_id}</span><span>来源任务：{assetPreview.preview.artifact.task_id}</span>{assetPreview.preview.provenance ? <span>原产物版本：v{assetPreview.preview.provenance.version} · SHA256：{assetPreview.preview.provenance.sha256}</span> : <span>此历史成果没有登记版本溯源。</span>}</div></details></div> : null}</Modal>
  </div>;
}

export default function WorkspaceCreationPage() {
  const { id } = useParams<{ id: string }>();
  return id ? <WorkspaceSectionShell key={id} workspaceId={id} active="creation">{workspace => <WorkspaceCreation workspaceId={id} readOnly={workspace.status === "archived"} />}</WorkspaceSectionShell>
    : <PageShell title="创作"><EmptyState title="请选择工作空间" description="创作文稿需要归属于一个工作空间。" /></PageShell>;
}
