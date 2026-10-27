import {
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent,
} from "react";
import { Paperclip, Send, Square } from "lucide-react";
import { getResourcePreview, ingestResource } from "../../api/resources";
import { AttachmentQueue, type Attachment } from "../../features/resources/attachmentQueue";
import ResourceAttachments from "../../features/resources/ResourceAttachments";
import { ChatVoiceInput, type ChatVoiceControls } from "../../features/voice/ChatVoiceInput";
import { appendFinalTranscriptToComposer } from "../../features/voice/chatVoice";
import { Button } from "../ui";
import { ApprovalModeControl } from "../../features/security/ApprovalModeControl";
import { ComposerModelControls } from "./ComposerModelControls";
import { buildChatRequestOptions, type ChatRequestOptions, type ThinkingMode, type WorkspaceMode } from "./chatRequestOptions";
import { WorkspaceModeControl } from "./WorkspaceModeControl";
import "./compactComposer.css";

export interface RestoredComposerDraft {
  id: string;
  text: string;
  resources: Array<{ id: string; name: string }>;
}

interface ChatInputProps {
  onSend: (text: string, resourceIds?: string[], options?: ChatRequestOptions) => void | boolean | Promise<void | boolean>;
  conversationId?: string | null;
  isLoading: boolean;
  disabled?: boolean;
  onStop?: () => void;
  suggestedText?: string;
  onTextUsed?: () => void;
  restoredDraft?: RestoredComposerDraft | null;
  /** Injected by GlobalVoiceHost so chat capture cannot create a second runtime. */
  chatVoiceControls?: ChatVoiceControls | null;
  onRequestGlobalVoiceSession?: () => void;
  workspaceMode?: WorkspaceMode;
  onWorkspaceModeChange?: (mode: WorkspaceMode) => Promise<void>;
  modeChanging?: boolean;
  modeError?: string | null;
}

