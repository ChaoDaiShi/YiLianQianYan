import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent,
} from "react";
import { Paperclip, Send, Square } from "lucide-react";
import { getResourcePreview, ingestResource } from "../../api/resources";
import { AttachmentQueue, type Attachment } from "../../features/resources/attachmentQueue";
import ResourceAttachments from "../../features/resources/ResourceAttachments";
import { Button } from "../ui";

interface ChatInputProps {
  onSend: (text: string, resourceIds?: string[]) => void | boolean | Promise<void | boolean>;
  conversationId?: string | null;
  isLoading: boolean;
  onStop?: () => void;
  suggestedText?: string;
  onTextUsed?: () => void;
}

export default function ChatInput({
  onSend,
  conversationId,
  isLoading,
  onStop,
  suggestedText,
  onTextUsed,
}: ChatInputProps) {
  const [input, setInput] = useState("");
  const [resourceNotice, setResourceNotice] = useState("");
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [sending, setSending] = useState(false);
  const sendInFlight = useRef(false);
  const queue = useMemo(() => {
    const next = new AttachmentQueue({
      upload: async (file, signal) => {
        const resource = await ingestResource(file, signal);
        if (!resource) throw new Error("资源上传失败，请移除后重试");
        return { id: resource.id };
      },
      preview: getResourcePreview,
      changed: () => setAttachments(next.snapshot()),
    });
    return next;
  }, [conversationId]);
  useEffect(() => { setAttachments(queue.snapshot()); return () => queue.dispose(); }, [queue]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!suggestedText) return;
    setInput(suggestedText);
    onTextUsed?.();
    textareaRef.current?.focus();
  }, [onTextUsed, suggestedText]);

  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = "auto";
    textarea.style.height = `${Math.min(textarea.scrollHeight, 180)}px`;
  }, [input]);

  const pendingAttachments = attachments.some(item => item.state !== "ready" && item.state !== "removed");
  const canSend = Boolean(input.trim() || queue.readyIds().length) && !pendingAttachments && !isLoading && !sending;
  const send = async () => {
    if (!canSend || sendInFlight.current) return;
    const text = input.trim() || "请查看我附加的文件。";
    sendInFlight.current = true; setSending(true); setResourceNotice("");
    try {
      const accepted = await onSend(text, queue.readyIds());
      if (accepted === false) { setResourceNotice("发送未完成，附件已保留"); return; }
      setInput(""); queue.snapshot().forEach(item => queue.remove(item.id));
      if (textareaRef.current) textareaRef.current.style.height = "auto";
    } catch { setResourceNotice("发送或资源绑定失败，附件已保留，请重试"); }
    finally { sendInFlight.current = false; setSending(false); }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
    }
  };

  const addFiles = (files: File[]) => {
    if (isLoading || sending) return;
    try { queue.add(files); setResourceNotice(""); void queue.process(); }
    catch (error) { setResourceNotice(error instanceof Error ? error.message : "附件无效"); }
  };
  const handleFileSelected = (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.currentTarget.files ?? []);
    event.currentTarget.value = "";
    addFiles(files);
  };

  return (
    <div className="home-composer w-full" onDragOver={event => event.preventDefault()} onDrop={event => { event.preventDefault(); addFiles(Array.from(event.dataTransfer.files)); }}>
      <div className="composer-card mx-auto rounded-2xl p-3 [width:min(680px,calc(100%-48px))]">
        <textarea
          ref={textareaRef}
          value={input}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={handleKeyDown}
          onPaste={event => { const files = Array.from(event.clipboardData.files).filter(file => file.type.startsWith("image/")); if (files.length) { event.preventDefault(); addFiles(files); } }}
          placeholder="告诉小昔涟你想完成什么……"
          rows={1}
          disabled={isLoading}
          className="block min-h-[52px] w-full resize-none bg-transparent px-2 py-1.5 text-sm leading-6 text-[var(--text)] outline-none placeholder:text-[var(--text-faint)] disabled:opacity-60"
        />
        <ResourceAttachments items={attachments} onRemove={id => queue.remove(id)} />
        {resourceNotice && (
          <p className="px-2 pb-1 text-[10px] text-[var(--text-muted)]" role="status">
            {resourceNotice}
          </p>
        )}
        <div className="composer-footer mt-2 flex items-center justify-between gap-2">
          <div className="flex min-w-0 items-center gap-2">
            <input
              ref={fileInputRef}
              type="file"
              multiple
              className="hidden"
              onChange={(event) => void handleFileSelected(event)}
            />
            <button
              type="button"
              className="rounded-md p-1.5 text-[var(--text-muted)] transition-colors hover:bg-[var(--panel-hover)] hover:text-[var(--text)] disabled:opacity-50"
              onClick={() => fileInputRef.current?.click()}
              disabled={isLoading || sending}
              aria-label="导入资源"
              title="选择、拖入或粘贴文件（最多 8 个，每个 25 MiB）"
            >
              <Paperclip className="h-3.5 w-3.5" />
            </button>
            <p className="truncate text-[10px] text-[var(--text-faint)]">
              Enter 发送 · Shift + Enter 换行
            </p>
          </div>
          {isLoading ? (
            <Button
              type="button"
              variant="danger"
              size="sm"
              onClick={onStop}
              title="停止执行"
            >
              <Square className="h-3.5 w-3.5" />
              停止
            </Button>
          ) : (
            <Button
              type="button"
              size="sm"
              onClick={() => void send()}
              disabled={!canSend}
              title="发送消息"
            >
              <Send className="h-3.5 w-3.5" />
              发送
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
