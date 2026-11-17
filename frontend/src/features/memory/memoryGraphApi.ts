import type { MemoryRecord, MemoryVersion } from "../../api/memory";
import { requestResult } from "../../core/api/http";
import type { MemoryRelationship } from "./relationship";

export type LinkStatus = "confirmed" | "pending" | "withdrawn";
export interface MemoryLink {
  id: string; source_memory_id: string; target_memory_id: string; label: string;
  status: LinkStatus; revision: number; created_at: number; updated_at: number;
}
export interface MemoryLinkVersion {
  relation_id: string; revision: number; label: string; status: LinkStatus; recorded_at: number;
  source_version_id: string; target_version_id: string;
}
export interface MemoryGraphQuery {
  q?: string; category?: string; source?: string; withdrawn?: boolean;
  workspace_id?: string; conversation_id?: string;
}
export interface MemorySimilarity {
  source_memory_id: string; target_memory_id: string; score: number;
}
export interface MemoryGraphSemantic {
  configured: boolean; indexed_memory_ids: string[]; similarities: MemorySimilarity[];
}
export interface MemoryChangeEvent {
  id: string;
  memory_id: string;
  version_id: string;
  previous_version_id?: string | null;
  recorded_at: number;
  kind: string;
  related_event_id?: string | null;
  related_memory_id?: string | null;
  related_version_id?: string | null;
  relation_id?: string | null;
}
export interface MemoryGraphSnapshot {
  memories: MemoryRecord[];
  histories: Record<string, MemoryVersion[]>;
  current_version_ids?: Record<string, string>;
  change_events?: MemoryChangeEvent[];
  relations: MemoryLink[];
  relation_versions: MemoryLinkVersion[];
  next_offset: number | null;
  semantic?: MemoryGraphSemantic;
  extracted_relations?: {id:string;source_memory_id:string;source_memory_revision:number;relationship:MemoryRelationship;recorded_at?:number;evidence_start:number;evidence_end:number}[];
}

export async function extractMemoryRelationships(id: string, relationId?: string) {
  const result = await requestResult<{candidate_ids:string[]}>("POST", "/api/memories/extract", {memory_id:id,mode:"relationships", relation_id:relationId});
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function readMemoryGraph(query: MemoryGraphQuery, limit = 100): Promise<MemoryGraphSnapshot> {
  const params = new URLSearchParams({ limit: String(limit) });
  for (const key of ["q", "category", "source", "workspace_id", "conversation_id"] as const) if (query[key]) params.set(key, query[key]);
  if (query.withdrawn) params.set("withdrawn", "true");
  const result = await requestResult<MemoryGraphSnapshot>("GET", `/api/memories/graph?${params}`);
  if (!result.ok) throw Object.assign(new Error(result.error), { status: result.status });
  return result.data;
}

export async function createMemoryLink(data: Pick<MemoryLink, "source_memory_id" | "target_memory_id" | "label" | "status">) {
  const result = await requestResult<MemoryLink>("POST", "/api/memories/relations", data);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function changeMemoryLink(id: string, data: { label: string; status: LinkStatus; expected_revision: number }) {
  const result = await requestResult<MemoryLink>("PATCH", `/api/memories/relations/${encodeURIComponent(id)}`, data);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function stageLinkedCorrection(id: string, data: { target_memory_id: string; content: string; reason: string; expected_revision: number; source_version_id?: string }) {
  const result = await requestResult<{ candidate_id: string | null }>("POST", `/api/memories/relations/${encodeURIComponent(id)}/correction-candidate`, data);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
