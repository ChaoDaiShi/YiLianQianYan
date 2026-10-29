import { memo, useState } from "react";
import { Check, Copy, GitBranch, Trash2, Undo2, Volume2 } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Message } from "../../types";
import ToolCallCard, { ToolResultContent } from "./ToolCallCard";
import { isExternalHttpUrl, openExternalUrl } from "./externalLink";
import MessageProcess from "./MessageProcess";
import { ResourceCards } from "../../features/resources/ResourceCard";

export interface MessageBubbleProps {
  message: Message;
  /** TTS stays a deliberate user gesture and is supplied by GlobalVoiceHost. */
  onSpeak?: (text: string) => void;
  onRollback?: (message: Message) => void;
  onDelete?: (message: Message) => void;
  onBranch?: (message: Message) => void;
  actionsDisabled?: boolean;
}

export function areMessageBubblePropsEqual(
  previous: MessageBubbleProps,
  next: MessageBubbleProps,
) {
  return (
    previous.message === next.message &&
    previous.onSpeak === next.onSpeak &&
    previous.onRollback === next.onRollback &&
    previous.onDelete === next.onDelete &&
    previous.onBranch === next.onBranch &&
    previous.actionsDisabled === next.actionsDisabled
  );
}

function MessageBubble({ message, onSpeak, onRollback, onDelete, onBranch, actionsDisabled = false }: MessageBubbleProps) {
  const isUser = message.role === "user";
  const isTool = message.role === "tool";
  const [copyStatus, setCopyStatus] = useState<"idle" | "success" | "error">("idle");

  if (isTool) {
    return message.content?.startsWith("data:image/") ? (
      <div className="mb-4 max-w-[820px]">
        <ToolResultContent result={message.content} />
      </div>
    ) : null;
  }

  const toolCalls = Array.isArray(message.tool_calls) ? message.tool_calls : [];

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(message.content || "");
      setCopyStatus("success");
      window.setTimeout(() => setCopyStatus("idle"), 1500);
    } catch {
      setCopyStatus("error");
      window.setTimeout(() => setCopyStatus("idle"), 2000);
    }
  };

  const renderMarkdownLink = ({
    href,
    children,
    ...props
  }: React.AnchorHTMLAttributes<HTMLAnchorElement>) => {
    const external = Boolean(href && isExternalHttpUrl(href));
    return (
      <a
        {...props}
        href={href}
        target={external ? "_blank" : undefined}
        rel={external ? "noreferrer" : undefined}
        onClick={
          external
            ? (event) => {
                event.preventDefault();
                void openExternalUrl(href!);
              }
            : undefined
        }
      >
        {children}
      </a>
    );
  };

  return (
    <article data-message-id={message.id} tabIndex={-1} className="conversation-message group mb-5 animate-msg-in">
      <div
        className={
          "conversation-message-row flex items-start gap-2.5 " +
          (isUser ? "justify-end" : "justify-start")
        }
      >
        <div className={"conversation-message-stack " + (isUser ? "conversation-message-stack-user" : "conversation-message-stack-assistant")}>
          {isUser&&message.attachments?.length?<div className="conversation-message-attachments"><ResourceCards files={message.attachments}/></div>:null}
        {!(isUser && !message.content && message.attachments?.length) && <div
          className={
            "conversation-message-content relative min-w-0 text-sm leading-7 " +
            (isUser
              ? "conversation-message-user max-w-[720px] rounded-2xl rounded-br-md border border-[var(--accent-border)] bg-[var(--accent-soft)] px-4 py-3 text-[var(--text-primary)] shadow-sm"
              : "conversation-message-assistant max-w-[820px] rounded-2xl rounded-tl-md border border-[var(--border-soft)] bg-[var(--surface-solid)] px-4 py-3 text-[var(--text-primary)] shadow-card-token")
          }
        >
          {!isUser && <MessageProcess process={message.process}/>}
          {!isUser&&message.attachments?.length ? <ResourceCards files={message.attachments}/> : null}
          {isUser ? (
            message.content && <p className="whitespace-pre-wrap break-words">{message.content}</p>
          ) : (
            <div className="conversation-markdown prose prose-sm max-w-none">
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                components={{
                  a: renderMarkdownLink,
                  table: ({ children, ...props }) => (
                    <div className="table-scroll-container my-3 max-w-full overflow-x-auto">
                      <table className="min-w-full divide-y border border-[var(--border-soft)]" {...props}>
                        {children}
                      </table>
                    </div>
                  ),
                  pre: ({ children, ...props }) => (
                    <pre className="code-scroll-container my-3 max-w-full overflow-x-auto rounded-lg border border-[var(--border-soft)] bg-[var(--surface-muted)] p-3 text-xs" {...props}>
                      {children}
                    </pre>
                  ),
                }}
              >
                {message.content}
              </ReactMarkdown>
            </div>
          )}
          {!isUser && message.content && (
            <div className="conversation-message-actions mt-3 flex flex-wrap items-center gap-1" role="group" aria-label="回答操作">
              {onBranch && <button type="button" onClick={()=>onBranch(message)} disabled={actionsDisabled}
                className="inline-flex items-center gap-1 rounded-md border border-[var(--border-soft)] bg-[var(--surface-solid)] px-2 py-1.5 text-xs text-[var(--accent-soft-fg)] disabled:opacity-50"
                title="保留当前路径，从这条回答新建分支" aria-label="从这条回答新建分支"><GitBranch className="h-3.5 w-3.5"/>新建分支</button>}
              <button
                type="button"
                onClick={() => onSpeak?.(message.content)}
                disabled={!onSpeak}
                className="rounded-md border border-[var(--border-soft)] bg-[var(--surface-solid)] p-1.5 text-[var(--text-muted)] hover:text-[var(--text-primary)] disabled:cursor-not-allowed disabled:opacity-50"
                title="朗读回答"
                aria-label="朗读回答"
              >
                <Volume2 className="h-3.5 w-3.5" />
              </button>
              <button
                type="button"
                onClick={() => void copy()}
                className="flex items-center gap-1 rounded-md border border-[var(--border-soft)] bg-[var(--surface-solid)] p-1.5 text-[var(--text-muted)] hover:text-[var(--text-primary)]"
                title={copyStatus === "success" ? "已复制" : copyStatus === "error" ? "复制失败" : "复制回答"}
                aria-label={copyStatus === "success" ? "已复制" : copyStatus === "error" ? "复制失败" : "复制回答"}
              >
                {copyStatus === "success" ? (
                  <>
                    <Check className="h-3.5 w-3.5 text-[var(--success)]" />
                    <span className="text-[10px] font-medium text-[var(--success)]">已复制</span>
                  </>
                ) : copyStatus === "error" ? (
                  <>
                    <Copy className="h-3.5 w-3.5 text-[var(--danger)]" />
                    <span className="text-[10px] font-medium text-[var(--danger)]">复制失败</span>
                  </>
                ) : (
                  <Copy className="h-3.5 w-3.5" />
                )}
              </button>
              {onDelete && <button
                type="button"
                onClick={() => onDelete(message)}
                disabled={actionsDisabled}
                className="rounded-md border border-[var(--border-soft)] bg-[var(--surface-solid)] p-1.5 text-[var(--text-muted)] hover:text-[var(--danger)] disabled:cursor-not-allowed disabled:opacity-50"
                title="删除这条回答"
                aria-label="删除这条回答"
              ><Trash2 className="h-3.5 w-3.5" /></button>}
            </div>
          )}
        </div>}
        </div>
      </div>

      {isUser && (onRollback || onDelete) && <div className="conversation-message-actions mt-2 flex justify-end gap-1" role="group" aria-label="用户消息操作">
        {onRollback && <button
          type="button"
          onClick={() => onRollback(message)}
          disabled={actionsDisabled}
          className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)] disabled:cursor-not-allowed disabled:opacity-50"
          title="回退到这条消息之前，并恢复到输入框"
          aria-label="回退这条消息"
        ><Undo2 className="h-3.5 w-3.5" /><span>回退</span></button>}
        {onDelete && <button
          type="button"
          onClick={() => onDelete(message)}
          disabled={actionsDisabled}
          className="rounded-md p-1.5 text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--danger)] disabled:cursor-not-allowed disabled:opacity-50"
          title="只删除这条消息"
          aria-label="删除这条消息"
        ><Trash2 className="h-3.5 w-3.5" /></button>}
      </div>}

      {toolCalls.length > 0 && (
        <div className="conversation-tool-calls mt-3 max-w-[900px]">
          {toolCalls.map((toolCall) => (
            <ToolCallCard key={toolCall.toolCallId} {...toolCall} compact />
          ))}
        </div>
      )}
    </article>
  );
}

export default memo(MessageBubble, areMessageBubblePropsEqual);
