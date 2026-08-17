// ============================================================
// API Client — fetch-based HTTP + SSE for backend communication
// ============================================================

import { controlSessionHeaders } from "./controlSession";
import type { AppConfig } from "../types";

export const API_BASE =
  (import.meta.env.VITE_API_BASE as string | undefined)?.replace(/\/$/, "") ||
  "http://127.0.0.1:9420";

export async function request<T>(
  method: string,
  path: string,
  body?: unknown
): Promise<T | null> {
  try {
    const opts: RequestInit = {
      method,
      headers: {
        "Content-Type": "application/json",
        ...controlSessionHeaders(),
      },
    };
    if (body !== undefined) {
      opts.body = JSON.stringify(body);
    }
    const res = await fetch(`${API_BASE}${path}`, opts);
    if (!res.ok) {
      console.error(`API ${method} ${path}: ${res.status}`);
      return null;
    }
    if (res.status === 204) return {} as T;
    const text = await res.text();
    if (!text) return {} as T;
    return JSON.parse(text) as T;
  } catch (e) {
    console.error(`API ${method} ${path} failed:`, e);
    return null;
  }
}

// ── Conversations ──

export async function listConversations() {
  return request<any[]>("GET", "/api/conversations");
}

export async function createConversation(title?: string) {
  return request<any>("POST", "/api/conversations", { title });
}

export async function loadConversation(id: string) {
  return request<any>("GET", `/api/conversations/${id}`);
}

export async function deleteConversation(id: string) {
  return request<any>("DELETE", `/api/conversations/${id}`);
}

// ── Settings ──

export async function getSettings() {
  return request<AppConfig>("GET", "/api/settings");
}

export async function updateSettings(config: AppConfig) {
  return request<{ status?: string; error?: string }>("PUT", "/api/settings", config);
}

// ── Tools ──

export async function listTools() {
  return request<any[]>("GET", "/api/tools");
}

// ── Skills ──

export async function listSkills() {
  return request<any[]>("GET", "/api/skills");
}

export async function loadSkill(name: string) {
  return request<any>("GET", `/api/skills/${encodeURIComponent(name)}`);
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
  return request<McpServer>("POST", "/api/plugins/mcp", data);
}

export async function updateMcpServer(id: string, data: Partial<McpServer>) {
  return request<McpServer>("PUT", `/api/plugins/mcp/${id}`, data);
}

export async function deleteMcpServer(id: string) {
  return request<{ status: string }>("DELETE", `/api/plugins/mcp/${id}`);
}

export async function toggleMcpServer(id: string) {
  return request<McpServer>("POST", `/api/plugins/mcp/${id}/toggle`);
}

export async function testMcpServer(id: string) {
  return request<{ ok: boolean; message: string }>("POST", `/api/plugins/mcp/${id}/test`);
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

// ============================================================
// SSE Streaming (for chat)
// ============================================================

export interface AgentEvent {
  type: string;
  conversation_id: string;
  token?: string;
  tool_call_id?: string;
  tool_name?: string;
  args?: Record<string, unknown>;
  result?: string;
  status?: string;
  error?: string;
  message_id?: string;
  risk_level?: string;
  reason?: string;
  approval_id?: string;
  verification_success?: boolean;
  verification_reason?: string;
  should_replan?: boolean;
}

export type EventHandler = (event: AgentEvent) => void;

export function sendMessage(
  message: string,
  conversationId: string | null,
  onEvent: EventHandler,
  workflowId?: string | null
): AbortController {
  const controller = new AbortController();

  fetch(`${API_BASE}/api/chat`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...controlSessionHeaders(),
    },
    body: JSON.stringify({
      conversation_id: conversationId,
      message,
      workflow_id: workflowId || undefined,
    }),
    signal: controller.signal,
  })
    .then(async (res) => {
      if (!res.ok) {
        onEvent({
          type: "error",
          conversation_id: conversationId || "",
          error: `HTTP ${res.status}`,
        });
        return;
      }

      const reader = res.body?.getReader();
      if (!reader) return;

      const decoder = new TextDecoder();
      let buffer = "";
      let currentEvent = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() || "";

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed) {
            currentEvent = "";
            continue;
          }

          if (trimmed.startsWith("event:")) {
            currentEvent = trimmed.slice(6).trim();
            continue;
          }

          if (trimmed.startsWith("data:")) {
            const data = trimmed.slice(5).trim();
            if (data === "ping" || data === "[DONE]") continue;

            try {
              const parsed = JSON.parse(data);

              if (currentEvent === "connected") {
                onEvent({
                  type: "connected",
                  conversation_id: parsed.conversation_id || "",
                });
              } else if (parsed.type) {
                onEvent(parsed as AgentEvent);
              }
            } catch {
              console.warn("SSE parse error for:", data);
            }
          }
        }
      }

      // Stream ended without an explicit terminal event (e.g. agent paused
      // for approval). Let the handler clear transient loading state.
      onEvent({ type: "stream_end", conversation_id: conversationId || "" });
    })
    .catch((err) => {
      if (err.name !== "AbortError") {
        onEvent({
          type: "error",
          conversation_id: conversationId || "",
          error: String(err),
        });
      }
    });

  return controller;
}

