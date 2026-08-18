import { describe, expect, it } from "vitest";
import type { ExecutionRecord } from "./model";
import type { PendingApproval } from "../../types/approval";
import { reconcilePersistedApproval } from "./approvalReconciliation";

const record: ExecutionRecord = {
  toolCallId: "call-1",
  conversationId: "conv-1",
  approvalId: "approval-1",
  name: "process",
  args: { command: "cargo run" },
  riskLevel: "high",
  executionStatus: "awaiting_approval",
  approvalStatus: "pending",
  verificationStatus: "not_requested",
  sequence: 1,
};

const approval = (status: PendingApproval["status"]): PendingApproval => ({
  approval_id: "approval-1",
  conversation_id: "conv-1",
  tool_call_id: "call-1",
  tool_name: "process",
  arguments: { command: "cargo run" },
  risk_level: "high",
  reason: "starts a process",
  status,
  created_at: "2026-08-18T00:00:00.000Z",
  expires_at: "2026-08-18T00:20:00.000Z",
});

describe("reconcilePersistedApproval", () => {
  it("restores a pending approval so the action card can render decisions", () => {
    expect(reconcilePersistedApproval(record, approval("pending"))).toEqual({
      kind: "pending",
      approval: approval("pending"),
    });
  });

  it("turns an already approved approval into a resolved execution event", () => {
    expect(reconcilePersistedApproval(record, approval("approved"))).toEqual({
      kind: "resolved",
      event: {
        type: "approval_resolved",
        conversation_id: "conv-1",
        tool_call_id: "call-1",
        approval_id: "approval-1",
        status: "approved",
      },
    });
  });

  it("preserves a rejected decision instead of leaving the record pending", () => {
    expect(reconcilePersistedApproval(record, approval("rejected"))).toEqual({
      kind: "resolved",
      event: {
        type: "approval_resolved",
        conversation_id: "conv-1",
        tool_call_id: "call-1",
        approval_id: "approval-1",
        status: "rejected",
      },
    });
  });

  it("closes a stale approval as cancelled when the backend no longer has it", () => {
    expect(reconcilePersistedApproval(record, null)).toEqual({
      kind: "resolved",
      event: {
        type: "approval_resolved",
        conversation_id: "conv-1",
        tool_call_id: "call-1",
        approval_id: "approval-1",
        status: "cancelled",
        reason: "审批记录已失效，原始操作未执行。",
      },
    });
  });
});
