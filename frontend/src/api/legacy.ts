// ============================================================
// Legacy API implementation — frozen behind domain entrypoints and client facade
//
// The HTTP core (`API_BASE`, `request`, `requestResult`, `ApiResult`) now lives
// in `src/core/api/http.ts` and is re-exported below, so every existing import
// keeps working. Draining the rest of this file into the domain entrypoints is
// the remaining work.
// ============================================================

import { API_BASE, request } from "../core/api/http";
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

export { listPlugins, createMcpServer, updateMcpServer, deleteMcpServer, toggleMcpServer, testMcpServer, listSubagents, listSkills, loadSkill, createSkill, updateSkill, deleteSkill } from "./plugins";
export type { McpServer, PluginListResponse, SubagentMetadata, SkillSummary, SkillDetail } from "./plugins";















// ── System ──

export async function getSystemInfo() {
  return request<any>("GET", "/api/system");
}

export { MEMORY_RETRIEVAL_MODE_LABELS, listMemories, getMemory, createMemory, updateMemory, deleteMemory, getMemoryStats, extractMemories, batchImportMemories, batchDeleteMemories, exportMemories, mergeMemories, reindexMemories, retrieveMemories } from "./memory";
export type { MemoryRecord, MemoryStats, MemoryReindexResult, MemoryRetrievalMode, ScoredMemory, MemoryRetrieveResponse, MemoryQuery } from "./memory";































































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




















































export { listTasks, createTask, getTask, startTask, retryTask, cancelTask, listTaskExecutions, listTaskTimeline, listTaskArtifacts, listTaskDecisions, resolveTaskDecision, listAgents, createAgent, listTeams, createTeam } from "./tasks";
export type { Task, TaskStatus, TaskPriority, TaskExecution, TaskExecutionStatus, TaskEvent, Artifact, ArtifactType, TaskDecision, TaskDecisionOption, AgentDefinition, AgentTeam } from "./tasks";

























// ── Workspaces ── (implementation in `./workspaces`)
export {
  listWorkspaces,
  createWorkspace,
  getWorkspace,
  updateWorkspace,
  deleteWorkspace,
} from "./workspaces";
export type { Workspace, WorkspaceStatus } from "./workspaces";































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
