// Conversation list and lifecycle. Transport is `core/api/http.ts`, so this
// module owns its four calls rather than borrowing them from `legacy`.
//
// The create / load / delete returns are the untrusted payload shapes the
// backend sends; a caller that needs a typed row re-reads through
// `listConversations`.
import { request } from "../core/api/http";
import type { ConversationSummary } from "../types";

export async function listConversations() {
  return request<ConversationSummary[]>("GET", "/api/conversations");
}

export async function createConversation(title?: string) {
  return request<any>("POST", "/api/conversations", { title });
}

export async function loadConversation(id: string) {
  return request<any>("GET", `/api/conversations/${id}`);
}

export async function deleteConversation(id: string) {
  return request<any>("DELETE", `/api/conversations/${id}`);
}
