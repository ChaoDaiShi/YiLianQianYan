import { describe, expect, it } from "vitest";
import { filterToolDiagnostics, toolDiagnosticStatus, conversationLogPath } from "./toolDiagnostics";
import type { ToolDiagnostic } from "../../api/system";

const record: ToolDiagnostic = { conversationId: "session / 1", conversationTitle: "项目检查", toolCallId: "call-1", name: "read_file", args: { path: "missing.txt" }, executionStatus: "failed", verificationStatus: "not_requested", result: "file does not exist", updatedAt: 100, riskLevel: "low", approvalStatus: "not_required", sequence: 1 };
describe("tool diagnostics", () => {
  it("searches actual failure, arguments and conversation fields", () => {
    expect(filterToolDiagnostics([record], "missing.txt")).toEqual([record]);
    expect(filterToolDiagnostics([record], "项目检查")).toEqual([record]);
    expect(filterToolDiagnostics([record], "access denied")).toEqual([]);
  });
  it("distinguishes execution failures and verification failures", () => {
    expect(toolDiagnosticStatus(record)).toEqual({ label: "执行失败", tone: "danger" });
    expect(toolDiagnosticStatus({ ...record, executionStatus: "succeeded", verificationStatus: "failed" })).toEqual({ label: "验证失败", tone: "danger" });
    expect(toolDiagnosticStatus({ ...record, executionStatus: "succeeded" })).toEqual({ label: "执行成功", tone: "success" });
  });
  it("returns to the existing conversation route with an encoded identifier", () => {
    expect(conversationLogPath(record)).toBe("/chat/session%20%2F%201");
  });
});
