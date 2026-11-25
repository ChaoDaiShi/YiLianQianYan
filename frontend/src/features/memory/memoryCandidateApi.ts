import { requestResult } from "../../core/api/http";
import type { MemoryRecord } from "../../api/memory";
import type { MemoryRelationship } from "./relationship";

export interface MemoryCandidate {
  id: string;
  category: string;
  content: string;
  created_at: number;
  source_kind: "user_message" | "task_reflection" | "proofloop_acceptance" | "related_memory_correction" | "memory_version";
  source_memory_id?: string | null;
  source_memory_revision?: number | null;
  source_memory_scope?: string | null;
  source_title: string | null;
  source_excerpt: string | null;
  source_conversation_id: string | null;
  source_message_id: string | null;
  source_task_id: string | null;
  source_execution_id: string | null;
  source_graph_id: string | null;
  source_report_id: string | null;
  source_agent: string | null;
  workspace_id: string | null;
  source_removed: boolean;
  relationship?: MemoryRelationship | null;
  correction?: {
    relation_id: string;
    label: string;
    target_memory_id: string;
    target_revision: number;
    source_version_id?: string | null;
    reason: string;
  } | null;
}

export type CandidateScope = "conversation" | "workspace" | "profile";
export interface ReviewedMemory { content: string; category: string; scope: CandidateScope; relationship?: MemoryRelationship }

const path = "/api/memories/candidates";

export async function stageCompanionFeedback(activityId: string) {
  const result = await requestResult<{ candidate_ids: string[] }>("POST", `${path}/feedback`, { activity_id: activityId });
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function listMemoryCandidates(offset = 0) {
  const result = await requestResult<{ entries: MemoryCandidate[]; next_offset: number | null }>("GET", `${path}?offset=${offset}`);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function acceptMemoryCandidate(id: string, review: ReviewedMemory) {
  const result = await requestResult<MemoryRecord>("POST", `${path}/${encodeURIComponent(id)}/accept`, review);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function dismissMemoryCandidate(id: string) {
  const result = await requestResult<{ status: string }>("POST", `${path}/${encodeURIComponent(id)}/dismiss`);
  if (!result.ok) throw new Error(result.error);
}
