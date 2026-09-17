import type { Edge, Node } from "@xyflow/react";
import type {
  CanvasNodeLayout,
  CanvasView,
  TaskGraphDetail,
  TaskNodeDetail,
  TaskExecutionStatus,
  TaskNodeStatus,
} from "../../api/taskWorld";

export type TaskNodeRole = "Task" | "Decision" | "Human" | "Output" | "Group";

export interface TaskNodeProjection extends TaskNodeDetail {
  role: TaskNodeRole;
  isRunning: boolean;
}

export interface TaskGraphProjection {
  graphId: string;
  schemaVersion: number;
  revision: number;
  nodes: TaskNodeProjection[];
  edges: TaskGraphDetail["edges"];
  revisions: TaskGraphDetail["revisions"];
  checkpoints: TaskGraphDetail["checkpoints"];
}

export interface TaskGraphCanvasNodeData extends Record<string, unknown> {
  task: TaskNodeProjection;
  role: TaskNodeRole;
  status: TaskNodeStatus;
  executionStatus: TaskExecutionStatus | null;
  isRunning: boolean;
  isFocused: boolean;
}

export type TaskGraphCanvasNode = Node<TaskGraphCanvasNodeData, "task-world">;
export type TaskGraphCanvasEdge = Edge;

export interface TaskGraphReactFlowModel {
  nodes: TaskGraphCanvasNode[];
  edges: TaskGraphCanvasEdge[];
}

export interface ExecutionTrailItem {
  nodeId: string;
  node: TaskNodeProjection;
}

export interface ExecutorAvailability {
  kind: "configured" | "not_configured" | "unavailable";
  label: string;
  reason: string;
}

const SUPPORTED_TASK_EVENTS = new Set([
  "task.created",
  "task.graph.updated",
  "task.node.ready",
  "task.node.running",
  "task.node.completed",
  "task.node.failed",
  "task.node.cancelled",
  "task.waiting_user",
  "task.canvas.updated",
  "task.checkpoint.created",
  "task.restored",
  "task.execution.created",
  "task.execution.started",
  "task.execution.updated",
  "task.execution.waiting_approval",
  "task.execution.validating",
  "task.execution.succeeded",
  "task.execution.failed",
  "task.execution.cancelled",
  "task.execution.retry",
  "task.node.stale",
  "task.rerun.started",
  "task.rerun.completed",
]);

export function deriveTaskRole(node: TaskNodeDetail): TaskNodeRole {
  if (node.kind === "approval") return "Decision";
  if (node.kind === "user_checkpoint") return "Human";
  if (node.validation.status === "output") return "Output";
  return "Task";
}

export function projectTaskGraph(detail: TaskGraphDetail): TaskGraphProjection {
  return {
    graphId: detail.graph_id,
    schemaVersion: detail.graph.schema_version,
    revision: detail.revision,
    nodes: detail.nodes.map((node) => ({
      ...node,
      state: { ...node.state },
      acceptance_criteria: [...node.acceptance_criteria],
      resources: node.resources.map((resource) => ({ ...resource })),
      validation: { ...node.validation, issues: [...node.validation.issues] },
      role: deriveTaskRole(node),
      latest_execution: node.latest_execution
        ? {
            ...node.latest_execution,
            validation: node.latest_execution.validation
              ? {
                  ...node.latest_execution.validation,
                  issues: [...node.latest_execution.validation.issues],
                }
              : null,
          }
        : node.latest_execution,
      execution_history: (node.execution_history || []).map((execution) => ({
        ...execution,
        validation: execution.validation
          ? { ...execution.validation, issues: [...execution.validation.issues] }
          : null,
      })),
      isRunning: node.status === "running" || isActiveExecution(node.latest_execution?.status),
    })),
    edges: detail.edges.map((edge) => ({ ...edge })),
    revisions: detail.revisions.map((revision) => ({ ...revision })),
    checkpoints: detail.checkpoints.map((checkpoint) => ({ ...checkpoint })),
  };
}

export function isActiveExecution(status: TaskExecutionStatus | null | undefined): boolean {
  return status === "dispatching"
    || status === "waiting_approval"
    || status === "running"
    || status === "validating";
}

export function executionStatusLabel(status: TaskExecutionStatus): string {
  const labels: Record<TaskExecutionStatus, string> = {
    pending: "待执行",
    ready: "已就绪",
    dispatching: "派发中",
    waiting_approval: "等待审批",
    running: "执行中",
    validating: "校验中",
    succeeded: "校验通过",
    failed: "失败",
    blocked: "已阻塞",
    cancelled: "已取消",
    stale: "已过期",
  };
  return labels[status];
}

