import { requestResult } from "../core/api/http";
import type { KnowledgeDocument } from "./knowledge";

export type KnowledgeBuildMode = "quick" | "deep";
export type KnowledgeBuildStatus = "queued" | "researching" | "synthesizing" | "completed" | "failed" | "cancelled" | "interrupted";
export interface KnowledgeBuildSource {
  id: string; title: string; url: string; query: string; excerpt: string;
  evidence_kind: "content" | "snippet" | "title_only";
  source_type: string; source_type_reason: string;
  source_priority: "preferred" | "supporting" | "unclassified";
}
export interface KnowledgeBuildNode {
  id: string; parent_id: string | null; title: string; summary: string; source_ids: string[];
}
export interface KnowledgeBuildResult {
  summary: string;
  nodes: KnowledgeBuildNode[];
  edges: { from: string; to: string; relation: "prerequisite" | "related" }[];
  claims: {
    id: string; text: string;
    status: "corroborated" | "single_source" | "contradicted" | "unverified";
    evidence: { source_id: string; quote: string; stance: "supports" | "contradicts" }[];
  }[];
  contradictions: { claim_id: string; description: string }[];
  limitations: string[];
}
export interface KnowledgeBuild {
  id: string; topic: string; mode: KnowledgeBuildMode; status: KnowledgeBuildStatus;
  progress: { stage: string; queries_completed: number; queries_total: number; sources_collected: number; rounds_completed: number; rounds_total: number };
  sources: KnowledgeBuildSource[]; result: KnowledgeBuildResult | null;
  error: string | null; document_id: string | null; created_at: number; updated_at: number;
}
async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const response = await requestResult<T>(method, path, body);
  if (!response.ok) throw new Error(response.error);
  return response.data;
}
const buildPath = (id: string) => `/api/knowledge/builds/${encodeURIComponent(id)}`;
export const listKnowledgeBuilds = () => call<{ builds: KnowledgeBuild[] }>("GET", "/api/knowledge/builds");
export const createKnowledgeBuild = (topic: string, mode: KnowledgeBuildMode) => call<{ build: KnowledgeBuild }>("POST", "/api/knowledge/builds", { topic, mode });
export const getKnowledgeBuild = (id: string) => call<{ build: KnowledgeBuild }>("GET", buildPath(id));
export const cancelKnowledgeBuild = (id: string) => call<{ build: KnowledgeBuild }>("POST", `${buildPath(id)}/cancel`);
export const importKnowledgeBuild = (id: string) => call<{ build: KnowledgeBuild; document: KnowledgeDocument; created: boolean }>("POST", `${buildPath(id)}/import`);
