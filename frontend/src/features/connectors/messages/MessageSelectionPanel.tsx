import { GitBranch, MessageSquare, X } from "lucide-react";
import { Link } from "react-router-dom";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useState } from "react";
import type { ConnectorMessage } from "../../../api/connectorMessages";
import { Button } from "../../../components/ui";
import { ResourceCards } from "../../resources/ResourceCard";

export default function MessageSelectionPanel({ message, title, origin, busy, onClose, onBranch }: {
  message: ConnectorMessage;
  title?: string;
  origin?: string;
  busy: boolean;
  onClose: () => void;
  onBranch: () => void;
}) {
  const assistant = message.role === "assistant";
  const pending = message.id.startsWith("pending:") || message.id.startsWith("pending-");
  const [reasoningOpen, setReasoningOpen] = useState(pending);
  return <aside className="message-selection-panel" aria-label="选中消息详情">
    <header><div className="message-selection-heading"><span className="message-selection-emblem"><MessageSquare size={17}/></span><div><h2>{assistant ? "小涟的回答" : message.role === "user" ? "我的提问" : "执行记录"}</h2><p>{origin ? `${origin} · ` : ""}{title || "当前对话"}</p></div></div><Button variant="ghost" iconOnly size="sm" aria-label="关闭消息详情" onClick={onClose}><X size={17}/></Button></header>
    <div className="message-selection-body scrollbar-thin">
      {!!message.attachments?.length && <ResourceCards files={message.attachments} />}
      {assistant && message.reasoning && <details className="message-selection-reasoning" open={reasoningOpen} onToggle={event => setReasoningOpen(event.currentTarget.open)}><summary>模型思考</summary><div><ReactMarkdown remarkPlugins={[remarkGfm]}>{message.reasoning}</ReactMarkdown></div></details>}
      {message.content && <div className="message-selection-content prose prose-sm max-w-none"><ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown></div>}
      {pending && !message.content && !message.reasoning && <p className="message-selection-waiting" role="status">正在等待模型输出…</p>}
      {message.error && <p className="message-selection-error" role="alert">{message.error}</p>}
    </div>
    <footer>{assistant && !pending && <Button variant="secondary" size="sm" disabled={busy} onClick={onBranch}><GitBranch size={14}/>从这条回答新建分支</Button>}{!pending && <Link to={`/chat/${message.conversation_id}`}>打开完整对话</Link>}</footer>
  </aside>;
}
