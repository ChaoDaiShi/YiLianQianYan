import { useEffect, useRef, useState, type ReactNode } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { FileText, Plus, Upload } from "lucide-react";
import { Button, ConfirmDialog, Input, Modal, Select, Textarea } from "../../components/ui";
import { createConversation } from "../../api/conversations";
import { createTask } from "../../api/tasks";
import { ingestResource } from "../../api/resources";
import { getKnowledge, listKnowledge, type KnowledgeDocument } from "../../api/knowledge";
import {
  createPage, duplicatePage, exportPageMarkdown, listPageHistory, loadPage, movePageToTrash,
  restorePage, restorePageRevision, updatePageBlocks, updatePageMetadata,
  type BlockOperation, type PageBlock, type PageRevision, type WorkspacePage,
} from "../../api/workspacePages";
import { requestResult } from "../../core/api/http";
import type { Workspace } from "../../api/workspaces";
import type { AttachmentReference } from "../../types";
import PageTree, { type PageTreeView } from "./PageTree";
import PageEditor from "./PageEditor";
import PagePicker from "./PagePicker";
import usePageDocument from "./usePageDocument";
import "./workspacePages.css";

const templates = [
  { value: "blank", label: "空白页面", markdown: "" },
  { value: "meeting", label: "会议记录", markdown: "## 议题\n\n## 决议\n\n## 待办\n\n- [ ] " },
  { value: "project", label: "项目计划", markdown: "## 目标\n\n## 里程碑\n\n## 下一步\n\n- [ ] " },
];

type Dialog = "create" | "details" | "move" | "link" | "knowledge" | "history" | "task" | null;
const sources: Record<string, string> = {
  create: "创建页面", metadata: "页面属性", user: "编辑内容", duplicate: "复制页面", restore_revision: "恢复版本",
  trash: "放入回收站", restore_trash: "从回收站恢复", assistant_proposal: "应用 AI 建议", chat_artifact: "保存聊天成果",
};

