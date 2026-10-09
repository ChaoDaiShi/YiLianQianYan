// Chat — SSE streaming turn dispatch and cancellation
//
// Transport is `core/api/http.ts`; this module owns these calls rather than
// borrowing them from the legacy bundle.
import { API_BASE } from "../core/api/http";
import { request } from "../core/api/http";
import { controlSessionHeaders } from "./controlSession";

// ============================================================
// SSE Streaming (for chat)
// ============================================================

export interface AgentEvent {
  type: string;
  conversation_id: string;
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
  approval_id?: string;
  verification_success?: boolean;
  verification_reason?: string;
  should_replan?: boolean;
}

export type EventHandler = (event: AgentEvent) => void;

export function sendMessage(
  message: string,
  conversationId: string | null,
  onEvent: EventHandler,
  workflowId?: string | null
): AbortController {
  const controller = new AbortController();

  fetch(`${API_BASE}/api/chat`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...controlSessionHeaders(),
    },
    body: JSON.stringify({
      conversation_id: conversationId,
      message,
      workflow_id: workflowId || undefined,
    }),
    signal: controller.signal,
  })
    .then(async (res) => {
      if (!res.ok) {
        let detail = "";
        try {
          const body = (await res.clone().json()) as {
            error?: string;
            message?: string;
          };
          detail = body.error || body.message || "";
        } catch {
          /* non-JSON error body */
        }
        onEvent({
          type: "error",
          conversation_id: conversationId || "",
          error: detail || `HTTP ${res.status}`,
        });
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
                onEvent({
                  type: "connected",
                  conversation_id: parsed.conversation_id || "",
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
      onEvent({ type: "stream_end", conversation_id: conversationId || "" });
    })
    .catch((err) => {
      if (err.name !== "AbortError") {
        onEvent({
          type: "error",
          conversation_id: conversationId || "",
          error: String(err),
        });
      }
    });

  return controller;
}

export async function stopGeneration(conversationId: string) {
  return request<any>("POST", "/api/chat/stop", { conversation_id: conversationId });
}
