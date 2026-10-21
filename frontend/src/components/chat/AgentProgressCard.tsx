import type { ToolCallRecord } from "../../types";
import ToolCallCard from "./ToolCallCard";
import { formatToolActivity } from "./toolDisplay";

interface AgentProgressCardProps {
  toolCalls: ToolCallRecord[];
}

export default function AgentProgressCard({
  toolCalls,
}: AgentProgressCardProps) {
  if (toolCalls.length === 0) return null;
  const pending = toolCalls.find((call) => call.approvalStatus === "pending" || call.status === "blocked");
  const running = [...toolCalls].reverse().find((call) => call.status === "running");
  const label = pending ? "等待你的确认" : running ? formatToolActivity(running.name, running.args) : "执行记录";

  return (
    <details
      className="agent-progress-card mb-3 border-l border-[var(--border-soft)] pl-3 text-xs text-[var(--text-secondary)]"
      aria-label="Agent 执行进度"
    >
      <summary className="agent-progress-header flex cursor-pointer list-none items-center gap-2 py-2">
        <span className={"agent-progress-dot h-1.5 w-1.5 rounded-full bg-[var(--accent-primary)] " + (pending || running ? "animate-pulse" : "")} />
        <span className="truncate text-xs text-[var(--text-secondary)]">{label}</span>
        <span className="agent-progress-count ml-auto text-[10px] text-[var(--text-faint)]">
          {toolCalls.length} 个动作
        </span>
        <span aria-hidden="true">⌄</span>
      </summary>
      <div>
        {toolCalls.map((toolCall) => (
          <ToolCallCard
            key={toolCall.toolCallId}
            {...toolCall}
            compact
          />
        ))}
      </div>
    </details>
  );
}
