// ============================================================
// Legacy API surface — compatibility re-exports only
//
// This module used to own every domain implementation. R2 / S6 moved them into
// the domain entrypoints (`conversations`, `chat`, `system`, `plugins`,
// `memory`, `workflows`, `tasks`, `workspaces`, `capabilities`), each of which
// imports its transport from `src/core/api/http.ts`.
//
// What is left is a compatibility surface: `api/client.ts` re-exports this
// module, so every pre-existing `from "../api/client"` import keeps resolving
// to the same function objects. No definition is declared here, and none may
// be added — `src/architecture/boundaries.test.ts` enforces that.
//
// New code should import the narrow domain module it consumes.
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
