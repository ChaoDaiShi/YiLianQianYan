import { useState } from "react";
import { Check, Copy } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Message } from "../../types";
import ToolCallCard, { ToolResultContent } from "./ToolCallCard";

interface MessageBubbleProps {
  message: Message;
}

export default function MessageBubble({ message }: MessageBubbleProps) {
  const isUser = message.role === "user";
  const isTool = message.role === "tool";
  const [copied, setCopied] = useState(false);

  if (isTool) {
    return message.content?.startsWith("data:image/") ? (
      <div className="mb-4">
        <ToolResultContent result={message.content} />
      </div>
    ) : null;
  }

  const toolCalls = Array.isArray(message.tool_calls) ? message.tool_calls : [];

  const copy = async () => {
    await navigator.clipboard.writeText(message.content || "");
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  return (
    <article className="group mb-5 animate-msg-in">
      <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
        <div
          className={`relative max-w-[88%] px-4 py-3 text-sm leading-7 ${
            isUser
              ? "rounded-2xl rounded-br-md bg-[var(--accent)] text-[var(--accent-fg)] shadow-sm"
              : "w-full text-[var(--text)]"
          }`}
        >
          {isUser ? (
            <p className="whitespace-pre-wrap">{message.content}</p>
          ) : (
            <div className="prose prose-sm max-w-none overflow-x-auto">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>
                {message.content}
              </ReactMarkdown>
            </div>
          )}
          {!isUser && message.content && (
            <button
              type="button"
              onClick={() => void copy()}
              className="absolute -bottom-2 right-1 rounded-md border border-[var(--border)] bg-[var(--panel)] p-1.5 text-[var(--text-muted)] opacity-0 transition-opacity hover:text-[var(--text)] focus:opacity-100 group-hover:opacity-100"
              title="复制回答"
              aria-label="复制回答"
            >
              {copied ? (
                <Check className="h-3.5 w-3.5 text-[var(--success)]" />
              ) : (
                <Copy className="h-3.5 w-3.5" />
              )}
            </button>
          )}
        </div>
      </div>

      {toolCalls.length > 0 && (
        <div className="mt-3">
          {toolCalls.map((toolCall) => (
            <ToolCallCard
              key={toolCall.toolCallId}
              toolCallId={toolCall.toolCallId}
              name={toolCall.name}
              args={toolCall.args}
              status={toolCall.status}
              result={toolCall.result}
              riskLevel={toolCall.riskLevel}
              approvalStatus={toolCall.approvalStatus}
              verificationStatus={toolCall.verificationStatus}
              verificationReason={toolCall.verificationReason}
            />
          ))}
        </div>
      )}
    </article>
  );
}
