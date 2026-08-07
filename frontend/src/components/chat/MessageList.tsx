import { Message } from "../../types";
import type { PendingApproval } from "../../types/approval";
import { useApprovalStore } from "../../stores/approvalStore";
import { StreamingState } from "./ChatView";
import MessageBubble from "./MessageBubble";
import StreamingText from "./StreamingText";
import ToolCallCard from "./ToolCallCard";
import ApprovalCard from "../approval/ApprovalCard";
import { EmptyState } from "../ui";

interface MessageListProps {
  messages: Message[];
  streaming: StreamingState | null;
  messagesEndRef: React.RefObject<HTMLDivElement>;
  onHint?: (text: string) => void;
  error?: string | null;
  onApprove?: (approval: PendingApproval) => void;
  onReject?: (approval: PendingApproval) => void;
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
  onApprove,
  onReject,
}: MessageListProps) {
  const pendingApprovals = useApprovalStore((s) => s.pending);
  const resolving = useApprovalStore((s) => s.resolving);

  return (
    <div className="flex-1 overflow-y-auto scrollbar-thin px-4 py-6">
      {messages.length === 0 && !streaming && (
        <EmptyState
          icon={<img src="/favicon.png" alt="" className="w-16 h-16 rounded-2xl object-cover shadow-lg" />}
          title="忆涟千言"
          description="执行命令、管理文件、搜索内容、调用工具。直接告诉我你想做什么。"
          action={
            <div className="grid grid-cols-2 gap-2 text-xs max-w-md">
              {HINTS.map((hint) => (
                <button
                  key={hint}
                  className="px-3 py-2.5 rounded-lg border border-[var(--border)] bg-[var(--panel)]/60 hover:border-[var(--accent)]/50 hover:text-[var(--accent)] transition-colors text-left text-[var(--text-muted)]"
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

      {messages.map((msg) => (
        <MessageBubble key={msg.id} message={msg} />
      ))}

      {streaming && (
        <div className="mb-4 animate-msg-in">
          {Array.from(streaming.toolCalls.entries()).map(([id, toolCall]) => (
            <ToolCallCard
              key={id}
              toolCallId={id}
              name={toolCall.name}
              args={toolCall.args}
              status={toolCall.status}
              result={toolCall.result}
            />
          ))}

          {streaming.content && (
            <div className="p-4 rounded-2xl bg-[var(--panel)] border border-[var(--border)] text-[var(--text)]">
              <StreamingText text={streaming.content} />
            </div>
          )}

          {!streaming.content && streaming.toolCalls.size === 0 && (
            <div className="flex items-center gap-2 p-4">
              <div className="flex gap-1">
                <span className="w-2 h-2 bg-[var(--accent)] rounded-full animate-pulse-dot" style={{ animationDelay: "0s" }} />
                <span className="w-2 h-2 bg-[var(--accent)] rounded-full animate-pulse-dot" style={{ animationDelay: "0.2s" }} />
                <span className="w-2 h-2 bg-[var(--accent)] rounded-full animate-pulse-dot" style={{ animationDelay: "0.4s" }} />
              </div>
            </div>
          )}
        </div>
      )}

      {pendingApprovals.length > 0 && (
        <div className="mt-1 mb-2">
          {pendingApprovals.map((a) => (
            <ApprovalCard
              key={a.approval_id}
              approval={a}
              resolving={!!resolving[a.approval_id]}
              onApprove={(approval) => onApprove?.(approval)}
              onReject={(approval) => onReject?.(approval)}
            />
          ))}
        </div>
      )}

      {error && (
        <div className="mb-4 px-4 py-3 rounded-xl border border-[var(--danger)]/40 bg-red-500/10 text-[var(--danger)] text-sm">
          {error}
        </div>
      )}

      <div ref={messagesEndRef} />
    </div>
  );
}
