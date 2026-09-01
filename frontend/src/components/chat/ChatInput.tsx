import {
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent,
} from "react";
import { Paperclip, Send, Square } from "lucide-react";
import { ingestResource } from "../../api/resources";
import { Button } from "../ui";

interface ChatInputProps {
  onSend: (text: string) => void;
  isLoading: boolean;
  onStop?: () => void;
  suggestedText?: string;
  onTextUsed?: () => void;
}

export default function ChatInput({
  onSend,
  isLoading,
  onStop,
  suggestedText,
  onTextUsed,
}: ChatInputProps) {
  const [input, setInput] = useState("");
  const [resourceNotice, setResourceNotice] = useState("");
  const [isUploading, setIsUploading] = useState(false);
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

  const send = () => {
    const text = input.trim();
    if (!text || isLoading) return;
    onSend(text);
    setInput("");
    if (textareaRef.current) textareaRef.current.style.height = "auto";
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      send();
    }
  };

  const handleFileSelected = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = "";
    if (!file) return;
    setIsUploading(true);
    setResourceNotice(`正在导入 ${file.name}…`);
    try {
      const resource = await ingestResource(file);
      setResourceNotice(
        resource
          ? `已导入 ${resource.name} · Resource ID: ${resource.id}`
          : `导入 ${file.name} 失败`,
      );
    } catch {
      setResourceNotice(`导入 ${file.name} 失败`);
    } finally {
      setIsUploading(false);
    }
  };

  return (
    <div className="home-composer w-full">
      <div className="composer-card mx-auto rounded-2xl p-3 [width:min(680px,calc(100%-48px))]">
        <textarea
          ref={textareaRef}
          value={input}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="告诉小昔涟你想完成什么……"
          rows={1}
          disabled={isLoading}
          className="block min-h-[52px] w-full resize-none bg-transparent px-2 py-1.5 text-sm leading-6 text-[var(--text)] outline-none placeholder:text-[var(--text-faint)] disabled:opacity-60"
        />
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
              className="hidden"
              onChange={(event) => void handleFileSelected(event)}
            />
            <button
              type="button"
              className="rounded-md p-1.5 text-[var(--text-muted)] transition-colors hover:bg-[var(--panel-hover)] hover:text-[var(--text)] disabled:opacity-50"
              onClick={() => fileInputRef.current?.click()}
              disabled={isUploading}
              aria-label="导入资源"
              title="导入资源（最大 25 MiB）"
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
              onClick={send}
              disabled={!input.trim()}
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
