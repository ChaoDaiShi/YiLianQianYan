import { useCallback, useEffect, useMemo, useState } from "react";
import {
  applyEdgeChanges,
  applyNodeChanges,
  Background,
  Controls,
  Handle,
  MiniMap,
  Position,
  ReactFlow,
  ReactFlowProvider,
  SelectionMode,
  useReactFlow,
  type Connection,
  type EdgeChange,
  type NodeChange,
  type NodeProps,
  type Viewport,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import type { CanvasNodeLayout, CanvasViewport, CanvasView } from "../../api/taskWorld";
import {
  executionStatusLabel,
  toReactFlowModel,
  type TaskGraphCanvasEdge,
  type TaskGraphCanvasNode,
  type TaskGraphProjection,
} from "./taskGraphProjection";

interface TaskWorldCanvasProps {
  projection: TaskGraphProjection;
  view: CanvasView | null;
  focusedNodeId: string | null;
  onFocusNode: (nodeId: string) => void;
  onLayoutSave: (layouts: CanvasNodeLayout[]) => void;
  onViewportChange: (viewport: CanvasViewport) => void;
  onSelectionChange: (nodeIds: string[]) => void;
  onConnect: (connection: Connection) => void;
  onDeleteNodes: (nodes: TaskGraphCanvasNode[]) => void;
  onDeleteEdges: (edges: TaskGraphCanvasEdge[]) => void;
  semanticLocked?: boolean;
}

export default function TaskWorldCanvas(props: TaskWorldCanvasProps) {
  return (
    <ReactFlowProvider>
      <TaskWorldCanvasSurface {...props} />
    </ReactFlowProvider>
  );
}

function TaskWorldCanvasSurface({
  projection,
  view,
  focusedNodeId,
  onFocusNode,
  onLayoutSave,
  onViewportChange,
  onSelectionChange,
  onConnect,
  onDeleteNodes,
  onDeleteEdges,
  semanticLocked = false,
}: TaskWorldCanvasProps) {
  const model = useMemo(
    () => toReactFlowModel(projection, view, focusedNodeId),
    [focusedNodeId, projection, view],
  );
  const [nodes, setNodes] = useState<TaskGraphCanvasNode[]>(model.nodes);
  const [edges, setEdges] = useState<TaskGraphCanvasEdge[]>(model.edges);
  const { fitView } = useReactFlow<TaskGraphCanvasNode, TaskGraphCanvasEdge>();

  useEffect(() => {
    setNodes(model.nodes);
    setEdges(model.edges);
  }, [model]);

  useEffect(() => {
    if (!focusedNodeId || !nodes.some((node) => node.id === focusedNodeId)) return;
    void fitView({
      nodes: [{ id: focusedNodeId }],
      duration: 280,
      padding: 0.25,
    });
  }, [fitView, focusedNodeId, nodes]);

  const handleNodesChange = useCallback((changes: NodeChange<TaskGraphCanvasNode>[]) => {
    const allowed = semanticLocked ? changes.filter((change) => change.type !== "remove") : changes;
    setNodes((current) => applyNodeChanges(allowed, current));
  }, [semanticLocked]);

  const handleEdgesChange = useCallback((changes: EdgeChange[]) => {
    const allowed = semanticLocked ? changes.filter((change) => change.type !== "remove") : changes;
    setEdges((current) => applyEdgeChanges(allowed, current));
  }, [semanticLocked]);

  const handleNodeDragStop = useCallback(
    (_event: MouseEvent | TouchEvent, _node: TaskGraphCanvasNode, draggedNodes: TaskGraphCanvasNode[]) => {
      onLayoutSave(
        draggedNodes.map((node) => ({
          node_id: node.id,
          x: node.position.x,
          y: node.position.y,
          width: node.width || 240,
          height: node.height || 128,
        })),
      );
    },
    [onLayoutSave],
  );

  const handleMoveEnd = useCallback(
    (_event: MouseEvent | TouchEvent | null, viewport: Viewport) => {
      onViewportChange({ x: viewport.x, y: viewport.y, zoom: viewport.zoom });
    },
    [onViewportChange],
  );

  return (
    <div className="task-world-canvas" data-testid="task-world-canvas">
      <ReactFlow<TaskGraphCanvasNode, TaskGraphCanvasEdge>
        nodes={nodes}
        edges={edges}
        nodeTypes={NODE_TYPES}
        onNodesChange={handleNodesChange}
        onEdgesChange={handleEdgesChange}
        onNodeClick={(_event, node) => onFocusNode(node.id)}
        onConnect={semanticLocked ? undefined : onConnect}
        onNodeDragStop={handleNodeDragStop}
        onMoveEnd={handleMoveEnd}
        onSelectionChange={({ nodes: selectedNodes }) =>
          onSelectionChange(selectedNodes.map((node) => node.id))
        }
        onNodesDelete={onDeleteNodes}
        onEdgesDelete={onDeleteEdges}
        fitView
        selectionOnDrag
        selectionMode={SelectionMode.Partial}
        selectNodesOnDrag
        deleteKeyCode={semanticLocked ? null : ["Backspace", "Delete"]}
        multiSelectionKeyCode={["Control", "Meta"]}
        minZoom={0.1}
        maxZoom={4}
        nodesConnectable={!semanticLocked}
        nodesDraggable
        elementsSelectable
        aria-label="无限任务画布"
      >
        <Background gap={24} size={1} color="var(--task-world-grid)" />
        <Controls showInteractive={false} />
        <MiniMap pannable zoomable className="task-world-minimap" />
      </ReactFlow>
    </div>
  );
}

function TaskWorldNode({ data, selected }: NodeProps<TaskGraphCanvasNode>) {
  return (
    <div
      className={`task-world-node ${selected ? "is-selected" : ""}`}
      data-role={data.role}
      data-status={data.status}
      data-execution-status={data.executionStatus || undefined}
      data-running={data.isRunning ? "true" : "false"}
    >
      <Handle type="target" position={Position.Left} className="task-world-handle" />
      <div className="task-world-node-kicker">
        <span>{data.role}</span>
        <span>{data.executionStatus ? executionStatusLabel(data.executionStatus) : statusLabel(data.status)}</span>
      </div>
      <div className="task-world-node-title">{data.task.title}</div>
      <div className="task-world-node-summary">{data.task.instruction_summary}</div>
      {data.task.latest_execution && (
        <span className="task-world-execution-attempt">
          第 {data.task.latest_execution.attempt} 次尝试
        </span>
      )}
      {data.isRunning && <span className="task-world-running">Running</span>}
      <Handle type="source" position={Position.Right} className="task-world-handle" />
    </div>
  );
}

const NODE_TYPES = { "task-world": TaskWorldNode };

function statusLabel(status: TaskGraphCanvasNode["data"]["status"]): string {
  const labels: Record<TaskGraphCanvasNode["data"]["status"], string> = {
    pending: "待处理",
    runnable: "可运行",
    running: "运行中",
    succeeded: "已完成",
    failed: "失败",
    blocked: "已阻塞",
    cancelled: "已取消",
    invalidated: "需复核",
  };
  return labels[status];
}
