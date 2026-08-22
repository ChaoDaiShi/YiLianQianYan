import type { AgentRunState } from "../../features/execution/model";

export type AgentStatus = "idle" | "running" | "approval" | "finished" | "error";

export function resolveAgentStatus({
  connection,
  hasApproval,
  isLoading,
  finished,
}: {
  connection: AgentRunState["connection"];
  hasApproval: boolean;
  isLoading: boolean;
  finished: boolean;
}): AgentStatus {
  if (hasApproval) return "approval";
  if (finished) return "finished";
  if (connection === "error") return "error";
  if (isLoading || connection === "connecting" || connection === "connected") {
    return "running";
  }
  return "idle";
}

export const AGENT_STATUS_LABELS: Record<AgentStatus, string> = {
  idle: "小昔涟正在等待你的指令",
  running: "正在为你处理任务……",
  approval: "有一步需要你的确认",
  finished: "已经处理好了",
  error: "任务没有成功完成",
};
