import { describe, expect, it } from "vitest";
import approvalCardSource from "./ApprovalCard.tsx?raw";

describe("approval card presentation contract", () => {
  it("shows real operation details and explicit approval actions", () => {
    expect(approvalCardSource).toContain("formatToolDisplayName(approval.tool_name)");
    expect(approvalCardSource).toContain("approval.reason");
    expect(approvalCardSource).toContain("拒绝此次工具操作");
    expect(approvalCardSource).toContain("允许此次工具操作");
    expect(approvalCardSource).toContain("attestVoiceApprovalDisplayed");
    expect(approvalCardSource).toContain("session?.generation");
  });

  it("does not invent an impact field when the backend does not provide one", () => {
    expect(approvalCardSource).not.toContain("影响");
    expect(approvalCardSource).not.toContain("该操作需要明确授权");
  });
});
