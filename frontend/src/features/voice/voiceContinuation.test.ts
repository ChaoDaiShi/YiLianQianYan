import { beforeEach, describe, expect, it, vi } from "vitest";
import { loadConversation } from "../../api/conversations";
import { sendMessage } from "../../api/chat";
import { approveAction } from "../../api/approvals";
import type { AgentEvent } from "../../api/client";
import {
  reduceApprovalEvents,
  reduceConversationEvents,
  runVoiceContinuation,
} from "./voiceContinuation";

vi.mock("../../api/conversations", () => ({ loadConversation: vi.fn() }));
vi.mock("../../api/chat", () => ({ sendMessage: vi.fn(), stopGeneration: vi.fn() }));
vi.mock("../../api/approvals", () => ({ approveAction: vi.fn().mockResolvedValue(undefined), rejectAction: vi.fn().mockResolvedValue(undefined) }));
beforeEach(() => vi.resetAllMocks());

function event(type: string, extra: Partial<AgentEvent> = {}): AgentEvent {
  return { type, conversation_id: "conversation-a", ...extra };
}

describe("voice continuation evidence", () => {
  it("refuses approval continuations from an older server without trusted display attestation", async () => {
    await expect(runVoiceContinuation({ kind: "approval", conversation_id: "conversation-a", approval_id: "approval-1", decision: "approve" } as never))
      .rejects.toThrow("voice_approval_attestation_required");
    expect(approveAction).not.toHaveBeenCalled();
  });
  it("executes a server-attested approval through the existing continuation", async () => {
    vi.mocked(approveAction).mockImplementationOnce(async (_id, _conversation, onEvent) => {
      onEvent(event("approval_resolved", { status: "approved", approval_id: "approval-1" }));
      onEvent(event("verification", { approval_id: "approval-1", verification_success: true }));
      onEvent(event("done", { approval_id: "approval-1" }));
    });
    const result = await runVoiceContinuation({
      kind: "approval",
      conversation_id: "conversation-a",
      approval_id: "approval-1",
      attestation_id: "attestation-1",
      decision: "approve",
    });
    expect(result.verified).toBe(true);
    expect(approveAction).toHaveBeenCalledTimes(1);
    expect(approveAction).toHaveBeenCalledWith(
      "approval-1",
      "conversation-a",
      expect.any(Function),
      undefined,
      "attestation-1",
    );
  });
  it("settles cancellation even when the chat stream never sends another event", async () => {
    vi.mocked(loadConversation).mockResolvedValueOnce({ id: "conversation-a", messages: [] } as Awaited<ReturnType<typeof loadConversation>>);
    const controller = new AbortController();
    let started!: () => void;
    const start = new Promise<void>((resolve) => { started = resolve; });
    vi.mocked(sendMessage).mockImplementationOnce(() => { started(); return new AbortController(); });
    const result = runVoiceContinuation({ kind: "conversation", conversation_id: "conversation-a", message: "hello" }, { signal: controller.signal });
    await start;
    controller.abort();
    await expect(result).rejects.toThrow("取消");
    vi.mocked(sendMessage).mockClear();
  });
  it("revalidates the bound identity after loading and before committing the chat message", async () => {
    let current = true;
    vi.mocked(loadConversation).mockImplementationOnce(async () => {
      current = false;
      return { id: "conversation-a", messages: [] } as Awaited<ReturnType<typeof loadConversation>>;
    });
    vi.mocked(sendMessage).mockImplementationOnce((_message, _conversation, onEvent) => {
      onEvent({ type: "done", conversation_id: "conversation-a" });
      return new AbortController();
    });
    await expect(runVoiceContinuation({ kind: "conversation", conversation_id: "conversation-a", message: "hello" },
      { isCurrent: () => current })).rejects.toThrow("取消");
    expect(sendMessage).not.toHaveBeenCalled();
  });
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
