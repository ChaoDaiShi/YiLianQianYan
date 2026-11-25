import { useEffect, useState, type ReactNode } from "react";
import { FileText, GripVertical, Paperclip, Plus, Trash2 } from "lucide-react";
import { Button, Textarea } from "../../components/ui";
import ConversationMarkdown from "../../components/chat/ConversationMarkdown";
import { RichMessageParts } from "../../components/chat/RichContent";
import { loadPage, type PageBlock, type PageDocument } from "../../api/workspacePages";
import { Link } from "react-router-dom";
import type { AttachmentReference } from "../../types";

export const pageCommands = [
  { id: "paragraph", label: "段落", value: "" },
  { id: "heading", label: "标题", value: "## " },
  { id: "list", label: "列表", value: "- " },
  { id: "quote", label: "引用", value: "> " },
  { id: "code", label: "代码", value: "```\n\n```" },
  { id: "math", label: "公式", value: "$$\n\n$$" },
  { id: "divider", label: "分隔线", value: "---" },
  { id: "table", label: "表格", value: "| 列名 | 列名 |\n| --- | --- |\n|  |  |" },
  { id: "todo", label: "待办", value: "- [ ] " },
];

export default function PageEditor({ document, drafts, viewOnly, busy, toolbar, renderResource, onEdit, onInsert, onRemove, onMove, onReference, onKnowledge, onResource, onSelect, onCover }: {
  document: PageDocument;
  drafts: Record<string, string>;
  viewOnly: boolean;
  busy: boolean;
  toolbar: ReactNode;
  renderResource: (file: AttachmentReference) => ReactNode;
  onEdit: (id: string, text: string) => void;
  onInsert: (text: string) => void;
  onRemove: (id: string) => void;
  onMove: (id: string, before: string) => void;
  onReference: () => void;
  onKnowledge: () => void;
  onResource: (file: File) => void;
  onSelect: (id: string) => void;
  onCover: () => void;
}) {
  const [preview, setPreview] = useState(false);
  const [adding, setAdding] = useState(false);
  const [dropTarget, setDropTarget] = useState("");
  const editable = !viewOnly && !preview;

  return <div className="workspace-page-editor">
    <header className="workspace-page-editor-header">
      <div className="workspace-page-editor-title">
        {document.parent_page_id && <PageReference id={document.parent_page_id} onSelect={onSelect} className="workspace-page-parent" />}
        <h2>{document.icon && <span aria-hidden="true">{document.icon} </span>}{document.title}</h2>
      </div>
      <div className="workspace-page-toolbar">
        {!viewOnly && <Button size="sm" variant="ghost" aria-pressed={preview} onClick={() => setPreview(value => !value)}>{preview ? "编辑内容" : "阅读预览"}</Button>}
        {toolbar}
      </div>
    </header>
    <div className="workspace-page-content scrollbar-thin">
      {document.cover_resource_id && renderResource({ id: document.cover_resource_id, name: document.title + "封面", mime_type: "image/png" })}
      {!document.blocks.length && <p className="workspace-page-note">页面还没有内容。</p>}
      {document.blocks.map((block, index) => <div key={block.id} className="workspace-page-block" data-drop={dropTarget === block.id || undefined}
        onDragOver={event => {
          if (!editable || !event.dataTransfer.types.includes("application/yilian-block")) return;
          event.preventDefault(); setDropTarget(block.id);
        }}
        onDragLeave={() => setDropTarget("")}
        onDrop={event => {
          event.preventDefault(); setDropTarget("");
          const id = event.dataTransfer.getData("application/yilian-block");
          if (editable && id && id !== block.id) onMove(id, block.id);
        }}>
        {editable && <div className="workspace-page-block-actions">
          <button type="button" aria-label={"拖动内容块 " + (index + 1)} title="拖动调整内容顺序" draggable={!busy}
            onDragStart={event => { event.dataTransfer.setData("application/yilian-block", block.id); event.dataTransfer.effectAllowed = "move"; }}>
            <GripVertical size={15} />
          </button>
          <button type="button" title="移除内容块" aria-label={"移除内容块 " + (index + 1)} disabled={busy} onClick={() => onRemove(block.id)}><Trash2 size={14} /></button>
        </div>}
        <Block block={block} index={index} text={drafts[block.id]} editable={editable} onEdit={text => onEdit(block.id, text)} onSelect={onSelect} renderResource={renderResource} />
      </div>)}
      {editable && <div className="workspace-page-insert">
        <div className="workspace-page-toolbar">
          <Button size="sm" variant="secondary" disabled={busy} onClick={() => setAdding(value => !value)} aria-expanded={adding}><Plus size={15} />添加内容块</Button>
          <Button size="sm" variant="ghost" disabled={busy} onClick={onReference}><FileText size={15} />引用页面</Button>
          <Button size="sm" variant="ghost" disabled={busy} onClick={onKnowledge}>关联知识文档</Button>
          <label className="workspace-page-file-input"><Paperclip size={15} />添加附件
            <input type="file" disabled={busy} aria-label="添加附件" onChange={event => {
              const file = event.target.files?.[0]; if (file) onResource(file); event.target.value = "";
            }} />
          </label>
          {!document.cover_resource_id && <Button size="sm" variant="ghost" disabled={busy} onClick={onCover}>设置封面</Button>}
        </div>
        {adding && <div className="workspace-page-toolbar" aria-label="内容块类型">{pageCommands.map(command => <Button key={command.id} size="sm" variant="ghost" disabled={busy}
          onClick={() => { setAdding(false); onInsert(command.value); }}>{command.label}</Button>)}</div>}
      </div>}
    </div>
  </div>;
}

