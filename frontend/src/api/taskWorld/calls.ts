// Task World calls — graph definition, nodes and edges, executions, commands,
// checkpoints and the canvas view. Each is a thin wrapper over
// `requestTaskWorld`; the decisions live in the backend.
import type { CommandResult } from "../commands";
import { requestTaskWorld } from "./transport";
import { graphSummary } from "./transport";
import type {
  ApiResult,
  CanvasView,
  TaskCheckpointSummary,
  TaskGraphDefinition,
  TaskGraphDetail,
  TaskGraphReview,
  TaskGraphSummary,
  TaskNodeExecutionSummary,
  TaskRerunResponse,
} from "./types";
import type { CanvasViewResponse, TaskCheckpointResponse, TaskExecutionResponse, TaskExecutionsResponse, TaskGraphDetailResponse, TaskGraphNodeDefinition, TaskGraphResponse, TaskGraphReviewResponse, TaskGraphsResponse } from "./types";

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

export async function planTaskGraph(id: string, goal: string): Promise<ApiResult<TaskGraphDefinition>> {
  const result = await requestTaskWorld<TaskGraphResponse>("POST", "/api/task-world/graphs", { id, goal });
  return result.ok ? { ok: true, data: result.data.graph } : result;
}

export async function getTaskGraphDetail(graphId: string): Promise<ApiResult<TaskGraphDetail>> {
  const result = await requestTaskWorld<TaskGraphDetailResponse>(
    "GET",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/detail`,
  );
  return result.ok ? { ok: true, data: result.data.detail } : result;
}

export async function reviewTaskGraph(
  graphId: string,
  expectedRevision: number,
): Promise<ApiResult<TaskGraphReview>> {
  const result = await requestTaskWorld<TaskGraphReviewResponse>(
    "POST",
    `/api/task-world/graphs/${encodeURIComponent(graphId)}/review`,
    { expected_revision: expectedRevision },
  );
  return result.ok
    ? { ok: true, data: { ...result.data.review, reviewed_revision: result.data.reviewed_revision } }
    : result;
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
      groups: view.groups,
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
