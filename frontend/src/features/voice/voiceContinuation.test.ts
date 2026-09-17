import { describe, expect, it } from "vitest";
import type { AgentEvent } from "../../api/client";
import {
  reduceApprovalEvents,
  reduceConversationEvents,
} from "./voiceContinuation";

function event(type: string, extra: Partial<AgentEvent> = {}): AgentEvent {
  return { type, conversation_id: "conversation-a", ...extra };
}

describe("voice continuation evidence", () => {
  it("collects the real assistant stream only after a terminal done event", () => {
    const result = reduceConversationEvents([
      event("token", { token: "第三点是" }),
      event("token", { token: "安全验证。" }),
      event("done", { message_id: "assistant-1" }),
    ]);

    expect(result).toEqual({
      assistantText: "第三点是安全验证。",
      messageId: "assistant-1",
    });
  });

  it("rejects an incomplete or approval-paused conversation stream", () => {
    expect(() =>
      reduceConversationEvents([event("token", { token: "未完成" }), event("stream_end")]),
    ).toThrow("会话响应未完成");
    expect(() =>
      reduceConversationEvents([event("approval_required", { approval_id: "approval-1" })]),
    ).toThrow("会话等待审批");
  });

  it("accepts an approved continuation only with real verification", () => {
    const result = reduceApprovalEvents("approve", [
      event("approval_resolved", { status: "approved", approval_id: "approval-1" }),
      event("verification", {
        approval_id: "approval-1",
        verification_success: true,
        verification_reason: "window observed",
      }),
      event("done", { approval_id: "approval-1" }),
    ]);

    expect(result.verified).toBe(true);
    expect(result.narration).toContain("完成验证");
  });

  it("never turns a terminal event without verification into approval success", () => {
    expect(() =>
      reduceApprovalEvents("approve", [
        event("approval_resolved", { status: "approved", approval_id: "approval-1" }),
        event("done", { approval_id: "approval-1" }),
      ]),
    ).toThrow("审批操作没有通过真实验证");
  });

  it("returns a truthful rejection only after the existing approval stream resolves", () => {
    expect(
      reduceApprovalEvents("reject", [
        event("approval_resolved", { status: "rejected", approval_id: "approval-1" }),
        event("done", { approval_id: "approval-1" }),
      ]),
    ).toEqual({ narration: "已拒绝这项操作。", verified: false });
  });
});
