import type { Message, ToolCallRecord } from "../../types";
import { EmptyState } from "../ui";
import MessageBubble from "./MessageBubble";
import StreamingText from "./StreamingText";
import ToolCallCard from "./ToolCallCard";

export interface StreamingState {
  content: string;
  toolCalls: ToolCallRecord[];
}

interface MessageListProps {
  messages: Message[];
  streaming: StreamingState | null;
  messagesEndRef: React.RefObject<HTMLDivElement>;
  onHint?: (text: string) => void;
  error?: string | null;
}

const HINTS = [
  "列出当前目录的文件",
  "创建一个 Python 脚本",
  "搜索包含 TODO 的文件",
  "查看系统进程",
];

export default function MessageList({
  messages,
  streaming,
  messagesEndRef,
  onHint,
  error,
}: MessageListProps) {
  return (
    <div className="scrollbar-thin flex-1 overflow-y-auto px-4 py-6">
      <div className="message-column">
      {messages.length === 0 && !streaming && (
        <EmptyState
          icon={
            <img
              src="/favicon.png"
              alt=""
              className="h-16 w-16 rounded-2xl object-cover shadow-lg"
            />
          }
          title="忆涟千言"
          description="描述你的目标，我会规划步骤、调用本地工具，并反馈执行与验证结果。"
          action={
            <div className="grid max-w-md grid-cols-2 gap-2 text-xs">
              {HINTS.map((hint) => (
                <button
                  type="button"
                  key={hint}
                  className="rounded-lg border border-[var(--border)] bg-[var(--panel)] px-3 py-2.5 text-left text-[var(--text-muted)] transition-colors hover:border-[var(--accent)]/50 hover:text-[var(--accent)]"
                  onClick={() => onHint?.(hint)}
                >
                  {hint}
                </button>
              ))}
            </div>
          }
          className="h-full"
        />
      )}

      {messages.map((message) => (
        <MessageBubble key={message.id} message={message} />
      ))}

      {streaming && (
        <div className="mb-4 animate-msg-in">
          {streaming.toolCalls.map((toolCall) => (
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

          {streaming.content && (
            <div className="rounded-2xl border border-[var(--border)] bg-[var(--panel)] p-4 text-[var(--text)]">
              <StreamingText text={streaming.content} />
            </div>
          )}

          {!streaming.content && streaming.toolCalls.length === 0 && (
            <div className="flex items-center gap-2 p-4" aria-label="正在生成">
              <div className="flex gap-1">
                {[0, 0.2, 0.4].map((delay) => (
                  <span
                    key={delay}
                    className="h-2 w-2 animate-pulse rounded-full bg-[var(--accent)]"
                    style={{ animationDelay: `${delay}s` }}
                  />
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {error && (
        <div className="mb-4 rounded-xl border border-[var(--danger)]/40 bg-[var(--danger)]/10 px-4 py-3 text-sm text-[var(--danger)]">
          {error}
        </div>
      )}

        <div ref={messagesEndRef} />
      </div>
    </div>
  );
}
