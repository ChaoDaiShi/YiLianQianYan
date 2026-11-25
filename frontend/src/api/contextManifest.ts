import { requestResult } from "./transport";
export type ContextEntryStatus = "resolved" | "truncated" | "unavailable" | "unsupported" | "excluded";
export interface ContextManifestEntry { kind: string; source_id: string; version?: string | null; status: ContextEntryStatus; chars: number; reason?: string | null }
export interface ContextManifest { schema_version: number; entries: ContextManifestEntry[]; total_chars: number }
export interface ConversationTurn { id: string; user_message_id: string; status: string; started_at: number; finished_at: number | null }
export interface ConversationContextSnapshot {
  manifest: ContextManifest;
  turn_id?: string | null;
  timeline?: { request_received_at: number; memory_resolved_at: number; memory_sources: ContextManifestEntry[] };
}
export async function getLatestConversationTurn(id: string) {
  const result = await requestResult<{ turns: ConversationTurn[] }>("GET", `/api/conversations/${encodeURIComponent(id)}/turns`);
  if (!result.ok) throw new Error(result.error);
  return result.data.turns[0] ?? null;
}
export async function getConversationContextManifest(id: string): Promise<ConversationContextSnapshot> {
  const result = await requestResult<ConversationContextSnapshot>("GET", `/api/conversations/${encodeURIComponent(id)}/context-manifest`);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function getWorkflowContextManifest(id: string): Promise<{ owner: { kind: string; workflow_graph_id: string; run_id: string }; manifest: ContextManifest }> {
  const result = await requestResult<{ owner: { kind: string; workflow_graph_id: string; run_id: string }; manifest: ContextManifest }>("GET", `/api/workflow-runs/${encodeURIComponent(id)}/context-manifest`);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
