// Workflows — prompt-template CRUD, graph definition, and run lifecycle
//
// Transport is `core/api/http.ts`; this module owns these calls rather than
// borrowing them from the legacy bundle.
import { API_BASE } from "../core/api/http";
import { request } from "../core/api/http";
import { requestResult } from "../core/api/http";
import type { ApiResult } from "../core/api/http";
import { controlSessionHeaders } from "./controlSession";

// ── Workflows ──

export interface Workflow {
  id: string;
  name: string;
  description: string;
  nodes: string[];
  tags: string[];
  system_prompt_extra?: string;
  is_builtin: boolean;
  created_at: number;
  updated_at: number;
}

export async function listWorkflows() {
  return request<{ workflows: Workflow[]; active_id: string | null }>("GET", "/api/workflows");
}

export async function getWorkflow(id: string) {
  return request<Workflow>("GET", `/api/workflows/${id}`);
}

export async function createWorkflow(data: Partial<Workflow>) {
  return request<Workflow>("POST", "/api/workflows", data);
}

export async function updateWorkflow(id: string, data: Partial<Workflow>) {
  return request<Workflow>("PUT", `/api/workflows/${id}`, data);
}

export async function deleteWorkflow(id: string) {
  return request<{ status: string }>("DELETE", `/api/workflows/${id}`);
}

export async function activateWorkflow(id: string) {
  return request<{ active_id: string }>("POST", `/api/workflows/${id}/activate`);
}

// ============================================================
// Workflow Runtime — executable DAG types + API
// ============================================================

export type WorkflowNodeKind = "agent" | "tool" | "subagent" | "condition" | "output";

export type WorkflowCondition = "always" | "previous_succeeded";

export type WorkflowNodeConfig =
  | { type: "agent"; prompt: string }
  | { type: "tool"; tool_name: string; arguments: Record<string, unknown> }
  | { type: "subagent"; subagent_name: string; task: string }
  | { type: "condition"; when: WorkflowCondition }
  | { type: "output"; template?: string | null };

export interface WorkflowNodeDefinition {
  id: string;
  kind: WorkflowNodeKind;
  config: WorkflowNodeConfig;
}

export interface WorkflowEdgeDefinition {
  from: string;
  to: string;
}

export interface WorkflowGraphDefinition {
  schema_version: number;
  entry_node_id: string;
  nodes: WorkflowNodeDefinition[];
  edges: WorkflowEdgeDefinition[];
}

export interface WorkflowGraphRecord {
  id: string;
  name: string;
  description: string;
  definition: WorkflowGraphDefinition;
  created_at: number;
  updated_at: number;
}

export type WorkflowRunStatus =
  | "created"
  | "running"
  | "waiting_approval"
  | "completed"
  | "failed"
  | "cancelled";

export type WorkflowNodeRunStatus =
  | "pending"
  | "ready"
  | "running"
  | "waiting_approval"
  | "completed"
  | "failed"
  | "skipped"
  | "cancelled";

export interface WorkflowNodeRun {
  node_id: string;
  status: WorkflowNodeRunStatus;
  started_at?: number | null;
  finished_at?: number | null;
  error?: string | null;
  result?: { summary: string } | null;
}

export interface WorkflowRunRecord {
  run_id: string;
  workflow_graph_id?: string;
  execution_id: string;
  subject_id: string;
  status: WorkflowRunStatus;
  created_at: number;
  updated_at: number;
  nodes: WorkflowNodeRun[];
}

export type WorkflowApprovalEvent =
  | {
      type: "approval_resolved";
      approval_id: string;
      status: "approved" | "rejected";
    }
  | {
      type: "workflow_run_updated";
      workflow_run_id: string;
      workflow_node_id?: string | null;
      status: WorkflowRunStatus;
    }
  | {
      type: "workflow_approval_error";
      approval_id: string;
      workflow_run_id?: string | null;
      workflow_node_id?: string | null;
      error: string;
    };

export async function listWorkflowGraphs(): Promise<ApiResult<WorkflowGraphRecord[]>> {
  const res = await requestResult<{ graphs: WorkflowGraphRecord[] }>(
    "GET",
    "/api/workflow-graphs"
  );
  return res.ok ? { ok: true, data: res.data.graphs } : res;
}

