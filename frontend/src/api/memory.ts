// Memory records — retrieval, extraction, batch import/export and reindex
//
// Transport is `core/api/http.ts`; this module owns these calls rather than
// borrowing them from the legacy bundle.
import { requestResult } from "../core/api/http";

export interface MemoryGovernance {
  memory_id: string;
  revision: number;
  status: "active" | "withdrawn";
  scope: string;
  source_ref: string | null;
  reason: string;
  content_hash: string;
  embedding_revision: number | null;
  embedding_model: string | null;
  embedding_dimension: number | null;
  embedding_generation: number | null;
  valid_from: number | null;
  valid_until: number | null;
}

export interface MemoryVersion {
  revision: number; content: string; category: string; source: string;
  source_ref: string | null; scope: string; status: string; reason: string;
  recorded_at: number; valid_from: number | null; valid_until: number | null;
}

export async function getMemoryGovernance(id: string): Promise<MemoryGovernance> {
  const result = await requestResult<MemoryGovernance>("GET", `/api/memories/${encodeURIComponent(id)}/governance`);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function getMemoryHistory(id: string): Promise<MemoryVersion[]> {
  const result = await requestResult<MemoryVersion[]>("GET", `/api/memories/${encodeURIComponent(id)}/history`);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function correctMemory(id: string, data: { content: string; expected_revision: number; reason: string; scope?: string; source_ref?: string }): Promise<MemoryRecord> {
  const result = await requestResult<MemoryRecord>("POST", `/api/memories/${encodeURIComponent(id)}/correct`, data);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function withdrawMemory(id: string, data: { expected_revision: number; reason: string }): Promise<MemoryGovernance> {
  const result = await requestResult<MemoryGovernance>("POST", `/api/memories/${encodeURIComponent(id)}/withdraw`, data);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function listWithdrawnMemories(): Promise<MemoryRecord[]> {
  const result = await requestResult<MemoryRecord[]>("GET", "/api/memories/withdrawn");
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

// ── Memories ──

export interface MemoryRecord {
  id: string;
  content: string;
  category: string;
  source: string;
  source_conversation_id?: string;
  metadata?: string;
  created_at: number;
  updated_at: number;
}

export interface MemoryStats {
  total: number;
  by_category: Array<[string, number]>;
  by_source: Array<[string, number]>;
}

export interface MemoryReindexResult {
  ok: boolean;
  requested?: number;
  processed?: number;
  succeeded?: number;
  failed?: number;
  remaining?: number;
  error?: string;
}

export type MemoryRetrievalMode = "lexical" | "hybrid" | "lexical_fallback";

export const MEMORY_RETRIEVAL_MODE_LABELS: Record<MemoryRetrievalMode, string> = {
  lexical: "词法检索",
  hybrid: "混合检索",
  lexical_fallback: "Embedding 失败，已回退词法检索",
};

export interface ScoredMemory extends MemoryRecord {
  score: number;
  lexical_score: number;
  vector_score: number;
}

export interface MemoryRetrieveResponse {
  query: string;
  count: number;
  mode: MemoryRetrievalMode;
  memories: ScoredMemory[];
  ok?: boolean;
  error?: string;
}

export interface MemoryQuery {
  category?: string;
  source?: string;
  q?: string;
  limit?: number;
  offset?: number;
}

export async function listMemories(query?: MemoryQuery): Promise<MemoryRecord[]> {
  const params = new URLSearchParams();
  if (query?.category) params.set("category", query.category);
  if (query?.source) params.set("source", query.source);
  if (query?.q) params.set("q", query.q);
  if (query?.limit) params.set("limit", String(query.limit));
  if (query?.offset) params.set("offset", String(query.offset));
  const qs = params.toString();
  const result = await requestResult<MemoryRecord[]>("GET", `/api/memories${qs ? `?${qs}` : ""}`);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function getMemory(id: string): Promise<MemoryRecord> {
  const result = await requestResult<MemoryRecord>("GET", `/api/memories/${id}`);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function createMemory(data: {
  content: string;
  category?: string;
  source?: string;
  metadata?: string;
}): Promise<MemoryRecord> {
  const result = await requestResult<MemoryRecord>("POST", "/api/memories", data);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function updateMemory(
  id: string,
  data: { content?: string; category?: string; metadata?: string }
): Promise<MemoryRecord> {
  const result = await requestResult<MemoryRecord>("PUT", `/api/memories/${id}`, data);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function deleteMemory(id: string): Promise<{ status: string }> {
  const result = await requestResult<{ status: string }>("DELETE", `/api/memories/${id}`);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function getMemoryStats(): Promise<MemoryStats> {
  const result = await requestResult<MemoryStats>("GET", "/api/memories/stats");
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function extractMemories(conversationId?: string): Promise<{
  ok: boolean;
  created: number;
  skipped: number;
  memories: MemoryRecord[];
}> {
  const result = await requestResult<{
    ok: boolean;
    created: number;
    skipped: number;
    memories: MemoryRecord[];
  }>("POST", "/api/memories/extract", {
    conversation_id: conversationId,
  });
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function batchImportMemories(items: Array<{ content: string; category?: string }>) {
  const result = await requestResult<any>("POST", "/api/memories/batch-import", { items });
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function batchDeleteMemories(ids: string[]) {
  const result = await requestResult<any>("POST", "/api/memories/batch-delete", { ids });
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function exportMemories() {
  const result = await requestResult<any>("GET", "/api/memories/export");
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function mergeMemories(ids: string[]) {
  const result = await requestResult<any>("POST", "/api/memories/merge", { ids });
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function reindexMemories(): Promise<MemoryReindexResult> {
  const result = await requestResult<MemoryReindexResult>("POST", "/api/memories/reindex", { limit: 50 });
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function retrieveMemories(q: string, topK = 10, category?: string): Promise<MemoryRetrieveResponse> {
  const params = new URLSearchParams({ q, top_k: String(topK) });
  if (category) params.set("category", category);
  const result = await requestResult<MemoryRetrieveResponse>("GET", `/api/memories/retrieve?${params.toString()}`);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
