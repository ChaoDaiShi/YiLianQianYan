import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createConversationRuntime } from "./conversationRuntime";
import type { AgentEvent } from "../../api/chat";
import type { Conversation } from "../../types";
import type { PendingApproval } from "../../types/approval";

const approval: PendingApproval = {
  approval_id: "approval", conversation_id: "a", tool_call_id: "call", tool_name: "bash",
  arguments: { command: "echo approved" }, risk_level: "high", reason: "run command",
  status: "pending", created_at: "", expires_at: "",
};
const conversation = (status: Conversation["run_status"]): Conversation => ({
  id: "a", title: "A", messages: [], execution_history: [], created_at: 1, updated_at: 2,
  run_status: status,
});

function setup(initial = conversation("idle")) {
  let emit: (event: AgentEvent) => void = () => undefined;
  const load = vi.fn(async () => initial);
  const pending = vi.fn(async (): Promise<PendingApproval[]> => []);
  const lookup = vi.fn(async (): Promise<PendingApproval | null> => null);
  const setMode = vi.fn(async (_id: string, mode: "chat" | "work" | "create") => ({ mode }));
  const decide = vi.fn(async (_id: string, _conv: string | null, cb: (event: AgentEvent) => void) => {
    emit = cb;
  });
  const send = vi.fn((_text: string, _id: string | null, cb: (event: AgentEvent) => void) => { emit = cb; return new AbortController(); });
  const runtime = createConversationRuntime({
    loadConversation: load, listPendingApprovals: pending, getApproval: lookup, setWorkspaceMode: setMode,
    sendMessage: send,
    approveAction: decide, rejectAction: decide, stopGeneration: vi.fn(async () => undefined),
  });
  return { runtime, load, pending, lookup, decide, setMode, send, emit: (event: Omit<AgentEvent, "conversation_id">) => emit({ conversation_id: "a", ...event }) };
}

