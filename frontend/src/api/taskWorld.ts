import { controlSessionHeaders } from "./controlSession";
import { API_BASE } from "./transport";
import type { CommandResult } from "./commands";

export type TaskNodeKind = "work" | "approval" | "user_checkpoint";

export type TaskNodeStatus =
  | "pending"
  | "runnable"
  | "running"
  | "succeeded"
  | "failed"
  | "blocked"
  | "cancelled"
  | "invalidated";

export type TaskExecutionStatus =
  | "pending"
  | "ready"
  | "dispatching"
  | "waiting_approval"
  | "running"
  | "validating"
  | "succeeded"
  | "failed"
  | "blocked"
  | "cancelled"
  | "stale";

export interface RetryPolicy {
  max_attempts: number;
}

/** The raw graph definition is kept inside the API boundary only. */
export interface TaskGraphNodeDefinition {
  id: string;
  kind: TaskNodeKind;
  title: string;
  input: Record<string, unknown>;
  retry_policy: RetryPolicy;
}

export interface TaskGraphDefinition {
  schema_version: number;
  id: string;
  revision: number;
  nodes: TaskGraphNodeDefinition[];
  edges: TaskEdge[];
}

export interface TaskGraphSummary {
  id: string;
  schema_version: number;
  revision: number;
  node_count: number;
  edge_count: number;
}

export interface TaskNodeStateSummary {
  status: TaskNodeStatus;
  attempts: number;
  result_summary: string | null;
  error: string | null;
  started_at: number | null;
  finished_at: number | null;
  updated_at: number;
  command_execution?: TaskCommandExecution | null;
}

export interface CommandBinding {
  command: "desktop.app.focus" | "desktop.app.open";
  args: { app_id: string };
}

export interface TaskCommandExecution {
  request_id: string;
  command: string;
  app_id: string;
  attempt: number;
  graph_revision: number;
  status: "dispatching" | "waiting_approval" | "running" | "verified" | "failed" | "cancelled";
  approval_id: string | null;
}

export interface TaskResourceReference {
  id: string;
  name: string | null;
  uri: string | null;
}

export interface TaskValidationSummary {
  status: string;
  issues: string[];
}

export interface TaskNodeDetail {
  id: string;
  kind: TaskNodeKind;
  title: string;
  status: TaskNodeStatus;
  state: TaskNodeStateSummary;
  executor_ref: string | null;
  command_binding?: CommandBinding | null;
  instruction_summary: string;
  acceptance_criteria: string[];
  resources: TaskResourceReference[];
  validation: TaskValidationSummary;
  result_summary: string | null;
  latest_execution?: TaskNodeExecutionSummary | null;
  execution_history?: TaskNodeExecutionSummary[];
}

export interface TaskExecutionValidationSummary {
  status: "pending" | "accepted" | "rejected";
  issues: string[];
  checked_at: number | null;
}

export interface TaskNodeExecutionSummary {
  execution_id: string;
  attempt: number;
  status: TaskExecutionStatus;
  executor_ref: string | null;
  validation: TaskExecutionValidationSummary | null;
  approval_ref: string | null;
  failure_code: string | null;
  error: string | null;
  result_summary: string | null;
  created_at: number;
  updated_at: number;
  started_at: number | null;
  finished_at: number | null;
}

export interface TaskEdge {
  from: string;
  to: string;
}

export interface TaskRevisionSummary {
  graph_id: string;
  revision: number;
  change: string;
  node_count: number;
  edge_count: number;
  created_at: number;
}

export interface TaskCheckpointSummary {
  checkpoint_id: string;
  graph_id: string;
  graph_revision: number;
  created_at: number;
}

export interface TaskGraphDetail {
  graph: TaskGraphSummary;
  graph_id: string;
  revision: number;
  nodes: TaskNodeDetail[];
  edges: TaskEdge[];
  revisions: TaskRevisionSummary[];
  checkpoints: TaskCheckpointSummary[];
}

export interface CanvasViewport {
  x: number;
  y: number;
  zoom: number;
}

export interface CanvasNodeLayout {
  node_id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface CanvasView {
  schema_version: number;
  graph_id: string;
  view_revision: number;
  graph_revision_seen: number;
  viewport: CanvasViewport;
  node_layouts: CanvasNodeLayout[];
  selection: string[];
  updated_at: number;
}

export interface ApiError {
  status: number;
  code: string;
  message: string;
}

export type ApiResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: ApiError };

interface TaskGraphResponse {
  graph: TaskGraphDefinition;
}

interface TaskGraphsResponse {
  graphs: TaskGraphDefinition[];
}

interface TaskGraphDetailResponse {
  detail: TaskGraphDetail;
}

interface CanvasViewResponse {
  view: CanvasView;
}

interface TaskCheckpointResponse {
  checkpoint: TaskCheckpointSummary;
}

interface TaskExecutionResponse {
  execution: TaskNodeExecutionSummary;
}

interface TaskExecutionsResponse {
  executions: TaskNodeExecutionSummary[];
}

