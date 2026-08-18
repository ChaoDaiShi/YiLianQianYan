import { memo, useState } from "react";
import { Check, Copy } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Message } from "../../types";
import ToolCallCard, { ToolResultContent } from "./ToolCallCard";

export interface MessageBubbleProps {
  message: Message;
  showAssistantAvatar?: boolean;
}

export function areMessageBubblePropsEqual(
  previous: MessageBubbleProps,
  next: MessageBubbleProps,
) {
  return (
    previous.message === next.message &&
    previous.showAssistantAvatar === next.showAssistantAvatar
  );
}

function MessageBubble({
  message,
  showAssistantAvatar = true,
}: MessageBubbleProps) {
  const isUser = message.role === "user";
  const isTool = message.role === "tool";
  const [copied, setCopied] = useState(false);

  if (isTool) {
    return message.content?.startsWith("data:image/") ? (
      <div className="mb-4 ml-11 max-w-[820px]">
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
      <div className={"flex items-start gap-3 " + (isUser ? "justify-end" : "justify-start")}>
        {!isUser && (
          <div className="flex h-8 w-8 shrink-0 items-center justify-center">
            {showAssistantAvatar ? (
              <img
                src="/favicon.png"
                alt="小昔涟"
                className="h-8 w-8 rounded-xl object-cover"
              />
            ) : (
              <span className="h-2 w-2 rounded-full bg-[var(--accent-purple)]/45" aria-hidden="true" />
            )}
          </div>
        )}

        <div
          className={
            "relative min-w-0 text-sm leading-7 " +
            (isUser
              ? "max-w-[720px] rounded-2xl rounded-br-md border border-[var(--accent-border)] bg-[var(--accent-soft)] px-4 py-3 text-[var(--text-primary)]"
              : "max-w-[820px] rounded-2xl rounded-tl-md border border-[var(--border-soft)] bg-[var(--surface-solid)] px-4 py-3 text-[var(--text-primary)]")
          }
        >
          {isUser ? (
            <p className="whitespace-pre-wrap break-words">{message.content}</p>
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
              className="absolute -bottom-2 right-1 rounded-md border border-[var(--border-soft)] bg-[var(--surface-solid)] p-1.5 text-[var(--text-muted)] opacity-0 transition-opacity hover:text-[var(--text-primary)] focus:opacity-100 group-hover:opacity-100"
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
        <div className="ml-11 mt-3 max-w-[900px]">
          {toolCalls.map((toolCall) => (
            <ToolCallCard key={toolCall.toolCallId} {...toolCall} />
          ))}
        </div>
      )}
    </article>
  );
}

export default memo(MessageBubble, areMessageBubblePropsEqual);
