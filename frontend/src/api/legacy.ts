// ============================================================
// Legacy API implementation — frozen behind domain entrypoints and client facade
//
// The HTTP core (`API_BASE`, `request`, `requestResult`, `ApiResult`) now lives
// in `src/core/api/http.ts` and is re-exported below, so every existing import
// keeps working. Draining the rest of this file into the domain entrypoints is
// the remaining work.
// ============================================================

import { API_BASE, request, requestResult } from "../core/api/http";
import type {
  AppConfig,
  LlmModel,
  LlmModelPayload,
  LlmUsageReport,
} from "../types";

export { API_BASE, request, requestResult } from "../core/api/http";
export type { ApiFailure, ApiResult } from "../core/api/http";

// ── Conversations ── (implementation in `./conversations`)
export { listConversations, createConversation, loadConversation, deleteConversation } from "./conversations";

// ── Settings ──

export async function getSettings() {
  return request<AppConfig>("GET", "/api/settings");
}

export async function updateSettings(config: AppConfig) {
  return request<{ status?: string; error?: string }>("PUT", "/api/settings", config);
}

export async function listLlmModels() {
  return request<LlmModel[]>("GET", "/api/llm/models");
}

export async function createLlmModel(payload: LlmModelPayload) {
  return request<LlmModel>("POST", "/api/llm/models", payload);
}

export async function updateLlmModel(id: string, payload: LlmModelPayload) {
  return request<LlmModel>("PUT", `/api/llm/models/${encodeURIComponent(id)}`, payload);
}

export async function deleteLlmModel(id: string) {
  return request<Record<string, never>>("DELETE", `/api/llm/models/${encodeURIComponent(id)}`);
}

export async function verifyLlmModel(id: string) {
  return request<{ model: LlmModel }>("POST", `/api/llm/models/${encodeURIComponent(id)}/verify`);
}

export async function activateLlmModel(id: string) {
  return request<LlmModel>("POST", `/api/llm/models/${encodeURIComponent(id)}/activate`);
}

export async function getLlmUsage(query: string) {
  return request<LlmUsageReport>("GET", `/api/llm/usage${query}`);
}

// ── Tools ──

export async function listTools() {
  return request<any[]>("GET", "/api/tools");
}

// ── Skills ──

export interface SkillSummary {
  name: string;
  description: string;
  path: string;
  editable: boolean;
}

export interface SkillDetail extends SkillSummary {
  content: string;
  root_dir: string;
}

export async function listSkills() {
  return requestResult<SkillSummary[]>("GET", "/api/skills");
}

export async function loadSkill(name: string) {
  return requestResult<SkillDetail>("GET", `/api/skills/${encodeURIComponent(name)}`);
}

export async function createSkill(data: { name: string; content: string }) {
  return requestResult<SkillSummary>("POST", "/api/skills", data);
}

export async function updateSkill(
  currentName: string,
  data: { name: string; content: string }
) {
  return requestResult<SkillSummary>(
    "PUT",
    `/api/skills/${encodeURIComponent(currentName)}`,
    data
  );
}

export async function deleteSkill(name: string) {
  return requestResult<{ status: string }>(
    "DELETE",
    `/api/skills/${encodeURIComponent(name)}`
  );
}

// ── System ──