export interface TaskRerunResponse {
  graph_id: string;
  node_id: string;
  affected_nodes: string[];
}

interface ErrorPayload {
  error?: unknown;
  message?: unknown;
}

async function requestTaskWorld<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<ApiResult<T>> {
  try {
    const options: RequestInit = {
      method,
      headers: {
        "Content-Type": "application/json",
        ...controlSessionHeaders(),
      },
    };
    if (body !== undefined) options.body = JSON.stringify(body);

    const response = await fetch(`${API_BASE}${path}`, options);
    const text = response.status === 204 ? "" : await response.text();
    const payload = parseJson(text);
    if (!response.ok) {
      const message = errorMessage(payload) || `HTTP ${response.status}`;
      return {
        ok: false,
        error: {
          status: response.status,
          code: errorCode(payload) || `http_${response.status}`,
          message,
        },
      };
    }
    if (!text) return { ok: true, data: {} as T };
    if (payload === null) {
      return {
        ok: false,
        error: {
          status: 0,
          code: "invalid_json",
          message: "Task World API 返回了无效 JSON。",
        },
      };
    }
    return { ok: true, data: payload as T };
  } catch (error: unknown) {
    return {
      ok: false,
      error: {
        status: 0,
        code: "network_error",
        message: error instanceof Error ? error.message : String(error),
      },
    };
  }
}

function parseJson(text: string): unknown {
  if (!text.trim()) return null;
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return null;
  }
}

function errorMessage(payload: unknown): string | null {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return null;
  const value = payload as ErrorPayload;
  return typeof value.message === "string"
    ? value.message
    : typeof value.error === "string"
      ? value.error
      : null;
}

function errorCode(payload: unknown): string | null {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return null;
  const code = (payload as ErrorPayload).error;
  return typeof code === "string" && /^[a-z0-9_\-]+$/.test(code) ? code : null;
}

function graphSummary(graph: TaskGraphDefinition): TaskGraphSummary {
  return {
    id: graph.id,
    schema_version: graph.schema_version,
    revision: graph.revision,
    node_count: graph.nodes.length,
    edge_count: graph.edges.length,
  };
}

export async function listTaskGraphs(): Promise<ApiResult<TaskGraphSummary[]>> {
  const result = await requestTaskWorld<TaskGraphsResponse>("GET", "/api/task-world/graphs");
  return result.ok
    ? { ok: true, data: result.data.graphs.map(graphSummary) }
    : result;
}

export async function getTaskGraph(graphId: string): Promise<ApiResult<TaskGraphDefinition>> {
  const result = await requestTaskWorld<TaskGraphResponse>(
    "GET",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}`,
  );
  return result.ok ? { ok: true, data: result.data.graph } : result;
}

export async function createTaskGraph(
  graph: Pick<TaskGraphDefinition, "id" | "nodes" | "edges">,
): Promise<ApiResult<TaskGraphDefinition>> {
  const result = await requestTaskWorld<TaskGraphResponse>("POST", "/api/task-world/graphs", graph);
  return result.ok ? { ok: true, data: result.data.graph } : result;
}

export async function getTaskGraphDetail(graphId: string): Promise<ApiResult<TaskGraphDetail>> {
  const result = await requestTaskWorld<TaskGraphDetailResponse>(
    "GET",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/detail`,
  );
  return result.ok ? { ok: true, data: result.data.detail } : result;
}

export async function getCanvasView(graphId: string): Promise<ApiResult<CanvasView>> {
  const result = await requestTaskWorld<CanvasViewResponse>(
    "GET",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/canvas-view`,
  );
  return result.ok ? { ok: true, data: result.data.view } : result;
}

export async function saveCanvasView(
  graphId: string,
  view: CanvasView,
  expectedViewRevision = view.view_revision,
): Promise<ApiResult<CanvasView>> {
  const result = await requestTaskWorld<CanvasViewResponse>(
    "PUT",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/canvas-view`,
    {
      expected_view_revision: expectedViewRevision,
      schema_version: view.schema_version,
      view_revision: view.view_revision,
      graph_revision_seen: view.graph_revision_seen,
      viewport: view.viewport,
      node_layouts: view.node_layouts,
      selection: view.selection,
    },
  );
  return result.ok ? { ok: true, data: result.data.view } : result;
}

export async function addTaskNode(
  graphId: string,
  expectedRevision: number,
  node: TaskGraphNodeDefinition,
): Promise<ApiResult<TaskGraphDefinition>> {
  const result = await requestTaskWorld<TaskGraphResponse>(
    "POST",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/nodes`,
    { expected_revision: expectedRevision, node },
  );
  return result.ok ? { ok: true, data: result.data.graph } : result;
}

export async function updateTaskNode(
  graphId: string,
  nodeId: string,
  expectedRevision: number,
  node: Pick<TaskGraphNodeDefinition, "kind" | "title" | "input" | "retry_policy">,
): Promise<ApiResult<TaskGraphDefinition>> {
  const result = await requestTaskWorld<TaskGraphResponse>(
    "PUT",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/nodes/${encodeURIComponent(nodeId)}`,
    { expected_revision: expectedRevision, ...node },
  );
  return result.ok ? { ok: true, data: result.data.graph } : result;
}

