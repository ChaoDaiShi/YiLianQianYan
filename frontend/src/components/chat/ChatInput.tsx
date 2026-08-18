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
        <div className="mt-2 flex items-center justify-between gap-2">
          <p className="text-[10px] text-[var(--text-faint)]">
            Enter 发送 · Shift + Enter 换行
          </p>
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
