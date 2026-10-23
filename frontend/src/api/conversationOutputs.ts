import { API_BASE } from "./transport";
import { controlSessionHeaders } from "./controlSession";

export interface ConversationOutput {
  id: string; conversation_id: string; kind: "image" | "code"; title: string;
  source_tool: "image_generation" | "assistant_message"; profile_id: string | null;
  status: "awaiting_download" | "ready" | "download_failed";
  resource_id: string | null; preview_url: string | null; download_url: string | null;
  can_download: boolean;
  error: string | null; created_at: number; updated_at: number;
  source_message_id: string | null; block_index: number | null;
  version: number; previous_output_id: string | null;
}
export interface CodeCandidate { message_id: string; block_index: number; language: string; preview: string; created_at: number }
async function request<T>(conversation: string, suffix: string, signal?: AbortSignal, body?: unknown): Promise<T> {
  const response = await fetch(`${API_BASE}/api/conversations/${encodeURIComponent(conversation)}/outputs${suffix}`, {
    method: body === undefined ? "GET" : "POST", headers: { "Content-Type": "application/json", ...controlSessionHeaders() },
    body: body === undefined ? undefined : JSON.stringify(body), signal,
  });
  const value = await response.json();
  if (!response.ok) throw new Error(typeof value.error === "string" ? value.error : "会话成果请求失败");
  return value as T;
}
export async function listConversationOutputs(conversation: string, signal?: AbortSignal): Promise<ConversationOutput[]> {
  return (await request<{ outputs: ConversationOutput[] }>(conversation, "", signal)).outputs;
}
export async function getConversationOutput(conversation: string, id: string, signal?: AbortSignal): Promise<ConversationOutput> {
  return (await request<{ output: ConversationOutput }>(conversation, `/${encodeURIComponent(id)}`, signal)).output;
}
export async function listCodeCandidates(conversation: string, signal?: AbortSignal): Promise<CodeCandidate[]> {
  return (await request<{ candidates: CodeCandidate[] }>(conversation, "/code-candidates", signal)).candidates;
}
export async function saveCodeOutput(conversation: string, candidate: CodeCandidate, previousOutputId?: string): Promise<ConversationOutput> {
  return (await request<{ output: ConversationOutput }>(conversation, "/from-message", undefined, {
    message_id: candidate.message_id, block_index: candidate.block_index, previous_output_id: previousOutputId || null,
  })).output;
}
