import type { AgentEvent } from "../../api/client";
import type { PendingApproval } from "../../types/approval";
import type { ExecutionRecord } from "./model";

export type PersistedApprovalReconciliation =
  | { kind: "pending"; approval: PendingApproval }
  | { kind: "resolved"; event: AgentEvent }
  | { kind: "ignore" };

export function reconcilePersistedApproval(
  record: ExecutionRecord,
  approval: PendingApproval | null
): PersistedApprovalReconciliation {
  if (record.approvalStatus !== "pending" || !record.approvalId) {
    return { kind: "ignore" };
  }

  if (!approval) {
    return {
      kind: "resolved",
      event: {
        type: "approval_resolved",
        conversation_id: record.conversationId,
        tool_call_id: record.toolCallId,
        approval_id: record.approvalId,
        status: "cancelled",
        reason: "审批记录已失效，原始操作未执行。",
      },
    };
  }

  if (approval.approval_id !== record.approvalId) {
    return { kind: "ignore" };
  }

  if (approval.status === "pending") {
    return { kind: "pending", approval };
  }

  return {
    kind: "resolved",
    event: {
      type: "approval_resolved",
      conversation_id: record.conversationId,
      tool_call_id: record.toolCallId,
      approval_id: approval.approval_id,
      status: approval.status,
    },
  };
}
