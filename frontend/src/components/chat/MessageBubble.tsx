import { useState } from "react";
import { Copy, Check } from "lucide-react";
import { Message } from "../../types";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import ToolCallCard, { ToolResultContent } from "./ToolCallCard";

interface MessageBubbleProps {
  message: Message;
}

export default function MessageBubble({ message }: MessageBubbleProps) {
  const isUser = message.role === "user";
  const isTool = message.role === "tool";
  const [copied, setCopied] = useState(false);

  if (isTool) {
    if (message.content?.startsWith("data:image/")) {
      return (
        <div className="mb-4 ml-4">
          <ToolResultContent result={message.content} />
        </div>
      );
    }
    return null;
  }

  const rawToolCalls = message.tool_calls;
  const toolCalls = Array.isArray(rawToolCalls) ? rawToolCalls : [];

  const copy = async () => {
    await navigator.clipboard.writeText(message.content || "");
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className="mb-4 animate-msg-in group">
      <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
        <div
          className={`relative max-w-[85%] px-4 py-3 rounded-2xl ${
            isUser
              ? "bg-[var(--accent)] text-[var(--accent-fg)] rounded-br-md"
              : "bg-[var(--panel)] border border-[var(--border)] text-[var(--text)] rounded-bl-md"
          }`}
        >
          {isUser ? (
            <p className="whitespace-pre-wrap text-sm leading-relaxed">{message.content}</p>
          ) : (
            <div className="prose prose-sm max-w-none overflow-x-auto">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown>
            </div>
          )}
          {!isUser && message.content && (
            <button
              onClick={copy}
              className="absolute -bottom-3 right-2 opacity-0 group-hover:opacity-100 transition-opacity p-1 rounded bg-[var(--panel-2)] border border-[var(--border)] text-[var(--text-muted)] hover:text-[var(--text)]"
              title="复制"
            >
              {copied ? <Check className="w-3.5 h-3.5 text-[var(--success)]" /> : <Copy className="w-3.5 h-3.5" />}
            </button>
          )}
        </div>
      </div>

      {toolCalls.map((tc: any) => {
        const id = tc.toolCallId || tc.id || "";
        const name = tc.name || tc.function?.name || "";
        const args = tc.args || tc.function?.arguments || {};
        const status = tc.status || "success";
        const result = tc.result;
        return (
          <ToolCallCard
            key={id}
            toolCallId={id}
            name={name}
            args={typeof args === "string" ? {} : args}
            status={status}
            result={result}
          />
        );
      })}
    </div>
  );
}
