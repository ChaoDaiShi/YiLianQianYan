// ============================================================
// Legacy API implementation — frozen behind domain entrypoints and client facade
//
// The HTTP core (`API_BASE`, `request`, `requestResult`, `ApiResult`) now lives
// in `src/core/api/http.ts` and is re-exported below, so every existing import
// keeps working. Draining the rest of this file into the domain entrypoints is
// the remaining work.
// ============================================================

export { API_BASE, request, requestResult } from "../core/api/http";
export type { ApiFailure, ApiResult } from "../core/api/http";

// ── Conversations ── (implementation in `./conversations`)
export { listConversations, createConversation, loadConversation, deleteConversation } from "./conversations";

export { getSettings, updateSettings, listLlmModels, createLlmModel, updateLlmModel, deleteLlmModel, verifyLlmModel, activateLlmModel, getLlmUsage, listTools, getSystemInfo, healthCheck, isServerAvailable, getLogs, pushLog, listSecurityGrants, createSecurityGrant, deleteSecurityGrant, getIsolationStatus } from "./system";
export type { RuntimeHealth, LogEntry, LogsResponse, SecurityGrant, IsolationStatus } from "./system";





















export { listPlugins, createMcpServer, updateMcpServer, deleteMcpServer, toggleMcpServer, testMcpServer, listSubagents, listSkills, loadSkill, createSkill, updateSkill, deleteSkill } from "./plugins";
export type { McpServer, PluginListResponse, SubagentMetadata, SkillSummary, SkillDetail } from "./plugins";

















export { MEMORY_RETRIEVAL_MODE_LABELS, listMemories, getMemory, createMemory, updateMemory, deleteMemory, getMemoryStats, extractMemories, batchImportMemories, batchDeleteMemories, exportMemories, mergeMemories, reindexMemories, retrieveMemories } from "./memory";
export type { MemoryRecord, MemoryStats, MemoryReindexResult, MemoryRetrievalMode, ScoredMemory, MemoryRetrieveResponse, MemoryQuery } from "./memory";











































































export { listWorkflows, getWorkflow, createWorkflow, updateWorkflow, deleteWorkflow, activateWorkflow, listWorkflowGraphs, getWorkflowGraph, createWorkflowGraph, updateWorkflowGraph, deleteWorkflowGraph, startWorkflowRun, getWorkflowRun, listWorkflowRuns, cancelWorkflowRun, streamWorkflowApprovalDecision, approveWorkflowApproval, rejectWorkflowApproval, cancelWorkflowApprovalAction } from "./workflows";
export type { Workflow, WorkflowNodeKind, WorkflowCondition, WorkflowNodeConfig, WorkflowNodeDefinition, WorkflowEdgeDefinition, WorkflowGraphDefinition, WorkflowGraphRecord, WorkflowRunStatus, WorkflowNodeRunStatus, WorkflowNodeRun, WorkflowRunRecord, WorkflowApprovalEvent, WorkflowRunsQuery } from "./workflows";





















export { sendMessage, stopGeneration } from "./chat";
export type { AgentEvent, EventHandler } from "./chat";




































































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
