// Chat — SSE streaming turn dispatch and cancellation
//
// Transport is `core/api/http.ts`; this module owns these calls rather than
// borrowing them from the legacy bundle.
import { API_BASE } from "../core/api/http";
import { request, requestResult } from "../core/api/http";
import { controlSessionHeaders } from "./controlSession";
import type { ChatRequestOptions } from "../components/chat/chatRequestOptions";
import type { WorkspaceMode } from "../types";

// ============================================================
// SSE Streaming (for chat)
// ============================================================

export interface AgentEvent {
  type: string;
  conversation_id: string;
  user_message_id?: string;
  error_code?: string;
  affected_user_message_id?: string | null;
  token?: string;
  tool_call_id?: string;
  tool_name?: string;
  args?: Record<string, unknown>;
  result?: string;
  status?: string;
  error?: string;
  message_id?: string;
  risk_level?: string;
  reason?: string;
  phase?: "selected"|"switched"|"recovered"|"exhausted";
  pool_id?: string;
  pool_revision?: number;
  request_id?: string;
  attempt_id?: string;
  from_profile_id?: string|null;
  to_profile_id?: string|null;
  from_tier?: number|null;
  to_tier?: number|null;
  interrupted_content?: string|null;
  replace_content?: boolean;
  approval_id?: string;
  verification_success?: boolean;
  verification_reason?: string;
  should_replan?: boolean;
  /** Frontend transport metadata; HTTP admission failed before an SSE run existed. */
  request_rejected?: boolean;
  mode?: WorkspaceMode;
  automatic?: boolean;
}

export type EventHandler = (event: AgentEvent) => void;

export interface ConversationInput {
  question_id: string; run_id: string; conversation_id: string; tool_call_id: string;
  title: string; kind: "text" | "single_choice";
  options: Array<{ id: string; label: string; description: string }>;
  allow_text: boolean; status: "pending" | "answered" | "cancelled"; revision: number;
  answer: string | null; created_at: number; resolved_at: number | null;
}
export interface UserInputAnswer { revision: number; option_id?: string; text?: string }
export async function listConversationInputs(id: string): Promise<ConversationInput[]> {
  const result = await requestResult<ConversationInput[]>("GET", `/api/conversations/${encodeURIComponent(id)}/inputs`);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
export function answerConversationInput(questionId: string, answer: UserInputAnswer, onEvent: EventHandler, conversationId: string): AbortController {
  return streamChatRequest(`/api/conversation-inputs/${encodeURIComponent(questionId)}/answer`, answer, onEvent, conversationId);
}

export function sendMessage(
  message: string,
  conversationId: string | null,
  onEvent: EventHandler,
  workflowId?: string | null,
  options?: ChatRequestOptions,
): AbortController {
  return streamChatRequest("/api/chat", {conversation_id:conversationId,message,workflow_id:workflowId||undefined,...options},onEvent,conversationId);
}

export function streamChatRequest(path:string,body:unknown,onEvent:EventHandler,conversationId:string|null=null):AbortController {
  let resolvedConversationId=conversationId??"";
  const controller = new AbortController();

  fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...controlSessionHeaders(),
    },
    body: JSON.stringify(body),
    signal: controller.signal,
  })
    .then(async (res) => {
      if (!res.ok) {
        let detail = "";
        let errorCode: string | undefined;
        let affectedMessageId: string | null | undefined;
        try {
          const body = (await res.clone().json()) as {
            error?: string;
            message?: string;
            code?: string;
            affected_user_message_id?: string | null;
          };
          detail = body.error || body.message || "";
          errorCode = body.code;
          affectedMessageId = body.affected_user_message_id;
        } catch {
          /* non-JSON error body */
        }
        onEvent({
          type: "error",
          conversation_id: conversationId || "",
          error: detail || `HTTP ${res.status}`,
          request_rejected: true,
          error_code: errorCode,
          affected_user_message_id: affectedMessageId,
        });
        onEvent({type:"stream_end",conversation_id:resolvedConversationId});
        return;
      }

      const reader = res.body?.getReader();
      if (!reader) return;

      const decoder = new TextDecoder();
      let buffer = "";
      let currentEvent = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() || "";

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed) {
            currentEvent = "";
            continue;
          }

          if (trimmed.startsWith("event:")) {
            currentEvent = trimmed.slice(6).trim();
            continue;
          }

          if (trimmed.startsWith("data:")) {
            const data = trimmed.slice(5).trim();
            if (data === "ping" || data === "[DONE]") continue;

            try {
              const parsed = JSON.parse(data);

              if (currentEvent === "connected") {
                resolvedConversationId=parsed.conversation_id||resolvedConversationId;
                onEvent({
                  type: "connected",
                  conversation_id: parsed.conversation_id || "",
                  user_message_id: parsed.user_message_id,
                });
              } else if (parsed.type) {
                onEvent(parsed as AgentEvent);
              }
            } catch {
              console.warn("SSE parse error for:", data);
            }
          }
        }
      }

      // Stream ended without an explicit terminal event (e.g. agent paused
      // for approval). Let the handler clear transient loading state.
      onEvent({ type: "stream_end", conversation_id: resolvedConversationId });
    })
    .catch((err) => {
      if (err.name !== "AbortError") {
        onEvent({
          type: "error",
          conversation_id: conversationId || "",
          error: String(err),
        });
      }
      onEvent({type:"stream_end",conversation_id:resolvedConversationId});
    });

  return controller;
}

export async function stopGeneration(conversationId: string) {
  return request<any>("POST", "/api/chat/stop", { conversation_id: conversationId });
}
