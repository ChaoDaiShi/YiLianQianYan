import { useState, useRef, useEffect, KeyboardEvent } from "react";
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
    if (suggestedText) {
      setInput(suggestedText);
      onTextUsed?.();
      textareaRef.current?.focus();
    }
  }, [suggestedText, onTextUsed]);

  useEffect(() => {
    const el = textareaRef.current;
    if (el) {
      el.style.height = "auto";
      el.style.height = Math.min(el.scrollHeight, 200) + "px";
    }
  }, [input]);

  const handleSend = () => {
    if (!input.trim() || isLoading) return;
    onSend(input.trim());
    setInput("");
    if (textareaRef.current) textareaRef.current.style.height = "auto";
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div className="border-t border-[var(--border)] px-4 py-3 bg-[var(--panel)]/40 backdrop-blur-md">
      <div className="flex items-end gap-2 max-w-4xl mx-auto">
        <textarea
          ref={textareaRef}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="输入消息… Enter 发送，Shift+Enter 换行"
          rows={1}
          disabled={isLoading}
          className="flex-1 resize-none rounded-xl border border-[var(--border)] bg-[var(--input-bg)] px-4 py-2.5 text-sm text-[var(--text)] placeholder:text-[var(--text-faint)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 disabled:opacity-50"
        />
        {isLoading ? (
          <Button variant="danger" onClick={onStop} title="停止生成">
            <Square className="w-4 h-4" />
            停止
          </Button>
        ) : (
          <Button onClick={handleSend} disabled={!input.trim()} title="发送">
            <Send className="w-4 h-4" />
            发送
          </Button>
        )}
      </div>
    </div>
  );
}
