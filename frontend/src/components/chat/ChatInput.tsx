import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Send, Square } from "lucide-react";
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
  const textareaRef = useRef<HTMLTextAreaElement>(null);

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

  return (
    <div className="shrink-0 px-3 pb-3 pt-2 min-[960px]:px-5 min-[960px]:pb-5">
      <div className="message-column">
        <div className="surface-elevated flex items-end gap-2 rounded-2xl p-2.5">
          <div className="min-w-0 flex-1">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={(event) => setInput(event.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="告诉我你想完成什么…"
              rows={1}
              disabled={isLoading}
              className="block max-h-[180px] w-full resize-none bg-transparent px-2 py-1.5 text-sm leading-6 text-[var(--text)] outline-none placeholder:text-[var(--text-faint)] disabled:opacity-60"
            />
            <p className="px-2 pt-1 text-[10px] text-[var(--text-faint)]">
              Enter 发送 · Shift + Enter 换行
            </p>
          </div>
          {isLoading ? (
            <Button
              type="button"
              variant="danger"
              onClick={onStop}
              title="停止执行"
              className="mb-0.5 shrink-0"
            >
              <Square className="h-4 w-4" />
              停止
            </Button>
          ) : (
            <Button
              type="button"
              onClick={send}
              disabled={!input.trim()}
              title="发送消息"
              className="mb-0.5 shrink-0"
            >
              <Send className="h-4 w-4" />
              发送
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
