import { describe, expect, it, vi } from "vitest";
import {
  createInitialAgentRunState,
  createInitialAgentWorkspaceState,
  hydrateToolCallRecords,
  reduceAgentEvent,
  reduceExecutionWorkspace,
  selectActiveRun,
  toToolCallRecords,
} from "./reducer";

describe("reduceAgentEvent", () => {
  it("records real tool start and end times for elapsed display", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-17T12:00:00.000Z"));

    let state = createInitialAgentRunState("conv-1");
    state = reduceAgentEvent(state, {
      type: "tool_start",
      conversation_id: "conv-1",
      tool_call_id: "call-time",
      tool_name: "powershell",
      args: { command: "Get-Process" },
    });
    expect(state.records["call-time"].startedAt).toBe(
      Date.parse("2026-08-17T12:00:00.000Z")
    );

    vi.advanceTimersByTime(2100);
    state = reduceAgentEvent(state, {
      type: "tool_end",
      conversation_id: "conv-1",
      tool_call_id: "call-time",
      status: "success",
      result: "done",
    });
    expect(state.records["call-time"].finishedAt).toBe(
      Date.parse("2026-08-17T12:00:02.100Z")
    );
    vi.useRealTimers();
  });

  it("keeps tool completion separate from verification", () => {
    let state = createInitialAgentRunState("conv-1");
    state = reduceAgentEvent(state, {
      type: "tool_start",
      conversation_id: "conv-1",
      tool_call_id: "call-1",
      tool_name: "write_file",
      args: { path: "README.md" },
    });
    state = reduceAgentEvent(state, {
      type: "tool_end",
      conversation_id: "conv-1",
      tool_call_id: "call-1",
      status: "success",
      result: "written",
    });

    expect(state.records["call-1"].executionStatus).toBe("succeeded");
    expect(state.records["call-1"].verificationStatus).toBe("not_requested");

    state = reduceAgentEvent(state, {
      type: "verification",
      conversation_id: "conv-1",
      tool_call_id: "call-1",
      verification_success: false,
      verification_reason: "content mismatch",
    });

    expect(state.records["call-1"].executionStatus).toBe("succeeded");
    expect(state.records["call-1"].verificationStatus).toBe("failed");
    expect(state.records["call-1"].verificationReason).toBe("content mismatch");
  });

  it("preserves a failed tool_end as an execution failure", () => {
    let state = createInitialAgentRunState("conv-1");
    state = reduceAgentEvent(state, {
      type: "tool_start",
      conversation_id: "conv-1",
      tool_call_id: "call-failed",
      tool_name: "powershell",
      args: { command: "Get-Process" },
    });
    state = reduceAgentEvent(state, {
      type: "tool_end",
      conversation_id: "conv-1",
      tool_call_id: "call-failed",
      status: "error",
      result: "access denied",
    });

    expect(state.records["call-failed"].executionStatus).toBe("failed");
    expect(state.records["call-failed"].result).toBe("access denied");
  });

  it("merges approval events into one record and distinguishes rejection", () => {
    let state = createInitialAgentRunState("conv-1");
    const required = {
      type: "approval_required",
      conversation_id: "conv-1",
      tool_call_id: "call-2",
      tool_name: "process",
      approval_id: "approval-1",
      risk_level: "high",
      reason: "starts a process",
      args: { command: "cargo run" },
    };
    state = reduceAgentEvent(state, required);
    state = reduceAgentEvent(state, required);
    state = reduceAgentEvent(state, {
      type: "approval_resolved",
      conversation_id: "conv-1",
      tool_call_id: "call-2",
      approval_id: "approval-1",
      status: "rejected",
    });

    expect(state.order).toEqual(["call-2"]);
    expect(state.records["call-2"].approvalStatus).toBe("rejected");
    expect(state.records["call-2"].executionStatus).toBe("rejected");
  });

  it("ignores unknown events without mutating state", () => {
    const state = createInitialAgentRunState("conv-1");
    const next = reduceAgentEvent(state, {
      type: "future_event",
      conversation_id: "conv-1",
    });
    expect(next).toBe(state);
  });

  it("does not mark an approval pause as a broken connection", () => {
    let state = createInitialAgentRunState("conv-1");
    state = reduceAgentEvent(state, { type: "connected", conversation_id: "conv-1" });
    state = reduceAgentEvent(state, {
      type: "approval_required",
      conversation_id: "conv-1",
      tool_call_id: "call-3",
      approval_id: "approval-3",
      tool_name: "process",
      risk_level: "high",
      args: {},
    });
    state = reduceAgentEvent(state, { type: "stream_end", conversation_id: "conv-1" });
    expect(state.connection).toBe("connected");
  });

  it("does not turn a completed stream into an interruption", () => {
    let state = createInitialAgentRunState("conv-1");
    state = reduceAgentEvent(state, { type: "connected", conversation_id: "conv-1" });
    state = reduceAgentEvent(state, {
      type: "done",
      conversation_id: "conv-1",
      message_id: "message-1",
    });
    state = reduceAgentEvent(state, { type: "stream_end", conversation_id: "conv-1" });
    expect(state.connection).toBe("connected");
  });

  it("keeps observable error state for the friendly UI error layer", () => {
    const state = reduceAgentEvent(createInitialAgentRunState("conv-1"), {
      type: "error",
      conversation_id: "conv-1",
      error: "connection refused",
    });

    expect(state.connection).toBe("error");
    expect(state.terminal).toBe(true);
    expect(state.terminalError).toBe("connection refused");
  });

  it("marks an in-flight tool as interrupted when its stream stops", () => {
    let state = createInitialAgentRunState("conv-1");
    state = reduceAgentEvent(state, {
      type: "tool_start",
      conversation_id: "conv-1",
      tool_call_id: "call-interrupted",
      tool_name: "process",
      args: {},
    });
    state = reduceAgentEvent(state, {
      type: "stream_end",
      conversation_id: "conv-1",
    });

    expect(state.connection).toBe("interrupted");
    expect(state.records["call-interrupted"].executionStatus).toBe(
      "interrupted"
    );
  });

  it("hydrates legacy tool_calls only when the input is an array", () => {
    expect(hydrateToolCallRecords({ invalid: true })).toEqual([]);
    const hydrated = hydrateToolCallRecords([
      { toolCallId: "legacy-1", name: "read_file", args: {}, status: "success" },
    ]);
    expect(hydrated[0].verificationStatus).toBe("not_requested");
    expect(toToolCallRecords(hydrated)).toHaveLength(1);
  });

  it("keeps each conversation run when the active conversation changes", () => {
    let workspace = createInitialAgentWorkspaceState("conv-1");
    workspace = reduceExecutionWorkspace(workspace, {
      type: "agent_event",
      event: { type: "token", conversation_id: "conv-1", token: "one" },
    });
    workspace = reduceExecutionWorkspace(workspace, {
      type: "activate_conversation",
      conversationId: "conv-2",
    });
    workspace = reduceExecutionWorkspace(workspace, {
      type: "agent_event",
      event: { type: "token", conversation_id: "conv-2", token: "two" },
    });
    workspace = reduceExecutionWorkspace(workspace, {
      type: "activate_conversation",
      conversationId: "conv-1",
    });

    expect(selectActiveRun(workspace).content).toBe("one");
    expect(workspace.runs["conv-2"].content).toBe("two");
  });

  it("starts a fresh run without replacing other conversations", () => {
    let workspace = createInitialAgentWorkspaceState("conv-1");
    workspace = reduceExecutionWorkspace(workspace, {
      type: "agent_event",
      event: { type: "token", conversation_id: "conv-1", token: "old" },
    });
    workspace = reduceExecutionWorkspace(workspace, {
      type: "activate_conversation",
      conversationId: "conv-2",
    });
    workspace = reduceExecutionWorkspace(workspace, {
      type: "agent_event",
      event: { type: "token", conversation_id: "conv-2", token: "keep" },
    });
    workspace = reduceExecutionWorkspace(workspace, {
      type: "start_run",
      conversationId: "conv-1",
    });

    expect(workspace.runs["conv-1"].content).toBe("");
    expect(workspace.runs["conv-1"].connection).toBe("connecting");
    expect(workspace.runs["conv-2"].content).toBe("keep");
  });

  it("deduplicates hydrated records by tool call id", () => {
    let workspace = createInitialAgentWorkspaceState("conv-1");
    workspace = reduceExecutionWorkspace(workspace, {
      type: "hydrate_history",
      conversationId: "conv-1",
      toolCalls: [
        { toolCallId: "same-call", name: "read_file", args: {}, status: "running" },
        { toolCallId: "same-call", name: "read_file", args: {}, status: "success" },
      ],
    });

    expect(workspace.runs["conv-1"].order).toEqual(["same-call"]);
    expect(workspace.runs["conv-1"].records["same-call"].executionStatus).toBe(
      "succeeded"
    );
  });
});
describe("execution history hydration", () => {
  it("restores persisted tool execution records when reopening a conversation", () => {
    const initial = createInitialAgentWorkspaceState("conversation-1");
    const hydrated = reduceExecutionWorkspace(initial, {
      type: "hydrate_history",
      conversationId: "conversation-1",
      toolCalls: [],
      executionHistory: [
        {
          conversationId: "conversation-1",
          toolCallId: "call-1",
          name: "open_notepad",
          args: { title: "记事本" },
          riskLevel: "low",
          approvalStatus: "not_required",
          executionStatus: "succeeded",
          verificationStatus: "passed",
          verificationReason: "窗口已出现",
          result: "已打开记事本。",
          sequence: 1,
        },
      ],
    });

    const records = selectActiveRun(hydrated).order.map(
      (id) => selectActiveRun(hydrated).records[id]
    );
    expect(records).toHaveLength(1);
    expect(records[0]).toMatchObject({
      toolCallId: "call-1",
      name: "open_notepad",
      executionStatus: "succeeded",
      verificationStatus: "passed",
    });
  });
});
