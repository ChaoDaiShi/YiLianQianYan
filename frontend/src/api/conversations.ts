// Conversation list and lifecycle. Transport is `core/api/http.ts`, so this
// module owns its four calls rather than borrowing them from `legacy`.
import { requestResult } from "../core/api/http";
import type { Conversation, ConversationSummary, WorkspaceMode } from "../types";

export interface MessageMutationResult {
  conversation: Conversation;
  deleted_message_ids: string[];
  removed_protocol_count: number;
}
export interface MessageRollbackResult extends MessageMutationResult {
  draft: { text: string; resources: Array<{ id: string; name: string }>; resource_scope: "conversation" };
}
export class MessageMutationError extends Error {
  readonly code?: string;
  readonly affectedUserMessageId?: string;
  constructor(message: string, details?: Record<string, unknown>) {
    super(message);
    this.name = "MessageMutationError";
    this.code = typeof details?.code === "string" ? details.code : undefined;
    this.affectedUserMessageId = typeof details?.affected_user_message_id === "string" ? details.affected_user_message_id : undefined;
  }
}
export async function rollbackConversationMessage(conversationId: string, messageId: string): Promise<MessageRollbackResult> {
  const result = await requestResult<MessageRollbackResult>("POST", `/api/conversations/${encodeURIComponent(conversationId)}/messages/${encodeURIComponent(messageId)}/rollback`, {}, true);
  if (!result.ok) throw new MessageMutationError(result.error, result.details);
  return result.data;
}
export async function deleteConversationMessage(conversationId: string, messageId: string): Promise<MessageMutationResult> {
  const result = await requestResult<MessageMutationResult>("DELETE", `/api/conversations/${encodeURIComponent(conversationId)}/messages/${encodeURIComponent(messageId)}`, undefined, true);
  if (!result.ok) throw new MessageMutationError(result.error, result.details);
  return result.data;
}

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
