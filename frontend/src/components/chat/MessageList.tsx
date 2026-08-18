import type { Message, ToolCallRecord } from "../../types";
import { MESSAGE_LIST_VIEWPORT_CLASS_NAME } from "../layout/workspaceLayout";
import AgentProgressCard from "./AgentProgressCard";
import MessageBubble from "./MessageBubble";
import StreamingText from "./StreamingText";

export interface StreamingState {
  content: string;
  toolCalls: ToolCallRecord[];
}

interface MessageListProps {
  messages: Message[];
  streaming: StreamingState | null;
  scrollContainerRef: React.RefObject<HTMLDivElement>;
  onScroll?: React.UIEventHandler<HTMLDivElement>;
  error?: string | null;
  onRetry?: () => void;
}

function ErrorNotice({ error, onRetry }: { error: string; onRetry?: () => void }) {
  return (
    <section
      className="conversation-error-notice mb-4 rounded-[var(--radius-lg)] border border-[var(--danger-border)] bg-[var(--danger-soft)] px-4 py-3"
      aria-label="任务执行错误"
    >
      <div className="flex items-start gap-3">
        <span className="mt-1 h-2.5 w-2.5 shrink-0 rounded-full bg-[var(--danger)]" />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-[var(--danger-fg)]">
            任务没有成功完成
          </p>
          <p className="mt-1 text-xs leading-5 text-[var(--text-secondary)]">
            执行过程中遇到了问题，任务已经停止。
          </p>
          <div className="mt-3 flex flex-wrap items-center gap-3">
            {onRetry && (
              <button
                type="button"
                onClick={onRetry}
                className="rounded-[var(--radius-sm)] bg-[var(--danger)] px-3 py-1.5 text-xs font-medium text-[var(--danger-fg)] transition-colors hover:opacity-90"
              >
                重新尝试
              </button>
            )}
            <details className="text-xs text-[var(--text-secondary)]">
              <summary className="cursor-pointer select-none hover:text-[var(--text-primary)]">
                查看技术详情
              </summary>
              <pre className="mt-2 max-h-40 max-w-full overflow-auto whitespace-pre-wrap rounded-lg border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2 font-mono text-[var(--text-secondary)]">
                {error}
              </pre>
            </details>
          </div>
        </div>
      </div>
    </section>
  );
}

export default function MessageList({
  messages,
  streaming,
  scrollContainerRef,
  onScroll,
  error,
  onRetry,
}: MessageListProps) {
  return (
    <div
      ref={scrollContainerRef}
      onScroll={onScroll}
      className={`${MESSAGE_LIST_VIEWPORT_CLASS_NAME} conversation-message-list`}
    >
      <div className="message-column">
        {messages.map((message, index) => (
          <MessageBubble
            key={message.id}
            message={message}
            showAssistantAvatar={
              message.role === "assistant" &&
              messages[index - 1]?.role !== "assistant"
            }
          />
        ))}

        {streaming && (
          <div className="conversation-streaming-state mb-4 animate-msg-in">
            <AgentProgressCard toolCalls={streaming.toolCalls} />

            {streaming.content && (
              <div className="conversation-streaming-message conversation-message-assistant min-w-0 max-w-[820px] rounded-2xl rounded-tl-md border border-[var(--border-soft)] bg-[var(--surface-solid)] p-4 text-[var(--text-primary)]">
                <StreamingText text={streaming.content} />
              </div>
            )}

            {!streaming.content && streaming.toolCalls.length === 0 && (
              <div className="conversation-streaming-wait flex items-center gap-3 px-1 py-3" aria-label="正在生成">
                <span className="text-xs text-[var(--text-secondary)]">
                  正在接收小昔涟的回复…
                </span>
                <span className="h-2 w-2 animate-pulse rounded-full bg-[var(--accent-primary)]" />
              </div>
            )}
          </div>
        )}

        {error && <ErrorNotice error={error} onRetry={onRetry} />}
      </div>
    </div>
  );
}
