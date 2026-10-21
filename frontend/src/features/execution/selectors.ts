import type { AgentRunState, ExecutionRecord } from "./model";

function priority(record: ExecutionRecord): number {
  if (record.approvalStatus === "pending" && record.riskLevel === "critical") {
    return 0;
  }
  if (record.approvalStatus === "pending" && record.riskLevel === "high") {
    return 1;
  }
  if (record.approvalStatus === "pending") return 2;
  if (record.executionStatus === "running" || record.executionStatus === "awaiting_approval") return 3;
  if (record.executionStatus === "queued") return 4;
  return 99;
}

export function selectCurrentAction(
  records: ExecutionRecord[]
): ExecutionRecord | null {
  return (
    [...records]
      .filter((record) => priority(record) < 99)
      .sort(
        (a, b) => priority(a) - priority(b) || b.sequence - a.sequence
      )[0] || null
  );
}

export function selectExecutionHistory(
  state: AgentRunState
): ExecutionRecord[] {
  return state.order
    .map((id) => state.records[id])
    .filter(Boolean)
    .sort((a, b) => b.sequence - a.sequence);
}
