import { approveAction, rejectAction } from "../../api/approvals";
import { sendMessage, stopGeneration, type AgentEvent } from "../../api/chat";
import { loadConversation } from "../../api/conversations";
import type { VoiceContinuation } from "../../api/voice";

export interface ConversationContinuationEvidence {
  assistantText: string;
  messageId: string | null;
}

export interface ApprovalContinuationEvidence {
  narration: string;
  verified: boolean;
}

export interface VoiceContinuationResult {
  narration: string;
  verified: boolean;
}

export interface RunVoiceContinuationOptions {
  signal?: AbortSignal;
  onEvent?: (event: AgentEvent) => void;
}

function findLastEvent(
  events: AgentEvent[],
  predicate: (event: AgentEvent) => boolean,
): AgentEvent | undefined {
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const event = events[index];
    if (predicate(event)) return event;
  }
  return undefined;
}

export function reduceConversationEvents(
  events: AgentEvent[],
): ConversationContinuationEvidence {
  const failure = events.find((event) => event.type === "error");
  if (failure) throw new Error(failure.error || "会话响应失败");
  if (events.some((event) => event.type === "approval_required")) {
    throw new Error("会话等待审批，不能把未完成内容交给 TTS");
  }
  const done = findLastEvent(events, (event) => event.type === "done");
  if (!done) throw new Error("会话响应未完成，不能交给 TTS");
  const assistantText = events
    .filter((event) => event.type === "token")
    .map((event) => event.token || "")
    .join("")
    .trim();
  if (!assistantText) throw new Error("会话没有产生可播报的 assistant response");
  return { assistantText, messageId: done.message_id ?? null };
}

export function reduceApprovalEvents(
  decision: "approve" | "reject",
  events: AgentEvent[],
): ApprovalContinuationEvidence {
  const failure = events.find((event) => event.type === "error");
  if (failure) throw new Error(failure.error || "审批 continuation 执行失败");
  const expectedStatus = decision === "approve" ? "approved" : "rejected";
  const resolved = events.some(
    (event) => event.type === "approval_resolved" && event.status === expectedStatus,
  );
  if (!resolved || !events.some((event) => event.type === "done")) {
    throw new Error("审批没有通过现有 continuation 完整解决");
  }
  if (decision === "reject") {
    return { narration: "已拒绝这项操作。", verified: false };
  }
  const verification = findLastEvent(events, (event) => event.type === "verification");
  if (verification?.verification_success !== true) {
    throw new Error("审批操作没有通过真实验证");
  }
  return { narration: "审批已通过，操作已经执行并完成验证。", verified: true };
}

function abortError(): Error {
  return new Error("语音 continuation 已取消");
}

async function waitForPersistedAssistant(
  conversationId: string,
  assistantText: string,
  previousMessageIds: Set<string>,
  signal?: AbortSignal,
): Promise<void> {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (signal?.aborted) throw abortError();
    const conversation = await loadConversation(conversationId);
    const messages = Array.isArray(conversation?.messages) ? conversation.messages : [];
    const persisted = messages.some(
      (message: { id?: string; role?: string; content?: string }) =>
        message.role === "assistant" &&
        message.content?.trim() === assistantText &&
        (!message.id || !previousMessageIds.has(message.id)),
    );
    if (persisted) return;
    await new Promise((resolve) => window.setTimeout(resolve, 25));
  }
  throw new Error("assistant response 尚未持久化，已阻止 TTS");
}

async function runConversationContinuation(
  continuation: Extract<VoiceContinuation, { kind: "conversation" }>,
  options: RunVoiceContinuationOptions,
): Promise<VoiceContinuationResult> {
  const before = await loadConversation(continuation.conversation_id);
  const previousMessageIds = new Set<string>(
    (Array.isArray(before?.messages) ? before.messages : [])
      .map((message: { id?: string }) => message.id)
      .filter((id: unknown): id is string => typeof id === "string"),
  );
  const events: AgentEvent[] = [];
  let requestController: AbortController | null = null;
  const onAbort = () => {
    requestController?.abort();
    void stopGeneration(continuation.conversation_id);
  };
  options.signal?.addEventListener("abort", onAbort, { once: true });
  try {
    await new Promise<void>((resolve, reject) => {
      if (options.signal?.aborted) {
        reject(abortError());
        return;
      }
      requestController = sendMessage(
        continuation.message,
        continuation.conversation_id,
        (event) => {
          events.push(event);
          options.onEvent?.(event);
          if (event.type === "done" || event.type === "error" || event.type === "stream_end") {
            resolve();
          }
        },
      );
    });
    if (options.signal?.aborted) throw abortError();
    const evidence = reduceConversationEvents(events);
    await waitForPersistedAssistant(
      continuation.conversation_id,
      evidence.assistantText,
      previousMessageIds,
      options.signal,
    );
    return { narration: evidence.assistantText, verified: true };
  } finally {
    options.signal?.removeEventListener("abort", onAbort);
  }
}

async function runApprovalContinuation(
  continuation: Extract<VoiceContinuation, { kind: "approval" }>,
  options: RunVoiceContinuationOptions,
): Promise<VoiceContinuationResult> {
  if (options.signal?.aborted) throw abortError();
  const events: AgentEvent[] = [];
  const submit = continuation.decision === "approve" ? approveAction : rejectAction;
  await submit(
    continuation.approval_id,
    continuation.conversation_id,
    (event) => {
      events.push(event);
      options.onEvent?.(event);
    },
    options.signal,
  );
  if (options.signal?.aborted) throw abortError();
  return reduceApprovalEvents(continuation.decision, events);
}

export async function runVoiceContinuation(
  continuation: VoiceContinuation,
  options: RunVoiceContinuationOptions = {},
): Promise<VoiceContinuationResult> {
  return continuation.kind === "conversation"
    ? runConversationContinuation(continuation, options)
    : runApprovalContinuation(continuation, options);
}
