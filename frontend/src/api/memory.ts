// Memory records — retrieval, extraction, batch import/export and reindex
//
// Transport is `core/api/http.ts`; this module owns these calls rather than
// borrowing them from the legacy bundle.
import { request } from "../core/api/http";

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

export async function listMemories(query?: MemoryQuery) {
  const params = new URLSearchParams();
  if (query?.category) params.set("category", query.category);
  if (query?.source) params.set("source", query.source);
  if (query?.q) params.set("q", query.q);
  if (query?.limit) params.set("limit", String(query.limit));
  if (query?.offset) params.set("offset", String(query.offset));
  const qs = params.toString();
  return request<MemoryRecord[]>("GET", `/api/memories${qs ? `?${qs}` : ""}`);
}

export async function getMemory(id: string) {
  return request<MemoryRecord>("GET", `/api/memories/${id}`);
}

export async function createMemory(data: {
  content: string;
  category?: string;
  source?: string;
  metadata?: string;
}) {
  return request<MemoryRecord>("POST", "/api/memories", data);
}

export async function updateMemory(
  id: string,
  data: { content?: string; category?: string; metadata?: string }
) {
  return request<MemoryRecord>("PUT", `/api/memories/${id}`, data);
}

export async function deleteMemory(id: string) {
  return request<{ status: string }>("DELETE", `/api/memories/${id}`);
}

export async function getMemoryStats() {
  return request<MemoryStats>("GET", "/api/memories/stats");
}

export async function extractMemories(conversationId?: string) {
  return request<{ status: string; message: string }>("POST", "/api/memories/extract", {
    conversation_id: conversationId,
  });
}

export async function batchImportMemories(items: Array<{ content: string; category?: string }>) {
  return request<any>("POST", "/api/memories/batch-import", { items });
}

export async function batchDeleteMemories(ids: string[]) {
  return request<any>("POST", "/api/memories/batch-delete", { ids });
}

export async function exportMemories() {
  return request<any>("GET", "/api/memories/export");
}

export async function mergeMemories(ids: string[]) {
  return request<any>("POST", "/api/memories/merge", { ids });
}

export async function reindexMemories() {
  return request<MemoryReindexResult>("POST", "/api/memories/reindex", { limit: 50 });
}

export async function retrieveMemories(q: string, topK = 10, category?: string) {
  const params = new URLSearchParams({ q, top_k: String(topK) });
  if (category) params.set("category", category);
  return request<MemoryRetrieveResponse>("GET", `/api/memories/retrieve?${params.toString()}`);
}
