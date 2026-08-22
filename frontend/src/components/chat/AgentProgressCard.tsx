import type { ToolCallRecord } from "../../types";
import ToolCallCard from "./ToolCallCard";

interface AgentProgressCardProps {
  toolCalls: ToolCallRecord[];
}

export default function AgentProgressCard({
  toolCalls,
}: AgentProgressCardProps) {
  if (toolCalls.length === 0) return null;

  return (
    <section
      className="agent-progress-card mb-4 rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface-muted)] p-3"
      aria-label="Agent 执行进度"
    >
      <div className="agent-progress-header mb-2 flex items-center gap-2 px-1">
        <span className="agent-progress-dot h-2 w-2 animate-pulse rounded-full bg-[var(--accent-blue)]" />
        <h3 className="text-xs font-semibold text-[var(--text-primary)]">
          正在执行任务
        </h3>
        <span className="agent-progress-count ml-auto text-[10px] text-[var(--text-faint)]">
          {toolCalls.length} 个动作
        </span>
      </div>
      <div>
        {toolCalls.map((toolCall) => (
          <ToolCallCard
            key={toolCall.toolCallId}
            {...toolCall}
            compact
          />
        ))}
      </div>
    </section>
  );
}
