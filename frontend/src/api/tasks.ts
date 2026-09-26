// Tasks, executions and the agent/team definitions they run against
//
// Transport is `core/api/http.ts`; this module owns these calls rather than
// borrowing them from the legacy bundle.
import { requestResult } from "../core/api/http";

// ============================================================
// Workspace / Task / Multi-Agent Runtime (v0.6)
// ============================================================

export type TaskStatus =
  | "draft"
  | "ready"
  | "running"
  | "waiting_approval"
  | "waiting_user"
  | "blocked"
  | "completed"
  | "failed"
  | "cancelled";

export type TaskPriority = "low" | "normal" | "high";

export interface Task {
  id: string;
  workspace_id: string;
  title: string;
  description: string;
  status: TaskStatus;
  priority: TaskPriority;
  workflow_graph_id?: string | null;
  agent_team_id?: string | null;
  created_at: number;
  updated_at: number;
  completed_at?: number | null;
}

export type TaskExecutionStatus =
  | "created"
  | "running"
  | "waiting_approval"
  | "waiting_user"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted";

export interface TaskExecution {
  id: string;
  task_id: string;
  execution_id: string;
  subject_id: string;
  workflow_run_id?: string | null;
  status: TaskExecutionStatus;
  attempt: number;
  started_at?: number | null;
  finished_at?: number | null;
  error?: string | null;
  created_at: number;
  updated_at: number;
}

export interface TaskEvent {
  id: string;
  event_type: string;
  message: string;
  metadata: Record<string, unknown>;
  created_at: number;
}

export type ArtifactType =
  | "file"
  | "document"
  | "code"
  | "report"
  | "image"
  | "data"
  | "text"
  | "other";

export interface Artifact {
  id: string;
  workspace_id: string;
  task_id: string;
  task_execution_id: string;
  name: string;
  artifact_type: ArtifactType;
  path?: string | null;
  mime_type?: string | null;
  size?: number | null;
  summary: string;
  created_at: number;
  updated_at: number;
}

export interface TaskDecisionOption {
  id: string;
  label: string;
  description: string;
}

export interface TaskDecision {
  id: string;
  task_id: string;
  task_execution_id: string;
  prompt: string;
  options: TaskDecisionOption[];
  status: "pending" | "resolved";
  created_at: number;
}

export interface AgentDefinition {
  id: string;
  name: string;
  description: string;
  instructions: string;
  allowed_tools: string[];
  model?: string | null;
  capabilities: string[];
  max_iterations: number;
  enabled: boolean;
  source: "builtin" | "local_file" | "database";
}

export interface AgentTeam {
  id: string;
  name: string;
  description: string;
  coordinator_agent_id: string;
  member_agent_ids: string[];
  max_depth: number;
  max_agent_executions: number;
  max_total_iterations: number;
  created_at: number;
  updated_at: number;
}

// ── Tasks ──

export async function listTasks(query: {
  workspace_id?: string;
  status?: TaskStatus;
  limit?: number;
  offset?: number;
} = {}) {
  const params = new URLSearchParams();
  if (query.workspace_id) params.set("workspace_id", query.workspace_id);
  if (query.status) params.set("status", query.status);
  if (query.limit !== undefined) params.set("limit", String(query.limit));
  if (query.offset !== undefined) params.set("offset", String(query.offset));
  const qs = params.toString();
  const res = await requestResult<{ tasks: Task[] }>(
    "GET",
    `/api/tasks${qs ? `?${qs}` : ""}`
  );
  return res.ok ? ({ ok: true, data: res.data.tasks } as const) : res;
}

export async function createTask(data: {
  workspace_id: string;
  title: string;
  description?: string;
  priority?: TaskPriority;
  workflow_graph_id?: string;
  agent_team_id?: string;
}) {
  return requestResult<Task>("POST", "/api/tasks", data);
}

export async function getTask(id: string) {
  return requestResult<Task>("GET", `/api/tasks/${id}`);
}

export async function startTask(id: string) {
  return requestResult<{
    task_id: string;
    task_execution_id: string;
    execution_id: string;
    status: string;
  }>("POST", `/api/tasks/${id}/start`);
}

export async function retryTask(id: string) {
  return requestResult<{
    task_id: string;
    task_execution_id: string;
    execution_id: string;
    status: string;
  }>("POST", `/api/tasks/${id}/retry`);
}

export async function cancelTask(id: string) {
  return requestResult<{ status: string }>("POST", `/api/tasks/${id}/cancel`);
}

export async function listTaskExecutions(taskId: string) {
  const res = await requestResult<{ executions: TaskExecution[] }>(
    "GET",
    `/api/tasks/${taskId}/executions`
  );
  return res.ok ? ({ ok: true, data: res.data.executions } as const) : res;
}

export async function listTaskTimeline(taskId: string) {
  const res = await requestResult<{ events: TaskEvent[] }>(
    "GET",
    `/api/tasks/${taskId}/timeline`
  );
  return res.ok ? ({ ok: true, data: res.data.events } as const) : res;
}

export async function listTaskArtifacts(taskId: string) {
  const res = await requestResult<{ artifacts: Artifact[] }>(
    "GET",
    `/api/tasks/${taskId}/artifacts`
  );
  return res.ok ? ({ ok: true, data: res.data.artifacts } as const) : res;
}

export async function listTaskDecisions(taskId: string) {
  const res = await requestResult<{ decisions: TaskDecision[] }>(
    "GET",
    `/api/task-decisions?task_id=${encodeURIComponent(taskId)}`
  );
  return res.ok ? ({ ok: true, data: res.data.decisions } as const) : res;
}

export async function resolveTaskDecision(decisionId: string, optionId: string) {
  return requestResult<{ decision_id: string; option_id: string; status: string }>(
    "POST",
    `/api/task-decisions/${decisionId}/resolve`,
    { option_id: optionId }
  );
}

// ── Agents & Teams ──

export async function listAgents() {
  const res = await requestResult<{ agents: AgentDefinition[] }>("GET", "/api/agents");
  return res.ok ? ({ ok: true, data: res.data.agents } as const) : res;
}

export async function createAgent(data: {
  name: string;
  description?: string;
  instructions?: string;
  allowed_tools?: string[];
  model?: string;
  capabilities?: string[];
  max_iterations?: number;
  enabled?: boolean;
}) {
  return requestResult<AgentDefinition>("POST", "/api/agents", data);
}

export async function listTeams() {
  const res = await requestResult<{ teams: AgentTeam[] }>("GET", "/api/agent-teams");
  return res.ok ? ({ ok: true, data: res.data.teams } as const) : res;
}

export async function createTeam(data: {
  name: string;
  description?: string;
  coordinator_agent_id: string;
  member_agent_ids?: string[];
  max_depth?: number;
  max_agent_executions?: number;
  max_total_iterations?: number;
}) {
  return requestResult<AgentTeam>("POST", "/api/agent-teams", data);
}
