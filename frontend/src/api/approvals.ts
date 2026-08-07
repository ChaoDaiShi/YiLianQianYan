// ============================================================
// Approval API client — approve/reject/cancel + SSE resume stream
// ============================================================

import { API_BASE, type AgentEvent } from "./client";
import type { PendingApproval } from "../types/approval";

interface PendingResponse {
  ok?: boolean;
  approvals?: PendingApproval[];
  approval?: PendingApproval;
  status?: string;
  error?: string;
}

async function requestJSON<T>(path: string, body?: unknown): Promise<T | null> {
  try {
    const res = await fetch(`${API_BASE}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    if (!res.ok) return null;
    return (await res.json()) as T;
  } catch (e) {
    console.error(`Approval API ${path} failed:`, e);
    return null;
  }
}

/**
 * POST the decision to the backend and consume the resume SSE stream.
 * The backend streams approval_resolved / tool_start / tool_end / token / done.
 */
function streamDecision(
  path: string,
  body: { conversation_id?: string | null },
  onEvent: (event: AgentEvent) => void
): AbortController {
  const controller = new AbortController();

  fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal: controller.signal,
  })
    .then(async (res) => {
      if (!res.ok) {
        const text = await res.text().catch(() => "");
        onEvent({ type: "error", conversation_id: "", error: `HTTP ${res.status}: ${text}` });
        return;
      }
      const reader = res.body?.getReader();
      if (!reader) return;

      const decoder = new TextDecoder();
      let buffer = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() || "";
        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed) continue;
          if (trimmed.startsWith("event:")) continue;
          if (trimmed.startsWith("data:")) {
            const data = trimmed.slice(5).trim();
            if (data === "ping" || data === "[DONE]") continue;
            try {
              const parsed = JSON.parse(data);
              if (parsed.type) onEvent(parsed as AgentEvent);
            } catch {
              console.warn("SSE parse error for:", data);
            }
          }
        }
      }
      // Stream ended — let the caller clear any transient loading state.
      onEvent({ type: "stream_end", conversation_id: body.conversation_id || "" });
    })
    .catch((err) => {
      if (err.name !== "AbortError") {
        onEvent({ type: "error", conversation_id: "", error: String(err) });
      }
    });

  return controller;
}

export function approveAction(
  approvalId: string,
  conversationId: string | null,
  onEvent: (event: AgentEvent) => void
): AbortController {
  return streamDecision(`/api/approvals/${approvalId}/approve`, { conversation_id: conversationId }, onEvent);
}

export function rejectAction(
  approvalId: string,
  conversationId: string | null,
  onEvent: (event: AgentEvent) => void
): AbortController {
  return streamDecision(`/api/approvals/${approvalId}/reject`, { conversation_id: conversationId }, onEvent);
}

export async function cancelApproval(approvalId: string, conversationId?: string | null) {
  return requestJSON<PendingResponse>(`/api/approvals/${approvalId}/cancel`, { conversation_id: conversationId });
}

export async function getApproval(approvalId: string) {
  try {
    const res = await fetch(`${API_BASE}/api/approvals/${approvalId}`);
    if (!res.ok) return null;
    const data = (await res.json()) as PendingResponse;
    return data.approval || null;
  } catch (e) {
    console.error("getApproval failed:", e);
    return null;
  }
}

export async function listPendingApprovals() {
  try {
    const res = await fetch(`${API_BASE}/api/approvals/pending`);
    if (!res.ok) return null;
    const data = (await res.json()) as PendingResponse;
    return data.approvals || [];
  } catch (e) {
    console.error("listPendingApprovals failed:", e);
    return null;
  }
}