function Block({ block, index, text, editable, onEdit, onSelect, renderResource }: {
  block: PageBlock; index: number; text?: string; editable: boolean; onEdit: (text: string) => void; onSelect: (id: string) => void;
  renderResource: (file: AttachmentReference) => ReactNode;
}) {
  if (block.type === "page_link") return <PageReference id={block.page_id} onSelect={onSelect} />;
  if (block.type === "knowledge_link") {
    const location = new URLSearchParams({ document:block.document_id, version:block.version, start:"0", end:String(block.end_char) });
    return <Link className="workspace-page-link" to={"/knowledge?" + location}><FileText size={14} aria-hidden="true" />{block.name}</Link>;
  }
  if (block.type === "resource") return renderResource({ id: block.resource_id, name: block.name, mime_type: block.mime_type });
  if (block.type === "rich") return <div className="workspace-page-snapshot">
    <Link to={"/chat/" + encodeURIComponent(block.conversation_id) + "?message=" + encodeURIComponent(block.message_id)}>聊天来源</Link>
    <RichMessageParts response={block.snapshot} />
  </div>;
  const markdown = text ?? block.text;
  if (!editable) return <ConversationMarkdown text={markdown} />;
  const slash = markdown.match(/^\/([^\s/]*)$/);
  const choices = slash ? pageCommands.filter(command => command.id.startsWith(slash[1]) || command.label.includes(slash[1])) : [];
  return <div className="workspace-page-markdown">
    <Textarea aria-label={"内容块 " + (index + 1)} value={markdown} rows={Math.max(3, Math.min(11, markdown.split("\n").length + 1))}
      placeholder="输入 Markdown，或输入 / 选择内容块" onChange={event => onEdit(event.target.value)}
      onKeyDown={event => {
        if (event.key === "Enter" && choices.length === 1 && !event.nativeEvent.isComposing) {
          event.preventDefault(); onEdit(choices[0].value);
        }
      }} />
    {choices.length > 0 && <div className="workspace-page-toolbar" aria-label="斜杠命令">
      {choices.map(command => <Button key={command.id} size="sm" variant="secondary" onClick={() => onEdit(command.value)}>{command.label}</Button>)}
    </div>}
  </div>;
}

function PageReference({ id, onSelect, className = "" }: { id: string; onSelect: (id: string) => void; className?: string }) {
  const [label, setLabel] = useState("");
  const [available, setAvailable] = useState(false);
  useEffect(() => {
    let current = true;
    void loadPage(id).then(result => {
      if (!current) return;
      setAvailable(result.ok);
      setLabel(result.ok ? result.data.title : "页面暂不可访问");
    });
    return () => { current = false; };
  }, [id]);
  return <button className={"workspace-page-link " + className} type="button" disabled={!available} onClick={() => onSelect(id)}>
    <FileText size={14} aria-hidden="true" />{label || "正在读取页面…"}
  </button>;
}
