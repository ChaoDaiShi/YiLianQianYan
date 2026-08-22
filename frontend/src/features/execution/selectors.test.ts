import { describe, expect, it } from "vitest";
import type { ExecutionRecord } from "./model";
import { selectCurrentAction } from "./selectors";

const record = (patch: Partial<ExecutionRecord>): ExecutionRecord => ({
  toolCallId: patch.toolCallId || "call",
  conversationId: "conv",
  name: "read_file",
  args: {},
  riskLevel: "low",
  executionStatus: "running",
  approvalStatus: "not_required",
  verificationStatus: "not_requested",
  sequence: 1,
  ...patch,
});

describe("selectCurrentAction", () => {
  it("prioritizes critical approval over high approval and failures", () => {
    const selected = selectCurrentAction([
      record({ toolCallId: "failed", executionStatus: "failed" }),
      record({
        toolCallId: "high",
        approvalStatus: "pending",
        riskLevel: "high",
      }),
      record({
        toolCallId: "critical",
        approvalStatus: "pending",
        riskLevel: "critical",
      }),
    ]);
    expect(selected?.toolCallId).toBe("critical");
  });

  it("prioritizes verification failure over running work", () => {
    const selected = selectCurrentAction([
      record({ toolCallId: "running" }),
      record({
        toolCallId: "verify",
        executionStatus: "succeeded",
        verificationStatus: "failed",
      }),
    ]);
    expect(selected?.toolCallId).toBe("verify");
  });
});