export async function stopGeneration(conversationId: string) {
  return request<any>("POST", "/api/chat/stop", { conversation_id: conversationId });
}

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

export type ApiResult<T> =
  | { ok: true; data: T }
  | { ok: false; status: number; error: string };

/** Like `request`, but surfaces backend validation errors to the caller. */
async function requestResult<T>(
  method: string,
  path: string,
  body?: unknown
): Promise<ApiResult<T>> {
  try {
    const opts: RequestInit = {
      method,
      headers: {
        "Content-Type": "application/json",
        ...controlSessionHeaders(),
      },
    };
    if (body !== undefined) opts.body = JSON.stringify(body);
    const res = await fetch(`${API_BASE}${path}`, opts);
    const text = await res.text();
    if (!res.ok) {
      let error = `HTTP ${res.status}`;
      try {
        const parsed = JSON.parse(text) as { error?: string; message?: string };
        if (parsed && typeof parsed === "object") {
          const msg = parsed.error || parsed.message;
          if (msg) error = String(msg);
        }
      } catch {
        /* non-JSON error body */
      }
      return { ok: false, status: res.status, error };
    }
    if (!text) return { ok: true, data: {} as T };
    return { ok: true, data: JSON.parse(text) as T };
  } catch (e) {
    return { ok: false, status: 0, error: String(e) };
  }
}

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

// ============================================================
// Workspace / Task / Multi-Agent Runtime (v0.6)
// ============================================================

export type WorkspaceStatus = "active" | "archived";

export interface Workspace {
  id: string;
  name: string;
  description: string;
  root_path?: string | null;
  status: WorkspaceStatus;
  created_at: number;
  updated_at: number;
  active_tasks?: number;
}

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

// ── Workspaces ──

export async function listWorkspaces() {
  const res = await requestResult<{ workspaces: Workspace[] }>("GET", "/api/workspaces");
  return res.ok ? ({ ok: true, data: res.data.workspaces } as const) : res;
}

export async function createWorkspace(data: {
  name: string;
  description?: string;
  root_path?: string;
}) {
  return requestResult<Workspace>("POST", "/api/workspaces", data);
}

export async function getWorkspace(id: string) {
  return requestResult<Workspace>("GET", `/api/workspaces/${id}`);
}

export async function updateWorkspace(
  id: string,
  data: { name?: string; description?: string; root_path?: string }
) {
  return requestResult<Workspace>("PUT", `/api/workspaces/${id}`, data);
}

export async function deleteWorkspace(id: string) {
  return requestResult<{ status: string }>("DELETE", `/api/workspaces/${id}`);
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

// ============================================================
// Unified Capability Registry (v0.8 Phase 3)
// ============================================================

export type CapabilityKind =
  | "tool"
  | "mcp_tool"
  | "subagent"
  | "agent"
  | "workflow"
  | "skill";

export type CapabilityProviderKind =
  | "builtin"
  | "mcp"
  | "subagent"
  | "agent_runtime"
  | "workflow_runtime"
  | "skill_runtime";

export type CapabilityRuntimeStatus =
  | "ready"
  | "unavailable"
  | "disabled"
  | "misconfigured"
  | "degraded"
  | "unknown";

export type CapabilityRisk = "low" | "medium" | "high" | "dynamic";

export interface CapabilityPermission {
  permission: string;
  required: boolean;
}

export interface CapabilityMetadata {
  source_id?: string | null;
  source_name?: string | null;
  version?: string | null;
  tags: string[];
  runtime_ready: boolean;
  extra: Record<string, unknown>;
}

export interface CapabilityDescriptor {
  id: string;
  kind: CapabilityKind;
  provider: CapabilityProviderKind;
  name: string;
  description: string;
  input_schema?: Record<string, unknown> | null;
  risk: CapabilityRisk;
  permissions: CapabilityPermission[];
  status: CapabilityRuntimeStatus;
  enabled: boolean;
  metadata: CapabilityMetadata;
}

export interface CapabilityRefreshReport {
  discovered: number;
  ready: number;
  unavailable: number;
  duplicates: number;
  provider_failures: number;
}

export async function listCapabilities(query: {
  kind?: CapabilityKind;
  provider?: CapabilityProviderKind;
  status?: CapabilityRuntimeStatus;
  q?: string;
  limit?: number;
  offset?: number;
} = {}) {
  const params = new URLSearchParams();
  if (query.kind) params.set("kind", query.kind);
  if (query.provider) params.set("provider", query.provider);
  if (query.status) params.set("status", query.status);
  if (query.q) params.set("q", query.q);
  if (query.limit !== undefined) params.set("limit", String(query.limit));
  if (query.offset !== undefined) params.set("offset", String(query.offset));
  const qs = params.toString();
  const res = await requestResult<{ capabilities: CapabilityDescriptor[]; total: number }>(
    "GET",
    `/api/capabilities${qs ? `?${qs}` : ""}`
  );
  return res.ok ? ({ ok: true, data: res.data } as const) : res;
}

export async function getCapability(id: string) {
  return requestResult<CapabilityDescriptor>("GET", `/api/capabilities/${id}`);
}

export async function refreshCapabilities() {
  return requestResult<CapabilityRefreshReport>("POST", "/api/capabilities/refresh");
}
