// Conversation list and lifecycle. Transport is `core/api/http.ts`, so this
// module owns its four calls rather than borrowing them from `legacy`.
import { requestResult } from "../core/api/http";
import type { Conversation, ConversationSummary, WorkspaceMode } from "../types";

export async function listConversations(): Promise<ConversationSummary[]> {
  const result = await requestResult<ConversationSummary[]>("GET", "/api/conversations");
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function createConversation(title?: string): Promise<ConversationSummary> {
  const result = await requestResult<ConversationSummary>("POST", "/api/conversations", { title });
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function loadConversation(id: string): Promise<Conversation> {
  const result = await requestResult<Conversation>("GET", `/api/conversations/${id}`);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
export async function setConversationWorkspaceMode(id: string, mode: WorkspaceMode): Promise<{ mode: WorkspaceMode }> {
  const result = await requestResult<{ mode: WorkspaceMode }>("PUT", `/api/conversations/${encodeURIComponent(id)}/mode`, { mode });
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function deleteConversation(id: string): Promise<{ status: string }> {
  const result = await requestResult<{ status: string }>("DELETE", `/api/conversations/${id}`);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
