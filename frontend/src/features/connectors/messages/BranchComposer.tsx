import { useState } from "react";
import { GitBranch, X } from "lucide-react";
import ChatInput from "../../../components/chat/ChatInput";
import type { ChatRequestOptions, WorkspaceMode } from "../../../components/chat/chatRequestOptions";
import type { ConnectorMessage } from "../../../api/connectorMessages";

export default function BranchComposer({ parent, onSend, onStop, onCancel, busy, disabled, conversationTitle, conversationId, emptyHint, contextLabel }: { parent?: ConnectorMessage; onSend: (text: string, resources?: string[], options?: ChatRequestOptions) => boolean | Promise<boolean>; onStop: () => void; onCancel?: () => void; busy: boolean; disabled?: boolean; conversationTitle?: string; conversationId?: string | null; emptyHint?: string; contextLabel?: string }) {
  const [mode, setMode] = useState<WorkspaceMode>("chat");
  return <div className="message-branch-composer branch-composer-card" aria-label={contextLabel ? "当前路径续聊" : parent ? "创建分支对话" : "画布新提问"}>
    <div className="message-branch-composer-context">{parent && <GitBranch size={14}/>}<span>{contextLabel || (parent ? `从${conversationTitle || "当前对话"}的回答继续` : emptyHint || "开始一条对话")}</span>{onCancel && parent && <button type="button" onClick={onCancel} disabled={busy} aria-label="取消创建分支"><X size={15}/></button>}</div>
    <ChatInput key={conversationId ?? "draft"} conversationId={conversationId} onSend={onSend} onStop={onStop} isLoading={busy} disabled={disabled} workspaceMode={mode} onWorkspaceModeChange={async value => setMode(value)}/>
  </div>;
}
