import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createConversationRuntime } from "./conversationRuntime";
import type { AgentEvent } from "../../api/chat";
import type { ConversationInput } from "../../api/chat";
import type { Conversation } from "../../types";
import { MessageMutationError, type MessageMutationResult, type MessageRollbackResult } from "../../api/conversations";
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
  const rollback = vi.fn(async (_id: string, _message: string): Promise<MessageRollbackResult> => ({ conversation: initial, deleted_message_ids: [], removed_protocol_count: 0, draft: { text: "", resources: [], resource_scope: "conversation" } }));
  const deleteMessage = vi.fn(async (_id: string, _message: string): Promise<MessageMutationResult> => ({ conversation: initial, deleted_message_ids: [], removed_protocol_count: 0 }));
  let inputs: ConversationInput[] = [];
  const answerInput = vi.fn((_question: string, _answer: { revision: number }, cb: (event: AgentEvent) => void, _conversation: string) => { emit = cb; return new AbortController(); });
  const runtime = createConversationRuntime({
    listConversationInputs: vi.fn(async () => inputs),
    answerConversationInput: answerInput,
    loadConversation: load, listPendingApprovals: pending, getApproval: lookup, setWorkspaceMode: setMode,
    rollbackMessage: rollback, deleteMessage,
    sendMessage: send,
    approveAction: decide, rejectAction: decide, stopGeneration: vi.fn(async () => undefined),
  });
  return { runtime, load, pending, lookup, decide, setMode, send, rollback, deleteMessage, answerInput, setInputs: (value: ConversationInput[]) => { inputs = value; }, emit: (event: Omit<AgentEvent, "conversation_id">) => emit({ conversation_id: "a", ...event }) };
}

describe("conversation runtime ownership", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });
  it("aligns the optimistic user bubble with the accepted server message identity", () => {
    const { runtime, emit } = setup();
    runtime.send("a", "查看系统服务");
    emit({ type: "connected", user_message_id: "accepted-user" });
    expect(runtime.snapshot("a").messages).toHaveLength(1);
    expect(runtime.snapshot("a").messages[0].id).toBe("accepted-user");
  });

  it("waits for authoritative deletion and retains other visible messages", async () => {
    const user = { id: "user", role: "user", content: "问题", created_at: 1 };
    const reply = { id: "reply", role: "assistant", content: "回答", created_at: 2 };
    const before: Conversation = { ...conversation("completed"), messages: [user, reply] };
    const after: Conversation = { ...before, messages: [user] };
    const { runtime, load, deleteMessage } = setup(before);
    await runtime.refresh("a");
    let resolve!: (value: MessageMutationResult) => void;
    deleteMessage.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    const deleting = runtime.deleteMessage("a", "reply");
    expect(runtime.snapshot("a").messages.map((message) => message.id)).toEqual(["user", "reply"]);
    expect(runtime.send("a", "并发发送")).toBe(false);
    load.mockResolvedValue(after);
    resolve({ conversation: after, deleted_message_ids: ["reply"], removed_protocol_count: 0 });
    await deleting;
    expect(runtime.snapshot("a").messages.map((message) => message.id)).toEqual(["user"]);
    expect(runtime.snapshot("a").historyMutation).toBeNull();
  });

  it("returns the server rollback draft and does not widen an invalid history rollback", async () => {
    const user = { id: "user", role: "user", content: "问题", created_at: 1 };
    const before: Conversation = { ...conversation("failed"), messages: [user], run_error: "provider failure" };
    const { runtime, load, rollback } = setup(before);
    await runtime.refresh("a");
    rollback.mockRejectedValueOnce(new MessageMutationError("更早的工具记录不完整", { code: "chat_history_protocol_invalid", affected_user_message_id: "earlier-user" }));
    await expect(runtime.rollback("a", "user")).rejects.toThrow("更早的工具记录不完整");
    expect(runtime.snapshot("a").messages[0].id).toBe("user");
    expect(runtime.snapshot("a").affectedUserMessageId).toBe("earlier-user");
    expect(rollback).toHaveBeenCalledTimes(1);
    const after = conversation("idle");
    const draft = { text: "问题", resources: [{ id: "resource", name: "资料.txt" }], resource_scope: "conversation" as const };
    rollback.mockResolvedValueOnce({ conversation: after, deleted_message_ids: ["user"], removed_protocol_count: 0, draft });
    load.mockResolvedValue(after);
    await expect(runtime.rollback("a", "user")).resolves.toMatchObject({ draft });
    expect(runtime.snapshot("a").messages).toEqual([]);
    expect(runtime.snapshot("a").affectedUserMessageId).toBeNull();
  });
  it("does not select an older persisted message for retry after HTTP admission rejects a new draft", async () => {
    const previous: Conversation = { ...conversation("completed"), messages: [{ id: "previous-user", role: "user", content: "以前的问题", created_at: 1 }] };
    const { runtime, emit } = setup(previous);
    await runtime.refresh("a");
    runtime.send("a", "尚未被接受的新问题");
    emit({ type: "error", error: "HTTP 400", request_rejected: true });
    emit({ type: "stream_end" });
    await runtime.refresh("a");
    expect(runtime.snapshot("a").retryMessageId).toBeNull();
    expect(runtime.snapshot("a").lastSubmittedText).toBe("尚未被接受的新问题");
    expect(runtime.snapshot("a").messages.map((message) => message.id)).toEqual(["previous-user"]);
  });

  it("identifies the failed persisted user message for retry after reloading", async () => {
    const { runtime } = setup({ ...conversation("failed"), run_error: "provider rejected", messages: [{ id: "failed-user", role: "user", content: "查看系统服务", created_at: 1 }] });
    await runtime.refresh("a");
    expect(runtime.snapshot("a").retryMessageId).toBe("failed-user");
  });
  it("restores necessary input and submits the original question once without approval", async () => {
    const { runtime, setInputs, answerInput, decide } = setup(conversation("waiting_input"));
    const question: ConversationInput = { question_id: "question", run_id: "original-turn", conversation_id: "a", tool_call_id: "ask", title: "输出语言？", kind: "text", options: [], allow_text: true, status: "pending", revision: 1, answer: null, created_at: 1, resolved_at: null };
    setInputs([question]);
    await runtime.refresh("a");
    expect(runtime.snapshot("a").pendingInputs).toEqual([question]);
    expect(runtime.send("a", "another turn")).toBe(false);
    expect(runtime.answer("a", question, { revision: 1, text: "中文" })).toBe(true);
    expect(runtime.answer("a", question, { revision: 1, text: "中文" })).toBe(false);
    expect(answerInput).toHaveBeenCalledTimes(1);
    expect(decide).not.toHaveBeenCalled();
  });
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
