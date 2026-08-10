import { describe, expect, it } from "vitest";
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