export default function ChatInput({
  onSend,
  conversationId,
  isLoading,
  disabled = false,
  onStop,
  suggestedText,
  onTextUsed,
  restoredDraft,
  chatVoiceControls = null,
  onRequestGlobalVoiceSession,
  workspaceMode = "chat",
  onWorkspaceModeChange,
  modeChanging = false,
  modeError,
}: ChatInputProps) {
  const [input, setInput] = useState("");
  const [modelId, setModelId] = useState("");
  const [thinking, setThinking] = useState<ThinkingMode>("standard");
  const [resourceNotice, setResourceNotice] = useState("");
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [sending, setSending] = useState(false);
  const sendInFlight = useRef(false);
  const [queueOwner,setQueueOwner]=useState<{conversationId:typeof conversationId;queue:AttachmentQueue}|null>(null);
  const queue=queueOwner&&queueOwner.conversationId===conversationId?queueOwner.queue:null;
  useEffect(()=>{
    const next=new AttachmentQueue({
      upload:async(file,signal)=>{
        const resource = await ingestResource(file, signal);
        if(!resource)throw new Error("资源上传失败，请移除后重试");
        return {id:resource.id};
      },
      preview:getResourcePreview,
      changed:()=>setAttachments(next.snapshot()),
    });
    setAttachments([]);setQueueOwner({conversationId,queue:next});
    return()=>next.dispose();
  },[conversationId]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const restoredId = useRef<string | null>(null);

  useEffect(() => {
    if (!queue || !restoredDraft || restoredId.current === restoredDraft.id) return;
    setInput(restoredDraft.text);
    queue.restore(restoredDraft.resources);
    restoredId.current = restoredDraft.id;
    setResourceNotice(restoredDraft.resources.length ? "已恢复当前对话绑定的附件，可调整后发送" : "");
    textareaRef.current?.focus();
  }, [queue, restoredDraft]);

  useEffect(() => {
    if (!suggestedText) return;
    setInput(suggestedText);
    onTextUsed?.();
    textareaRef.current?.focus();
  }, [onTextUsed, suggestedText]);

  useEffect(() => {
    if (!chatVoiceControls) return;
    return chatVoiceControls.subscribeFinal((text) => {
      setInput((current) => appendFinalTranscriptToComposer(current, text));
      textareaRef.current?.focus();
    });
  }, [chatVoiceControls]);

  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = "auto";
    const nextHeight = workspaceMode === "chat" ? Math.max(24, Math.min(textarea.scrollHeight, 120)) : Math.max(52, Math.min(textarea.scrollHeight, 150));
    textarea.style.height = `${nextHeight}px`;
  }, [input, workspaceMode]);

  const pendingAttachments = attachments.some(item => item.state !== "ready" && item.state !== "removed");
  const canSend = Boolean(queue && (input.trim() || queue.readyIds().length)) && !pendingAttachments && !isLoading && !disabled && !sending;
  const send = async () => {
    if (!queue || !canSend || sendInFlight.current) return;
    const text = input.trim() || "请查看我附加的文件。";
    sendInFlight.current = true; setSending(true); setResourceNotice("");
    try {
      const accepted = await onSend(text, queue.readyIds(), buildChatRequestOptions(modelId, thinking, workspaceMode));
      if (accepted === false) { setResourceNotice("发送未完成，附件已保留"); return; }
      setInput(""); queue.snapshot().forEach(item => queue.remove(item.id));
      if (textareaRef.current) textareaRef.current.style.height = "auto";
    } catch { setResourceNotice("发送或资源绑定失败，附件已保留，请重试"); }
    finally { sendInFlight.current = false; setSending(false); }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.nativeEvent.isComposing || event.keyCode === 229) {
      return;
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
    }
  };

  const addFiles = (files: File[]) => {
    if (!queue || isLoading || sending) return;
    try { queue.add(files); setResourceNotice(""); void queue.process(); }
    catch (error) { setResourceNotice(error instanceof Error ? error.message : "附件无效"); }
  };
  const handleFileSelected = (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.currentTarget.files ?? []);
    event.currentTarget.value = "";
    addFiles(files);
  };

  return (
    <div className="home-composer compact-composer w-full" data-workspace-mode={workspaceMode} onDragOver={event => event.preventDefault()} onDrop={event => { event.preventDefault(); addFiles(Array.from(event.dataTransfer.files)); }}>
      <div className="composer-card conversation-composer-inner">
        {(attachments.some(item => item.state !== "removed") || resourceNotice || modeError) && <div className="composer-feedback">
          <ResourceAttachments items={attachments} onRemove={id => queue?.remove(id)}/>
          {resourceNotice && <p className="text-caption text-[var(--text-muted)]" role="status">{resourceNotice}</p>}
          {modeError && <p className="text-caption text-[var(--danger)]" role="alert">{modeError}</p>}
        </div>}
        <div className="composer-mode-slot"><WorkspaceModeControl mode={workspaceMode} onChange={onWorkspaceModeChange} disabled={isLoading || sending || modeChanging} /></div>
        <textarea
          ref={textareaRef}
          value={input}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={handleKeyDown}
          onPaste={event => { const files = Array.from(event.clipboardData.files).filter(file => file.type.startsWith("image/")); if (files.length) { event.preventDefault(); addFiles(files); } }}
          placeholder={workspaceMode === "chat" ? "和小涟聊聊…" : workspaceMode === "work" ? "描述你希望完成的工作…" : "写下想法，开始创造…"}
          aria-label="聊天输入"
          rows={1}
          disabled={isLoading || disabled}
          title="Enter 发送 · Shift + Enter 换行"
          className="composer-textarea"
        />
        <div className="composer-actions">
          <input ref={fileInputRef} type="file" multiple className="hidden" onChange={handleFileSelected}/>
          <button type="button" className="composer-control" onClick={() => fileInputRef.current?.click()} disabled={isLoading || sending} aria-label="导入资源" title="选择、拖入或粘贴附件"><Paperclip size={16}/></button>
          <ChatVoiceInput controls={chatVoiceControls} onRequestGlobalVoiceSession={onRequestGlobalVoiceSession}/>
          <ComposerModelControls conversationId={conversationId} modelId={modelId} thinking={thinking} disabled={isLoading || sending} onModel={setModelId} onThinking={setThinking}/>
          <ApprovalModeControl disabled={isLoading || sending}/>
          {isLoading ? <Button type="button" className="composer-send" iconOnly size="sm" variant="danger" onClick={onStop} title="停止执行" aria-label="停止执行"><Square size={16}/></Button>
            : <Button type="button" className="composer-send" iconOnly size="sm" loading={sending} loadingLabel="正在提交" onClick={() => void send()} disabled={!canSend || sending} title="发送消息" aria-label="发送消息"><Send size={16}/></Button>}
        </div>

      </div>
    </div>
  );
}