export function toReactFlowModel(
  projection: TaskGraphProjection,
  view: CanvasView | null,
  focusedNodeId: string | null = null,
): TaskGraphReactFlowModel {
  const layouts = new Map(view?.node_layouts.map((layout) => [layout.node_id, layout]));
  const hiddenNodeIds = new Set(
    (view?.groups || [])
      .filter((group) => group.collapsed)
      .flatMap((group) => group.node_ids),
  );
  const nodes = projection.nodes.filter((task) => !hiddenNodeIds.has(task.id)).map((task, index) => {
    const layout = layouts.get(task.id);
    return {
      id: task.id,
      type: "task-world",
      position: layout ? { x: layout.x, y: layout.y } : defaultPosition(index),
      width: layout?.width,
      height: layout?.height,
      data: {
        task,
        role: task.role,
        status: task.status,
        executionStatus: task.latest_execution?.status || null,
        isRunning: task.isRunning,
        isFocused: focusedNodeId === task.id,
      },
      selected: view?.selection.includes(task.id) || focusedNodeId === task.id,
    } satisfies TaskGraphCanvasNode;
  });
  const edges = projection.edges.filter((edge) => !hiddenNodeIds.has(edge.from) && !hiddenNodeIds.has(edge.to)).map((edge) => ({
    id: `${edge.from}->${edge.to}`,
    source: edge.from,
    target: edge.to,
    type: "smoothstep",
  }));
  return { nodes, edges };
}

export function buildAutoLayout(projection: TaskGraphProjection): CanvasNodeLayout[] {
  const indegree = new Map(projection.nodes.map((node) => [node.id, 0]));
  const outgoing = new Map(projection.nodes.map((node) => [node.id, [] as string[]]));
  for (const edge of projection.edges) {
    if (!indegree.has(edge.from) || !indegree.has(edge.to)) continue;
    indegree.set(edge.to, (indegree.get(edge.to) || 0) + 1);
    outgoing.get(edge.from)?.push(edge.to);
  }
  const queue = projection.nodes.filter((node) => indegree.get(node.id) === 0).map((node) => node.id);
  const level = new Map(queue.map((id) => [id, 0]));
  for (let cursor = 0; cursor < queue.length; cursor += 1) {
    const id = queue[cursor];
    for (const next of outgoing.get(id) || []) {
      level.set(next, Math.max(level.get(next) || 0, (level.get(id) || 0) + 1));
      const remaining = (indegree.get(next) || 0) - 1;
      indegree.set(next, remaining);
      if (remaining === 0) queue.push(next);
    }
  }
  const rows = new Map<number, number>();
  return projection.nodes.map((node, index) => {
    const column = level.get(node.id) ?? index;
    const row = rows.get(column) || 0;
    rows.set(column, row + 1);
    return {
      node_id: node.id,
      x: 40 + column * 320,
      y: 40 + row * 200,
      width: 240,
      height: 128,
    };
  });
}

function defaultPosition(index: number) {
  const column = index % 4;
  const row = Math.floor(index / 4);
  return { x: 40 + column * 320, y: 40 + row * 200 };
}

export function buildExecutionTrail(projection: TaskGraphProjection): ExecutionTrailItem[] {
  return projection.nodes
    .map((node) => ({ nodeId: node.id, node }))
    .sort((left, right) => right.node.state.updated_at - left.node.state.updated_at);
}

export function isTaskWorldEvent(
  event: { type?: unknown; payload?: unknown },
  graphId: string,
): boolean {
  if (typeof event.type !== "string" || !SUPPORTED_TASK_EVENTS.has(event.type)) return false;
  if (!event.payload || typeof event.payload !== "object" || Array.isArray(event.payload)) return false;
  const payload = event.payload as { graph_id?: unknown };
  return payload.graph_id === graphId;
}

export function getExecutorAvailability(executorRef: string | null): ExecutorAvailability {
  if (executorRef === "command://desktop.app.focus" || executorRef === "command://desktop.app.open") {
    return {
      kind: "unavailable",
      label: "未安装",
      reason: "v1 未安装桌面控制 Provider；该执行引用不会被调度。",
    };
  }
  if (!executorRef) {
    return {
      kind: "not_configured",
      label: "未配置",
      reason: "该任务尚未配置执行引用。",
    };
  }
  return {
    kind: "configured",
    label: "已配置",
    reason: executorRef,
  };
}