export default function WorkspacePagesPanel({ workspace, renderResource, onTaskCreated }: {
  workspace: Workspace; renderResource: (file: AttachmentReference) => ReactNode; onTaskCreated: () => void;
}) {
  const [search, setSearch] = useSearchParams();
  const pageId = search.get("page");
  const [query, setQuery] = useState("");
  const [view, setView] = useState<PageTreeView>("tree");
  const [epoch, setEpoch] = useState(0);
  const [dialog, setDialog] = useState<Dialog>(null);
  const [target, setTarget] = useState<WorkspacePage>();
  const [name, setName] = useState("");
  const [taskDescription, setTaskDescription] = useState("");
  const [icon, setIcon] = useState("");
  const [cover, setCover] = useState<File>();
  const [removeCover, setRemoveCover] = useState(false);
  const [template, setTemplate] = useState(templates[0].value);
  const [busy, setBusy] = useState(false);
  const [managementError, setManagementError] = useState("");
  const [notice, setNotice] = useState("");
  const [trashTarget, setTrashTarget] = useState<WorkspacePage>();
  const [confirmTrash, setConfirmTrash] = useState(false);
  const [discardDraft, setDiscardDraft] = useState(false);
  const [mobileTree, setMobileTree] = useState(!pageId);
  const [history, setHistory] = useState<PageRevision[]>([]);
  const [historyEnd, setHistoryEnd] = useState(false);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [knowledge, setKnowledge] = useState<KnowledgeDocument[]>([]);
  const [knowledgeId, setKnowledgeId] = useState("");
  const importInput = useRef<HTMLInputElement>(null);
  const state = usePageDocument(pageId, workspace.id);
  const { document } = state;
  const readOnly = workspace.status === "archived";
  const refresh = () => setEpoch(value => value + 1);
  const selectId = (id?: string) => {
    setSearch(current => {
      const next = new URLSearchParams(current);
      next.set("tab", "pages");
      if (id) next.set("page", id); else next.delete("page");
      return next;
    }, { replace: true });
    setMobileTree(!id);
  };
  const select = async (id: string) => { if (await state.save()) selectId(id); };
  const navigate = useNavigate();

  useEffect(() => {
    setDialog(null);
    setConfirmTrash(false);
    setManagementError("");
  }, [pageId]);

  const beginCreate = (parent?: WorkspacePage) => {
    setTarget(parent); setName(""); setTemplate("blank"); setManagementError(""); setDialog("create");
  };
  const openDetails = () => {
    if (!document) return;
    setName(document.title); setIcon(document.icon ?? ""); setCover(undefined); setRemoveCover(false);
    setManagementError(""); setDialog("details");
  };
  const openPicker = (kind: "move" | "link") => { setTarget(undefined); setManagementError(""); setDialog(kind); };
  const openKnowledge = async () => {
    setKnowledge([]); setKnowledgeId(""); setManagementError(""); setDialog("knowledge"); setBusy(true);
    try {
      const result = await listKnowledge();
      setKnowledge(result.documents.filter(item => item.char_count > 0));
    } catch (error) {
      setManagementError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };
  const linkKnowledge = async () => {
    if (!knowledgeId) return;
    setManagementError(""); setBusy(true);
    try {
      const result = await getKnowledge(knowledgeId, false);
      if (await insert({
        id:crypto.randomUUID(), type:"knowledge_link", document_id:result.document.id, name:result.document.name,
        version:result.version, end_char:result.document.char_count,
      })) setDialog(null);
    } catch (error) {
      setManagementError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };
  const updateBlocks = async (operations: BlockOperation[]) => {
    const result = await state.mutate(page => updatePageBlocks(page.id, page.revision, operations));
    return Boolean(result);
  };
  const insert = (block: PageBlock) => updateBlocks([{ op: "insert", block }]);
  const openHistory = async (offset = 0) => {
    if (!document) return;
    if (!offset) { setHistory([]); setHistoryEnd(false); setDialog("history"); }
    setHistoryLoading(true); setManagementError("");
    const result = await listPageHistory(document.id, offset);
    setHistoryLoading(false);
    if (!result.ok) { setManagementError(result.error); return; }
    setHistory(current => offset ? [...current, ...result.data] : result.data);
    setHistoryEnd(result.data.length < 100);
  };

  const create = async (markdown?: { title: string; content: string }) => {
    if (!(await state.save())) return;
    setBusy(true); setManagementError("");
    const result = await createPage({
      workspace_id: workspace.id,
      parent_page_id: markdown ? undefined : target?.id,
      title: markdown?.title ?? name.trim(),
      markdown: markdown?.content ?? templates.find(item => item.value === template)?.markdown,
    });
    setBusy(false);
    if (!result.ok) { setManagementError(result.error); return; }
    setDialog(null); setView("tree"); setQuery(""); refresh(); selectId(result.data.id);
    setNotice(markdown ? "已导入 Markdown。" : "已创建页面。");
  };

  const move = async (id: string, parent: string | null, before?: string) => {
    if (!(await state.save())) return false;
    setBusy(true); setManagementError("");
    const current = state.current.current;
    if (current?.id === id) {
      const result = await state.mutate(page => updatePageMetadata(id, {
        revision: page.revision, parent_page_id: parent, before_page_id: before,
      }));
      setBusy(false);
      if (!result) return false;
    } else {
      const loaded = await loadPage(id);
      if (!loaded.ok) { setBusy(false); setManagementError(loaded.error); return false; }
      const result = await updatePageMetadata(id, { revision:loaded.data.revision, parent_page_id:parent, before_page_id:before });
      setBusy(false);
      if (!result.ok) { setManagementError(result.error); return false; }
    }
    refresh(); setNotice("页面位置已更新。"); return true;
  };

  const saveDetails = async () => {
    if (!document) return;
    setBusy(true);
    setManagementError("");
    let coverId = removeCover ? "" : undefined;
    if (cover) {
      const resource = await ingestResource(cover);
      if (!resource) { setBusy(false); setManagementError("封面上传失败。"); return; }
      coverId = resource.id;
    }
    const updated = await state.mutate(page => updatePageMetadata(page.id, {
      revision:page.revision, title:name.trim(), icon:icon.trim(), cover_resource_id:coverId,
    }));
    setBusy(false);
    if (updated) { refresh(); setDialog(null); }
  };

  const exportMarkdown = async () => {
    const page = state.current.current;
    if (!page || !(await state.save())) return;
    const result = await exportPageMarkdown(page.id);
    if (!result.ok) { setManagementError(result.error); return; }
    const url = URL.createObjectURL(result.data);
    const link = window.document.createElement("a");
    link.href = url;
    link.download = page.title.replace(/[<>:"/\\|?*]/g, "_") + ".md";
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    setNotice("已导出 Markdown。");
  };

  const collaborate = async (proposal: boolean) => {
    if (!(await state.save()) || !state.current.current) return;
    const page = state.current.current;
    setBusy(true); setManagementError("");
    try {
      const conversation = await createConversation(page.title, { useDefaultWorkspace: false });
      const bound = await requestResult<{ workspace_id: string }>("PUT", "/api/conversations/" + encodeURIComponent(conversation.id) + "/workspace", { workspace_id: workspace.id });
      if (!bound.ok) { setManagementError(bound.error); return; }
      const blocks = page.blocks.map(block => {
        if (block.type === "markdown") return { block_id: block.id, markdown: block.text };
        if (block.type === "page_link") return { type: block.type, page_id: block.page_id };
        if (block.type === "knowledge_link") return { type: block.type, name:block.name, document_id:block.document_id, version:block.version };
        if (block.type === "resource") return { type: block.type, name: block.name, resource_id: block.resource_id };
        return { type: block.type, message_id: block.message_id };
      });
      const context = JSON.stringify({ page_id: page.id, revision: page.revision, title: page.title, blocks }, null, 2);
      const instruction = proposal
        ? "请阅读下列页面资料。根据我的要求生成 yilian-rich 的 page_proposal，由我审查后应用。\n修改要求："
        : "请根据下列页面资料回答我的问题。\n问题：";
      navigate("/chat/" + encodeURIComponent(conversation.id), { state: { suggestedText: instruction + "\n\n页面资料：\n" + context } });
    } catch (error) {
      setManagementError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const createPageTask = async () => {
    if (!(await state.save()) || !state.current.current) return;
    const page = state.current.current;
    setBusy(true); setManagementError("");
    const result = await createTask({ workspace_id:workspace.id, title:name.trim(), description:taskDescription,
      page_source:{ page_id:page.id, revision:page.revision } });
    setBusy(false);
    if (!result.ok) { setManagementError(result.error); return; }
    setDialog(null);
    onTaskCreated();
    navigate(`/workspaces/${encodeURIComponent(workspace.id)}?tab=tasks&task=${encodeURIComponent(result.data.id)}`);
  };

  const addResource = async (file: File) => {
    setBusy(true); setManagementError("");
    const resource = await ingestResource(file);
    if (resource) {
      await insert({ id:crypto.randomUUID(), type:"resource", resource_id:resource.id, name:resource.name, mime_type:resource.mime_type });
    } else setManagementError("附件上传失败。");
    setBusy(false);
  };

  const remove = async () => {
    if (!(await state.save()) || !state.current.current) return;
    setManagementError("");
    const page = state.current.current;
    const result = await movePageToTrash(page.id, page.revision);
    if (!result.ok) { setManagementError(result.error); return; }
    setConfirmTrash(false); refresh(); selectId();
    setNotice("页面及其子页面已移入回收站。");
  };

  return <div className="workspace-pages" data-mobile-tree={mobileTree || undefined}>
    <aside className="workspace-pages-tree" aria-label="工作空间页面树">
      <header>
        <div className="workspace-page-toolbar">
          {pageId && <Button size="sm" variant="ghost" className="workspace-pages-mobile-nav" onClick={() => setMobileTree(false)}>返回编辑</Button>}
          <Button size="sm" disabled={readOnly || busy} onClick={() => beginCreate()}><Plus size={15} />新建页面</Button>
          <Button size="sm" variant="ghost" disabled={readOnly || busy} onClick={() => importInput.current?.click()}><Upload size={14} />导入 Markdown</Button>
          <input ref={importInput} type="file" className="sr-only" tabIndex={-1} accept=".md,.markdown,text/markdown" onChange={async event => {
            const file = event.target.files?.[0];
            event.target.value = "";
            if (file) await create({ title:file.name.replace(/\.(?:md|markdown)$/i, ""), content:await file.text() });
          }} />
        </div>
        <Input label="搜索页面" placeholder="标题或正文" value={query} onChange={event => setQuery(event.target.value)} disabled={view === "trash"} />
        <Select label="页面范围" size="sm" value={view} onChange={event => { setView(event.target.value as PageTreeView); setQuery(""); }}
          options={[{value:"tree",label:"页面树"},{value:"favorites",label:"收藏"},{value:"recent",label:"最近访问"},{value:"trash",label:"回收站"}]} />
      </header>
      <div className="workspace-pages-tree-body scrollbar-thin">
        <PageTree workspace={workspace.id} query={query} view={view} epoch={epoch} selected={pageId ?? undefined} writable={!readOnly && !busy}
          onSelect={page => view === "trash" ? setTrashTarget(page) : void select(page.id)}
          onCreateChild={parent => beginCreate(parent)} onMove={(id, parent, before) => void move(id, parent, before)} />
      </div>
    </aside>

    <div className="workspace-pages-main">
      <div className="workspace-page-feedback">
        <Button size="sm" variant="ghost" className="workspace-pages-mobile-nav" onClick={() => setMobileTree(value => !value)}>{mobileTree ? "返回编辑" : "页面列表"}</Button>
        {state.status === "loading" ? <p role="status">正在打开页面…</p> :
          state.status === "saving" ? <p role="status">正在保存…</p> :
          state.status === "edited" ? <p role="status">有待保存的修改</p> :
          state.status === "saved" ? <p role="status">已保存 · 版本 {document?.revision}</p> :
          notice && <p role="status">{notice}</p>}
        {(managementError || state.error) && <div role="alert" className="workspace-page-error">
          {managementError || state.error}
          {state.status === "conflict" && <Button size="sm" variant="secondary" onClick={() => setDiscardDraft(true)}>读取最新版本</Button>}
          {state.status === "error" && Object.keys(state.drafts).length > 0 && <Button size="sm" variant="secondary" onClick={() => void state.save()}>重试保存</Button>}
          {state.status === "error" && !document && pageId && <Button size="sm" variant="secondary" onClick={() => void state.reload(pageId)}>重试读取</Button>}
        </div>}
      </div>
      {document ? <PageEditor key={document.id} document={document} drafts={state.drafts} viewOnly={readOnly} renderResource={renderResource}
        busy={busy || state.status === "saving"} onEdit={state.edit}
        onInsert={text => void insert({ id:crypto.randomUUID(), type:"markdown", text })} onRemove={id => void updateBlocks([{ op:"remove", id }])}
        onMove={(id, before) => void updateBlocks([{ op:"move", id, before }])}
        onSelect={id => void select(id)} onReference={() => openPicker("link")} onKnowledge={() => void openKnowledge()}
        onResource={file => void addResource(file)} onCover={openDetails}
        toolbar={<>
          {readOnly ? <Button size="sm" variant="secondary" onClick={() => void exportMarkdown()}>导出 Markdown</Button> :
            <Select size="sm" aria-label="页面操作" value="" disabled={busy || state.status === "saving"} options={[
              {value:"",label:"页面操作"},{value:"details",label:"页面属性"},
              {value:"favorite",label:document.favorite ? "取消收藏" : "收藏页面"},{value:"child",label:"新建子页面"},
              {value:"move",label:"移动页面"},{value:"duplicate",label:"复制页面"},{value:"history",label:"版本记录"},
              {value:"export",label:"导出 Markdown"},{value:"chat",label:"作为聊天上下文"},{value:"ai",label:"让 AI 提议修改"},
              {value:"task",label:"从页面创建任务"},{value:"trash",label:"移入回收站"},
            ]} onChange={async event => {
              const action = event.target.value;
              if (action === "details") openDetails();
              if (action === "child") beginCreate(document);
              if (action === "move") openPicker("move");
              if (action === "history") await openHistory();
              if (action === "export") await exportMarkdown();
              if (action === "chat" || action === "ai") await collaborate(action === "ai");
              if (action === "task") { setName(document.title); setTaskDescription(""); setManagementError(""); setDialog("task"); }
              if (action === "trash") { setConfirmTrash(true); setManagementError(""); }
              if (action === "favorite") {
                if (await state.mutate(page => updatePageMetadata(page.id, {revision:page.revision, favorite:!page.favorite}))) refresh();
              }
              if (action === "duplicate" && await state.save() && state.current.current) {
                const current = state.current.current;
                setBusy(true);
                const result = await duplicatePage(current.id, current.revision);
                setBusy(false);
                if (result.ok) { refresh(); selectId(result.data.id); }
                else setManagementError(result.error);
              }
            }} />}
        </>} /> : !pageId && <div className="workspace-page-empty"><FileText size={30} /><p>从页面树打开一个页面开始编辑。</p></div>}
    </div>

    <Modal open={dialog !== null} title={
      dialog === "create" ? target ? "新建子页面" : "新建页面" :
      dialog === "details" ? "页面属性" : dialog === "move" ? "移动页面" : dialog === "link" ? "引用页面" :
      dialog === "knowledge" ? "关联知识文档" : dialog === "task" ? "从页面创建任务" : "版本记录"
    } size={dialog === "history" ? "lg" : "md"} closeDisabled={busy} onClose={() => setDialog(null)}
      footer={<>
        <Button variant="secondary" disabled={busy} onClick={() => setDialog(null)}>{dialog === "history" ? "关闭" : "取消"}</Button>
        {dialog === "create" && <Button loading={busy} disabled={!name.trim()} onClick={() => void create()}>创建页面</Button>}
        {dialog === "details" && <Button loading={busy} disabled={!name.trim()} onClick={() => void saveDetails()}>保存属性</Button>}
        {dialog === "move" && <Button loading={busy} onClick={async () => {
          if (document && await move(document.id, target?.id ?? null)) setDialog(null);
        }}>{target ? "移入所选页面" : "移到根层级"}</Button>}
        {dialog === "link" && <Button disabled={!target || state.status === "saving"} onClick={async () => {
          if (target && await insert({ id:crypto.randomUUID(), type:"page_link", page_id:target.id })) setDialog(null);
        }}>插入引用</Button>}
        {dialog === "knowledge" && <Button disabled={!knowledgeId} loading={busy} onClick={() => void linkKnowledge()}>关联文档</Button>}
        {dialog === "task" && <Button loading={busy} disabled={!name.trim()} onClick={() => void createPageTask()}>创建任务</Button>}
      </>}>
      <div className="workspace-page-dialog-content">
        {(managementError || state.error) && <p role="alert" className="workspace-page-error">{managementError || state.error}</p>}
        {dialog === "create" && <>
          {target && <p>父页面 · {target.title}</p>}
          <Input label="页面标题" value={name} maxLength={200} onChange={event => setName(event.target.value)} />
          <Select label="页面模板" value={template} onChange={event => setTemplate(event.target.value)}
            options={templates.map(item => ({ value:item.value,label:item.label }))} />
        </>}
        {dialog === "details" && <>
          <Input label="页面标题" value={name} maxLength={200} onChange={event => setName(event.target.value)} />
          <Input label="页面图标" value={icon} maxLength={16} onChange={event => setIcon(event.target.value)} />
          <label className="workspace-page-cover">封面图片<input type="file" accept="image/png,image/jpeg,image/gif,image/webp" onChange={event => { setCover(event.target.files?.[0]); setRemoveCover(false); }} /></label>
          {document?.cover_resource_id && <label><input type="checkbox" checked={removeCover} onChange={event => { setRemoveCover(event.target.checked); if (event.target.checked) setCover(undefined); }} />移除现有封面</label>}
        </>}
        {dialog === "task" && <>
          <p>任务将引用当前页面保存的版本。</p>
          <Input label="任务标题" value={name} maxLength={200} onChange={event => setName(event.target.value)} />
          <Textarea label="任务说明" value={taskDescription} maxLength={4000} onChange={event => setTaskDescription(event.target.value)} />
        </>}
        {(dialog === "move" || dialog === "link") && <>
          {dialog === "move" && <Button size="sm" variant="secondary" aria-pressed={!target} onClick={() => setTarget(undefined)}>根层级</Button>}
          <PagePicker workspace={workspace.id} selected={target?.id} excluded={document?.id} onSelect={setTarget} />
        </>}
        {dialog === "knowledge" && (busy && !knowledge.length ? <p role="status">正在读取知识文档…</p> :
          !knowledge.length ? <p>暂无可关联的知识文档。</p> :
          <Select label="文档" value={knowledgeId} onChange={event => setKnowledgeId(event.target.value)} options={[
            {value:"",label:"选择文档"}, ...knowledge.map(item => ({value:item.id,label:item.name})),
          ]} />)}
        {dialog === "history" && <>
          <p>恢复将保存为一个新版本。</p>
          {historyLoading && <p role="status">正在读取版本…</p>}
          {history.map(item => <div key={item.revision} className="workspace-page-history-row">
            <span>版本 {item.revision} · {sources[item.source.split(":", 1)[0]] ?? item.source} · {new Date(item.created_at).toLocaleString("zh-CN")}</span>
            <Button size="sm" variant="secondary" disabled={item.revision === document?.revision || state.status === "saving"} onClick={async () => {
              const restored = await state.mutate(page => restorePageRevision(page.id, page.revision, item.revision));
              if (restored) { setDialog(null); refresh(); setNotice("已恢复历史版本。"); }
            }}>恢复版本</Button>
          </div>)}
          {!historyEnd && !historyLoading && history.length > 0 && <Button size="sm" variant="ghost" onClick={() => void openHistory(history.length)}>加载更多版本</Button>}
        </>}
      </div>
    </Modal>
    <ConfirmDialog open={confirmTrash} title="移入回收站" description="页面和它的子页面将一起移入回收站，可以从回收站恢复。"
      confirmLabel="移入回收站" onCancel={() => setConfirmTrash(false)} onConfirm={remove}>
      {managementError && <p role="alert" className="workspace-page-error">{managementError}</p>}
    </ConfirmDialog>
    <ConfirmDialog open={Boolean(trashTarget)} title="恢复页面" description={trashTarget?.title ?? ""}
      confirmLabel="恢复页面" onCancel={() => setTrashTarget(undefined)} onConfirm={async () => {
        if (!trashTarget) return;
        const result = await restorePage(trashTarget.id, trashTarget.revision);
        if (result.ok) { setTrashTarget(undefined); refresh(); setNotice("页面已从回收站恢复。"); }
        else setManagementError(result.error);
      }}>
      {managementError && <p role="alert" className="workspace-page-error">{managementError}</p>}
    </ConfirmDialog>
    <ConfirmDialog open={discardDraft} title="读取最新版本" description="读取后将清除当前页面尚未保存的本地修改。"
      confirmLabel="读取最新版本" onCancel={() => setDiscardDraft(false)} onConfirm={async () => {
        if (pageId) await state.reload(pageId);
        setDiscardDraft(false);
      }} />
  </div>;
}