export async function getSystemInfo() {
  return request<any>("GET", "/api/system");
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

// ── Plugins / MCP ──

export interface McpServer {
  id: string;
  name: string;
  transport: string;
  command?: string | null;
  args?: string[];
  url?: string | null;
  env?: Record<string, string>;
  enabled: boolean;
  created_at: number;
  updated_at: number;
  runtime_status?: string;
  protocol_version?: string | null;
  tools_count?: number;
  resources_count?: number;
  resource_templates_count?: number;
  prompts_count?: number;
  last_refresh?: number | null;
  safe_error?: string | null;
}

export interface PluginListResponse {
  builtin: Array<{ name: string; description: string; parameters: Record<string, unknown> }>;
  mcp: McpServer[];
  mcp_runtime_ready: boolean;
}

export async function listPlugins() {
  return request<PluginListResponse>("GET", "/api/plugins");
}

export async function createMcpServer(data: Partial<McpServer>) {
  return requestResult<McpServer>("POST", "/api/plugins/mcp", data);
}

export async function updateMcpServer(id: string, data: Partial<McpServer>) {
  return requestResult<McpServer>("PUT", `/api/plugins/mcp/${id}`, data);
}

export async function deleteMcpServer(id: string) {
  return requestResult<{ status: string }>("DELETE", `/api/plugins/mcp/${id}`);
}

export async function toggleMcpServer(id: string) {
  return requestResult<McpServer>("POST", `/api/plugins/mcp/${id}/toggle`);
}

export async function testMcpServer(id: string) {
  return requestResult<{ ok: boolean; message: string }>("POST", `/api/plugins/mcp/${id}/test`);
}

export interface SubagentMetadata {
  name: string;
  description: string;
  allowed_tools: string[];
  model?: string | null;
  workdir?: string | null;
  runtime_ready: boolean;
}

export async function listSubagents() {
  return request<SubagentMetadata[]>("GET", "/api/subagents");
}

// ── Security grants / isolation ──

export interface SecurityGrant {
  id: string;
  subject_id: string;
  effect: string;
  permission: string;
  resource: Record<string, unknown>;
  source: string;
  created_at: number;
  expires_at?: number | null;
}

export interface IsolationStatus {
  backend: string;
  process_containment: boolean;
  restricted_token: boolean;
  privilege_reduction: boolean;
  restricting_sids: boolean;
  job_object: boolean;
  kill_tree: boolean;
  filesystem_os_enforced: boolean;
  network_os_enforced: boolean;
  experimental_appcontainer_available: boolean;
}

export async function listSecurityGrants() {
  const res = await request<{ grants: SecurityGrant[] }>("GET", "/api/security/grants");
  return res?.grants ?? null;
}

export async function createSecurityGrant(data: {
  permission_id: string;
  effect: string;
  resource: Record<string, unknown>;
}) {
  return request<SecurityGrant>("POST", "/api/security/grants", data);
}

export async function deleteSecurityGrant(id: string) {
  return request<{ status: string }>("DELETE", `/api/security/grants/${id}`);
}

export async function getIsolationStatus() {
  return request<IsolationStatus>("GET", "/api/security/isolation/status");
}

export { listWorkflows, getWorkflow, createWorkflow, updateWorkflow, deleteWorkflow, activateWorkflow, listWorkflowGraphs, getWorkflowGraph, createWorkflowGraph, updateWorkflowGraph, deleteWorkflowGraph, startWorkflowRun, getWorkflowRun, listWorkflowRuns, cancelWorkflowRun, streamWorkflowApprovalDecision, approveWorkflowApproval, rejectWorkflowApproval, cancelWorkflowApprovalAction } from "./workflows";
export type { Workflow, WorkflowNodeKind, WorkflowCondition, WorkflowNodeConfig, WorkflowNodeDefinition, WorkflowEdgeDefinition, WorkflowGraphDefinition, WorkflowGraphRecord, WorkflowRunStatus, WorkflowNodeRunStatus, WorkflowNodeRun, WorkflowRunRecord, WorkflowApprovalEvent, WorkflowRunsQuery } from "./workflows";















// ── Health ──

export interface RuntimeHealth {
  status: "healthy" | "degraded";
  service: string;
  version: string;
  database: "healthy" | "unavailable";
  policy_version: string;
}

export async function healthCheck(): Promise<RuntimeHealth | null> {
  try {
    const response = await fetch(`${API_BASE}/api/health`, {
      method: "GET",
      headers: { Accept: "application/json" },
    });
    if (!response.ok) {
      console.error(`API GET /api/health: ${response.status}`);
      return null;
    }
    return (await response.json()) as RuntimeHealth;
  } catch (error) {
    console.error("API GET /api/health failed:", error);
    return null;
  }
}

export async function isServerAvailable(): Promise<boolean> {
  const result = await healthCheck();
  return result !== null;
}

export { sendMessage, stopGeneration } from "./chat";
export type { AgentEvent, EventHandler } from "./chat";









// ── Logs ──

export interface LogEntry {
  timestamp: number;
  level: string;
  source: string;
  message: string;
}

export interface LogsResponse {
  entries: LogEntry[];
  total: number;
}

export async function getLogs(params?: {
  count?: number;
  level?: string;
  source?: string;
  drain?: boolean;
}): Promise<LogsResponse | null> {
  const qs = new URLSearchParams();
  if (params?.count) qs.set("count", String(params.count));
  if (params?.level) qs.set("level", params.level);
  if (params?.source) qs.set("source", params.source);
  if (params?.drain) qs.set("drain", "true");
  const query = qs.toString();
  return request<LogsResponse>("GET", `/api/logs${query ? `?${query}` : ""}`);
}

export async function pushLog(level: string, source: string, message: string) {
  return request<any>("POST", "/api/logs", { level, source, message });
}




















































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

// ── Workspaces ── (implementation in `./workspaces`)
export {
  listWorkspaces,
  createWorkspace,
  getWorkspace,
  updateWorkspace,
  deleteWorkspace,
} from "./workspaces";
export type { Workspace, WorkspaceStatus } from "./workspaces";

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

// ============================================================
// ── Capabilities ── (implementation in `./capabilities`)
export { listCapabilities, getCapability, refreshCapabilities } from "./capabilities";
export type {
  CapabilityDescriptor,
  CapabilityKind,
  CapabilityMetadata,
  CapabilityPermission,
  CapabilityProviderKind,
  CapabilityRefreshReport,
  CapabilityRisk,
  CapabilityRuntimeStatus,
} from "./capabilities";