describe("conversation runtime ownership", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });
  it("keeps actual provider reasoning and called tools on completion and after reloading protocol history", async () => {
    const { runtime, emit, load } = setup();
    runtime.send("a", "read file");
    emit({ type: "reasoning", token: "provider thought" });
    emit({ type: "tool_start", tool_call_id: "real-call", tool_name: "read_file", args: { path: "a.txt" } });
    emit({ type: "tool_end", tool_call_id: "real-call", tool_name: "read_file", status: "success", result: "actual file" });
    emit({ type: "token", token: "answer" });
    emit({ type: "done", message_id: "answer" });
    const live = runtime.snapshot("a").messages;
    expect(live[live.length - 1].process?.reasoning).toBe("provider thought");
    expect(live[live.length - 1].process?.tools[0].execution?.status).toBe("success");
    load.mockResolvedValue({ ...conversation("completed"), messages: [{ id: "u", role: "user", content: "read file", created_at: 1 }, { id: "saved", role: "assistant", content: "answer", created_at: 2 }], turn_reasoning: { u: "provider thought" } });
    emit({ type: "stream_end" });
    await runtime.refresh("a");
    expect(runtime.snapshot("a").messages[1].process?.reasoning).toBe("provider thought");
    expect(runtime.snapshot("a").live).toBe(false);
  });
  it("takes automatic workspace changes only from real backend mode events", async () => {
    const { runtime, emit, load } = setup();
    runtime.send("a", "请帮我处理项目");
    expect(runtime.snapshot("a").workspaceMode).toBe("chat");
    emit({ type: "workspace_mode", mode: "work", reason: "需要操作工作区", automatic: true });
    expect(runtime.snapshot("a").workspaceMode).toBe("work");
    expect(runtime.snapshot("a").modeNotice?.automatic).toBe(true);
    load.mockResolvedValue({ ...conversation("completed"), workspace_mode: "work" });
    emit({ type: "stream_end" });
    await runtime.refresh("a");
    expect(runtime.snapshot("a").workspaceMode).toBe("work");
  });
  it("persists manual selection and restores the backend workspace mode on a fresh load", async () => {
    const { runtime, setMode } = setup({ ...conversation("idle"), workspace_mode: "create" });
    await runtime.refresh("a");
    expect(runtime.snapshot("a").workspaceMode).toBe("create");
    await runtime.changeWorkspaceMode("a", "work");
    expect(setMode).toHaveBeenCalledWith("a", "work");
    expect(runtime.snapshot("a").workspaceMode).toBe("work");
  });
  it("retains the real mode when manual persistence fails", async () => {
    const { runtime, setMode } = setup({ ...conversation("idle"), workspace_mode: "work" });
    await runtime.refresh("a");
    setMode.mockRejectedValue(new Error("mode could not be saved"));
    await expect(runtime.changeWorkspaceMode("a", "create")).rejects.toThrow("could not be saved");
    expect(runtime.snapshot("a").workspaceMode).toBe("work");
    expect(runtime.snapshot("a").modeError).toBe("mode could not be saved");
    expect(runtime.snapshot("a").modeChanging).toBe(false);
  });
  it("keeps a draft mode local and dispatches it explicitly when a real conversation is created", async () => {
    const { runtime, setMode, send } = setup();
    await runtime.changeWorkspaceMode("__draft__", "create");
    expect(setMode).not.toHaveBeenCalled();
    runtime.send("a", "creative task", null, { workspace_mode: runtime.snapshot("__draft__").workspaceMode });
    expect(send).toHaveBeenCalledWith("creative task", "a", expect.any(Function), null, { workspace_mode: "create" });
  });

  it("reconciles an approval conflict and continues observing the actual running task", async () => {
    const { runtime, pending, load, decide } = setup(conversation("waiting_approval"));
    pending.mockResolvedValue([approval]);
    await runtime.refresh("a");
    pending.mockResolvedValue([]);
    load.mockResolvedValue(conversation("running"));
    decide.mockRejectedValue(new Error("HTTP 409 already consumed"));
    await expect(runtime.decide(approval, "approve")).rejects.toThrow("409");
    expect(runtime.snapshot("a").pendingApprovals).toEqual([]);
    expect(runtime.snapshot("a").loading).toBe(true);
    load.mockResolvedValue({ ...conversation("completed"), messages: [{ id: "actual", role: "assistant", content: "finished elsewhere", created_at: 3 }] });
    await vi.advanceTimersByTimeAsync(1500);
    expect(runtime.snapshot("a").messages[0].content).toBe("finished elsewhere");
  });

  it("clears a disconnected accepted stream error when its backend run completes", async () => {
    const { runtime, emit, load } = setup(conversation("running"));
    runtime.send("a", "continue");
    emit({ type: "connected" });
    emit({ type: "token", token: "partial" });
    emit({ type: "error", error: "network lost" });
    load.mockResolvedValue({ ...conversation("completed"), messages: [{ id: "answer", role: "assistant", content: "complete answer", created_at: 3 }] });
    emit({ type: "stream_end" });
    await runtime.refresh("a");
    expect(runtime.snapshot("a").error).toBeNull();
    expect(runtime.snapshot("a").finished).toBe(true);
    expect(runtime.snapshot("a").messages[0].content).toBe("complete answer");
  });

  it("uses a new backend run timestamp to recover acceptance before any token arrived", async () => {
    const { runtime, emit, load } = setup(conversation("idle"));
    runtime.send("a", "continue");
    emit({ type: "error", error: "connection dropped before connected frame" });
    load.mockResolvedValue({ ...conversation("completed"), run_started_at: Date.now() + 1 });
    emit({ type: "stream_end" });
    await runtime.refresh("a");
    expect(runtime.snapshot("a").error).toBeNull();
    expect(runtime.snapshot("a").finished).toBe(true);
  });

  it("keeps HTTP admission rejection separate from a different completed backend run", async () => {
    const { runtime, emit, load } = setup(conversation("idle"));
    runtime.send("a", "rejected turn");
    emit({ type: "error", error: "HTTP 409 rejected", request_rejected: true });
    load.mockResolvedValue({ ...conversation("completed"), run_started_at: Date.now() + 1 });
    emit({ type: "stream_end" });
    await runtime.refresh("a");
    expect(runtime.snapshot("a").error).toBe("HTTP 409 rejected");
    expect(runtime.snapshot("a").finished).toBe(false);
  });

  it("polls a recovered background run until the backend persists its answer", async () => {
    const { runtime, load } = setup(conversation("running"));
    await runtime.refresh("a");
    load.mockResolvedValue({ ...conversation("completed"), messages: [{ id: "saved-answer", role: "assistant", content: "persisted answer", created_at: 3 }] });
    await vi.advanceTimersByTimeAsync(1500);
    expect(runtime.snapshot("a").messages[0].content).toBe("persisted answer");
    expect(runtime.snapshot("a").loading).toBe(false);
    expect(runtime.snapshot("a").run.terminal).toBe(true);
    expect(load).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(5000);
    expect(load).toHaveBeenCalledTimes(2);
  });
  it("keeps a rejected request visible when the persisted conversation has no new run", async () => {
    const { runtime, emit } = setup(conversation("idle"));
    runtime.send("a", "cannot run");
    emit({ type: "error", error: "selected model unavailable" });
    emit({ type: "stream_end" });
    await runtime.refresh("a");
    expect(runtime.snapshot("a").error).toBe("selected model unavailable");
    expect(runtime.snapshot("a").finished).toBe(false);
  });
  it("keeps other conversations isolated when an unobserved conversation completes", () => {
    const { runtime, emit } = setup();
    runtime.send("a", "hello");
    const unsubscribe = runtime.subscribe("b", () => undefined);
    emit({ type: "token", token: "answer for a" });
    emit({ type: "done", message_id: "answer-a" });
    expect(runtime.snapshot("b").messages).toEqual([]);
    expect(runtime.snapshot("b").run.content).toBe("");
    const messages = runtime.snapshot("a").messages;
    expect(messages[messages.length - 1].content).toBe("answer for a");
    unsubscribe();
  });

  it("does not claim EOF completed a task whose backend is still running", async () => {
    const { runtime, emit } = setup(conversation("running"));
    runtime.send("a", "continue");
    emit({ type: "stream_end" });
    await runtime.refresh("a");
    expect(runtime.snapshot("a").loading).toBe(true);
    expect(runtime.snapshot("a").run.terminal).toBe(false);
    expect(runtime.snapshot("a").finished).toBe(false);
  });
  it("reconciles stale pending history with the actual resolved approval", async () => {
    const persisted = { ...conversation("completed"), execution_history: [{ toolCallId: "call", approvalId: "approval", name: "bash", args: {}, approvalStatus: "pending", executionStatus: "awaiting_approval", sequence: 1 }] };
    const { runtime, lookup } = setup(persisted);
    lookup.mockResolvedValue({ ...approval, status: "rejected" });
    await runtime.refresh("a");
    expect(runtime.snapshot("a").run.records.call.approvalStatus).toBe("rejected");
    expect(runtime.snapshot("a").run.records.call.executionStatus).toBe("rejected");
    expect(runtime.snapshot("a").pendingApprovals).toEqual([]);
  });
  it("keeps consuming tokens and completion while the page has no subscribers", async () => {
    const { runtime, emit } = setup();
    const unsubscribe = runtime.subscribe("a", () => undefined);
    runtime.send("a", "hello");
    emit({ type: "token", token: "first " });
    unsubscribe();
    emit({ type: "token", token: "second" });
    emit({ type: "done", message_id: "answer" });
    const snapshot = runtime.snapshot("a");
    expect(snapshot.messages[snapshot.messages.length - 1]?.content).toBe("first second");
    expect(snapshot.loading).toBe(false);
    expect(snapshot.run.terminal).toBe(true);
  });

  it("recovers a pending approval as current actionable execution after a fresh page load", async () => {
    const { runtime, pending, decide, emit } = setup(conversation("waiting_approval"));
    pending.mockResolvedValue([approval]);
    await runtime.refresh("a");
    expect(runtime.snapshot("a").run.records.call.approvalId).toBe("approval");
    expect(runtime.snapshot("a").run.records.call.executionStatus).toBe("awaiting_approval");
    const decision = runtime.decide(approval, "approve");
    expect(decide).toHaveBeenCalledWith("approval", "a", expect.any(Function));
    emit({ type: "approval_resolved", approval_id: "approval", tool_call_id: "call", status: "approved" });
    emit({ type: "token", token: "continued" });
    emit({ type: "done", message_id: "continued-answer" });
    const messages = runtime.snapshot("a").messages;
    expect(messages[messages.length - 1]?.content).toBe("continued");
    await decision;
  });

  it("recovers genuine running and failed backend state without manufacturing completion", async () => {
    const { runtime, load } = setup(conversation("running"));
    await runtime.refresh("a");
    expect(runtime.snapshot("a").loading).toBe(true);
    expect(runtime.snapshot("a").run.terminal).toBe(false);
    load.mockResolvedValue({ ...conversation("failed"), run_error: "provider rejected" });
    await runtime.refresh("a");
    expect(runtime.snapshot("a").loading).toBe(false);
    expect(runtime.snapshot("a").error).toBe("provider rejected");
    expect(runtime.snapshot("a").messages).toEqual([]);
  });

  it("does not let hydration replace a newer live run", async () => {
    const { runtime, load, emit } = setup();
    let resolve!: (value: Conversation) => void;
    load.mockReturnValue(new Promise((done) => { resolve = done; }));
    const refreshing = runtime.refresh("a");
    runtime.send("a", "new turn");
    emit({ type: "token", token: "new answer" });
    resolve(conversation("idle"));
    await refreshing;
    expect(runtime.snapshot("a").run.content).toBe("new answer");
    expect(runtime.snapshot("a").messages[0].content).toBe("new turn");
  });
});