export async function deleteTaskNode(
  graphId: string,
  nodeId: string,
  expectedRevision: number,
): Promise<ApiResult<TaskGraphDefinition>> {
  const result = await requestTaskWorld<TaskGraphResponse>(
    "DELETE",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/nodes/${encodeURIComponent(nodeId)}`,
    { expected_revision: expectedRevision },
  );
  return result.ok ? { ok: true, data: result.data.graph } : result;
}

export async function startTaskNode(
  graphId: string,
  nodeId: string,
  expectedRevision: number,
): Promise<ApiResult<TaskGraphDefinition>> {
  const result = await requestTaskWorld<TaskGraphResponse>(
    "POST",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/nodes/${encodeURIComponent(nodeId)}/start`,
    { expected_revision: expectedRevision },
  );
  return result.ok ? { ok: true, data: result.data.graph } : result;
}

export async function startTaskExecution(
  graphId: string,
  nodeId: string,
  expectedRevision: number,
): Promise<ApiResult<TaskNodeExecutionSummary>> {
  const result = await requestTaskWorld<TaskExecutionResponse>(
    "POST",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/nodes/${encodeURIComponent(nodeId)}/executions`,
    { expected_revision: expectedRevision },
  );
  return result.ok ? { ok: true, data: result.data.execution } : result;
}

export async function listTaskNodeExecutions(
  graphId: string,
  nodeId: string,
): Promise<ApiResult<TaskNodeExecutionSummary[]>> {
  const result = await requestTaskWorld<TaskExecutionsResponse>(
    "GET",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/nodes/${encodeURIComponent(nodeId)}/executions`,
  );
  return result.ok ? { ok: true, data: result.data.executions } : result;
}

export async function cancelTaskExecution(
  graphId: string,
  executionId: string,
  expectedRevision: number,
): Promise<ApiResult<TaskNodeExecutionSummary>> {
  const result = await requestTaskWorld<TaskExecutionResponse>(
    "POST",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/executions/${encodeURIComponent(executionId)}/cancel`,
    { expected_revision: expectedRevision },
  );
  return result.ok ? { ok: true, data: result.data.execution } : result;
}

export async function rerunTaskFromNode(
  graphId: string,
  nodeId: string,
  expectedRevision: number,
): Promise<ApiResult<TaskRerunResponse>> {
  return requestTaskWorld<TaskRerunResponse>(
    "POST",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/rerun`,
    { expected_revision: expectedRevision, node_id: nodeId },
  );
}

export async function executeTaskCommand(graphId: string, nodeId: string, expectedRevision: number): Promise<ApiResult<CommandResult>> {
  return requestTaskWorld<CommandResult>("POST",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/nodes/${encodeURIComponent(nodeId)}/execute-command`,
    { expected_revision: expectedRevision });
}

export async function cancelTaskCommand(graphId: string, nodeId: string, expectedRevision: number, requestId: string): Promise<ApiResult<CommandResult>> {
  return requestTaskWorld<CommandResult>("POST",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/nodes/${encodeURIComponent(nodeId)}/cancel-command`,
    { expected_revision: expectedRevision, request_id: requestId });
}

export async function addTaskEdge(
  graphId: string,
  expectedRevision: number,
  from: string,
  to: string,
): Promise<ApiResult<TaskGraphDefinition>> {
  const result = await requestTaskWorld<TaskGraphResponse>(
    "POST",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/edges`,
    { expected_revision: expectedRevision, from, to },
  );
  return result.ok ? { ok: true, data: result.data.graph } : result;
}

export async function deleteTaskEdge(
  graphId: string,
  from: string,
  to: string,
  expectedRevision: number,
): Promise<ApiResult<TaskGraphDefinition>> {
  const result = await requestTaskWorld<TaskGraphResponse>(
    "DELETE",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/edges/${encodeURIComponent(from)}/${encodeURIComponent(to)}`,
    { expected_revision: expectedRevision },
  );
  return result.ok ? { ok: true, data: result.data.graph } : result;
}

export async function createTaskCheckpoint(
  graphId: string,
  expectedRevision: number,
): Promise<ApiResult<TaskCheckpointSummary>> {
  const result = await requestTaskWorld<TaskCheckpointResponse>(
    "POST",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/checkpoint`,
    { expected_revision: expectedRevision },
  );
  return result.ok ? { ok: true, data: result.data.checkpoint } : result;
}

export async function restoreTaskCheckpoint(
  graphId: string,
  checkpointId: string,
  expectedRevision: number,
): Promise<ApiResult<TaskGraphDefinition>> {
  const result = await requestTaskWorld<TaskGraphResponse>(
    "POST",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/restore`,
    { expected_revision: expectedRevision, checkpoint_id: checkpointId },
  );
  return result.ok ? { ok: true, data: result.data.graph } : result;
}
