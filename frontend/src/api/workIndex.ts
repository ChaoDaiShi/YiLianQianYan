import { requestResult } from "./transport";

export type WorkKind = "conversation" | "legacy_task" | "task_graph" | "personal_delegation";
export interface WorkIndexItem {
  id: string;
  kind: WorkKind;
  title: string;
  workspace_id: string | null;
  status: string;
  attention_required: boolean;
  href: string;
  updated_at: number;
  source_ids: { conversation_id?: string; automation_id?: string; graph_id?: string; task_id?: string };
}
export interface WorkIndex { items: WorkIndexItem[]; limit: number; truncated: boolean; projection_issues?: { kind: string; id: string; error: string; attempts: number }[] }
export async function listWorkIndex(workspaceId?: string, limit = 200): Promise<WorkIndex> {
  const query = new URLSearchParams({ limit: String(limit) });
  if (workspaceId) query.set("workspace_id", workspaceId);
  const result = await requestResult<WorkIndex>("GET", `/api/work-index?${query}`);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function reconcileWorkIndex(): Promise<void> {
  const result = await requestResult<unknown>("POST", "/api/work-index/reconcile", {});
  if (!result.ok) throw new Error(result.error);
}
