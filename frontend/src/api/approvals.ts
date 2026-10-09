import { API_BASE, type AgentEvent } from "./client";
import { controlSessionHeaders } from "./controlSession";
import type { PendingApproval } from "../types/approval";

interface PendingResponse {
  approvals?: PendingApproval[];
  approval?: PendingApproval;
}

async function requestJSON<T>(path: string, body?: unknown): Promise<T | null> {
  try {
    const response = await fetch(`${API_BASE}${path}`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        ...controlSessionHeaders(),
      },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    if (!response.ok) return null;
    return (await response.json()) as T;
  } catch (error) {
    console.error(`Approval API ${path} failed:`, error);
    return null;
  }
}

async function streamDecision(
  path: string,
  body: { conversation_id?: string | null; attestation_id?: string },
  onEvent: (event: AgentEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const response = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...controlSessionHeaders(),
    },
    body: JSON.stringify(body),
    signal,
  });

  if (!response.ok) {
    const detail = await response.text().catch(() => "");
    throw new Error(
      `审批请求失败（HTTP ${response.status}）${detail ? `：${detail}` : ""}`
    );
  }
  if (!response.body) {
    throw new Error("审批请求未返回可读取的事件流");
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let currentEvent = "";
  let terminalReceived = false;

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
      if (!trimmed.startsWith("data:")) continue;

      const data = trimmed.slice(5).trim();
      if (data === "ping" || data === "[DONE]") continue;

      try {
        const parsed = JSON.parse(data) as AgentEvent;
        const event =
          currentEvent === "connected"
            ? {
                type: "connected",
                conversation_id: parsed.conversation_id || "",
              }
            : parsed;
        if (!event.type) continue;
        terminalReceived =
          terminalReceived || event.type === "done" || event.type === "error";
        onEvent(event);
      } catch {
        console.warn("SSE parse error for:", data);
      }
    }
  }

  if (!terminalReceived) {
    onEvent({
      type: "stream_end",
      conversation_id: body.conversation_id || "",
    });
  }
}

export function approveAction(
  approvalId: string,
  conversationId: string | null,
  onEvent: (event: AgentEvent) => void,
  signal?: AbortSignal,
  attestationId?: string,
): Promise<void> {
  return streamDecision(
    `/api/approvals/${approvalId}/approve`,
    { conversation_id: conversationId, attestation_id: attestationId },
    onEvent,
    signal,
  );
}

export function rejectAction(
  approvalId: string,
  conversationId: string | null,
  onEvent: (event: AgentEvent) => void,
  signal?: AbortSignal,
  attestationId?: string,
): Promise<void> {
  return streamDecision(
    `/api/approvals/${approvalId}/reject`,
    { conversation_id: conversationId, attestation_id: attestationId },
    onEvent,
    signal,
  );
}

export async function cancelApproval(
  approvalId: string,
  conversationId?: string | null
) {
  return requestJSON<PendingResponse>(`/api/approvals/${approvalId}/cancel`, {
    conversation_id: conversationId,
  });
}

export async function getApproval(approvalId: string) {
  try {
    const response = await fetch(`${API_BASE}/api/approvals/${approvalId}`, {
      headers: controlSessionHeaders(),
    });
    if (!response.ok) {
      if (response.status === 404) return null;
      throw new Error(`审批状态查询失败（HTTP ${response.status}）`);
    }
    const data = (await response.json()) as PendingResponse;
    return data.approval || null;
  } catch (error) {
    console.error("getApproval failed:", error);
    throw error;
  }
}

export async function listPendingApprovals() {
  try {
    const response = await fetch(`${API_BASE}/api/approvals/pending`, {
      headers: controlSessionHeaders(),
    });
    if (!response.ok) return null;
    const data = (await response.json()) as PendingResponse;
    return data.approvals || [];
  } catch (error) {
    console.error("listPendingApprovals failed:", error);
    return null;
  }
}