export async function getWorkflowGraph(id: string) {
  return requestResult<WorkflowGraphRecord>("GET", `/api/workflow-graphs/${id}`);
}

export async function createWorkflowGraph(data: {
  name?: string;
  description?: string;
  definition: WorkflowGraphDefinition;
}) {
  return requestResult<WorkflowGraphRecord>("POST", "/api/workflow-graphs", data);
}

export async function updateWorkflowGraph(
  id: string,
  data: {
    name?: string;
    description?: string;
    definition?: WorkflowGraphDefinition;
  }
) {
  return requestResult<WorkflowGraphRecord>("PUT", `/api/workflow-graphs/${id}`, data);
}

export async function deleteWorkflowGraph(id: string) {
  return requestResult<{ status: string }>("DELETE", `/api/workflow-graphs/${id}`);
}

export async function startWorkflowRun(graphId: string) {
  return requestResult<{ run_id: string; execution_id: string; status: string }>(
    "POST",
    `/api/workflow-graphs/${graphId}/run`
  );
}

export async function getWorkflowRun(runId: string) {
  return requestResult<WorkflowRunRecord>("GET", `/api/workflow-runs/${runId}`);
}

export interface WorkflowRunsQuery {
  workflow_graph_id?: string;
  status?: WorkflowRunStatus;
  limit?: number;
  offset?: number;
}

export async function listWorkflowRuns(query: WorkflowRunsQuery = {}) {
  const params = new URLSearchParams();
  if (query.workflow_graph_id) params.set("workflow_graph_id", query.workflow_graph_id);
  if (query.status) params.set("status", query.status);
  if (query.limit !== undefined) params.set("limit", String(query.limit));
  if (query.offset !== undefined) params.set("offset", String(query.offset));
  const qs = params.toString();
  const res = await requestResult<{ runs: WorkflowRunRecord[] }>(
    "GET",
    `/api/workflow-runs${qs ? `?${qs}` : ""}`
  );
  return res.ok ? { ok: true, data: res.data.runs } : res;
}

export async function cancelWorkflowRun(runId: string) {
  return requestResult<WorkflowRunRecord>("POST", `/api/workflow-runs/${runId}/cancel`);
}

/** Stream a workflow approval decision (approve/reject) SSE response. */
export function streamWorkflowApprovalDecision(
  path: string,
  onEvent: (event: WorkflowApprovalEvent) => void
): Promise<void> {
  return fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...controlSessionHeaders(),
    },
    body: JSON.stringify({}),
  })
    .then(async (res) => {
      if (!res.ok) {
        const detail = await res.text().catch(() => "");
        onEvent({
          type: "workflow_approval_error",
          approval_id: "",
          error: detail || `HTTP ${res.status}`,
        });
        return;
      }
      if (!res.body) return;
      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() || "";
        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed) continue;
          if (trimmed.startsWith("data:")) {
            const data = trimmed.slice(5).trim();
            if (data === "ping" || data === "[DONE]") continue;
            try {
              const parsed = JSON.parse(data) as WorkflowApprovalEvent;
              if (parsed && parsed.type) onEvent(parsed);
            } catch {
              /* skip unparseable frames */
            }
          }
        }
      }
    })
    .catch((err) => {
      onEvent({ type: "workflow_approval_error", approval_id: "", error: String(err) });
    });
}

export function approveWorkflowApproval(
  approvalId: string,
  onEvent: (event: WorkflowApprovalEvent) => void
) {
  return streamWorkflowApprovalDecision(`/api/approvals/${approvalId}/approve`, onEvent);
}

export function rejectWorkflowApproval(
  approvalId: string,
  onEvent: (event: WorkflowApprovalEvent) => void
) {
  return streamWorkflowApprovalDecision(`/api/approvals/${approvalId}/reject`, onEvent);
}

export async function cancelWorkflowApprovalAction(approvalId: string) {
  return requestResult<{ ok: boolean; error?: string }>(
    "POST",
    `/api/approvals/${approvalId}/cancel`,
    {}
  );
}
