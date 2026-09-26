import { useCallback, useEffect, useMemo, useState } from "react";
import {
  applyEdgeChanges,
  applyNodeChanges,
  Background,
  Controls,
  MiniMap,
  Panel as FlowPanel,
  ReactFlow,
  ReactFlowProvider,
  SelectionMode,
  useReactFlow,
  type Connection,
  type EdgeChange,
  type NodeChange,
  type Viewport,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import type { CanvasNodeLayout, CanvasViewport, CanvasView } from "../../../api/taskWorld";
import {
  buildAutoLayout,
  type TaskGraphCanvasEdge,
  type TaskGraphCanvasNode,
  type TaskGraphProjection,
} from "../taskGraphProjection";
import TaskNode from "./TaskNode";
import { reconcileCanvasNodes } from "./model/reconcileCanvasNodes";
import { canvasCameraCommand, initialCanvasViewport, type CanvasCameraEvent } from "./model/canvasViewport";
import TaskExecutionTrail from "../TaskExecutionTrail";
import { Button, Panel } from "../../../components/ui";

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
    <ReactFlowProvider key={props.projection.graphId}>
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
  const incomingEdges = useMemo(() => {
    const hidden = new Set(view?.groups.filter((group) => group.collapsed).flatMap((group) => group.node_ids));
    return projection.edges.map((edge) => ({
      id: `${edge.from}->${edge.to}`, source: edge.from, target: edge.to,
      type: "smoothstep", hidden: hidden.has(edge.from) || hidden.has(edge.to),
    }));
  }, [projection.edges, view?.groups]);
  const [nodes, setNodes] = useState<TaskGraphCanvasNode[]>(() => reconcileCanvasNodes([], projection, view, { focusedNodeId }));
  const [edges, setEdges] = useState<TaskGraphCanvasEdge[]>(incomingEdges);
  const [initialViewport] = useState(() => initialCanvasViewport(view?.viewport));
  const { fitView, zoomTo, getNode } = useReactFlow<TaskGraphCanvasNode, TaskGraphCanvasEdge>();

  useEffect(() => {
    setNodes((current) => reconcileCanvasNodes(current, projection, view, { focusedNodeId }));
  }, [projection, view, focusedNodeId]);

  useEffect(() => {
    setEdges((current) => incomingEdges.map((edge) => ({ ...current.find((item) => item.id === edge.id), ...edge })));
  }, [incomingEdges]);

  const moveCamera = useCallback((event: CanvasCameraEvent) => {
    if (event.type === "locate" && (!getNode(event.nodeId) || getNode(event.nodeId)?.hidden)) return;
    const command = canvasCameraCommand(event);
    if (command?.kind === "fit") void fitView(command.options);
    if (command?.kind === "zoom") void zoomTo(command.zoom);
  }, [fitView, getNode, zoomTo]);

  const selectFromTrail = useCallback((nodeId: string) => {
    setNodes((current) => current.map((node) => node.selected === (node.id === nodeId)
      ? node : { ...node, selected: node.id === nodeId }));
    onFocusNode(nodeId);
  }, [onFocusNode]);
  const locateNode = useCallback((nodeId: string) => moveCamera({ type: "locate", nodeId }), [moveCamera]);
  const autoLayout = useCallback(() => {
    const layouts = buildAutoLayout(projection);
    setNodes((current) => reconcileCanvasNodes(current, projection, view, { focusedNodeId, layouts }));
    onLayoutSave(layouts);
  }, [focusedNodeId, onLayoutSave, projection, view]);
  const handleNodeClick = useCallback((_event: React.MouseEvent, node: TaskGraphCanvasNode) => onFocusNode(node.id), [onFocusNode]);
  const handleSelectionChange = useCallback(({ nodes: selectedNodes }: { nodes: TaskGraphCanvasNode[] }) => {
    onSelectionChange(selectedNodes.map((node) => node.id));
  }, [onSelectionChange]);

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
    <>
    <TaskExecutionTrail projection={projection} focusedNodeId={focusedNodeId} onFocus={selectFromTrail} onLocate={locateNode} />
    <Panel padding={false} className="min-h-0 overflow-hidden">
    <div className="task-world-canvas" data-testid="task-world-canvas">
      <ReactFlow<TaskGraphCanvasNode, TaskGraphCanvasEdge>
        nodes={nodes}
        edges={edges}
        nodeTypes={NODE_TYPES}
        onNodesChange={handleNodesChange}
        onEdgesChange={handleEdgesChange}
        onNodeClick={handleNodeClick}
        onConnect={semanticLocked ? undefined : onConnect}
        onNodeDragStop={handleNodeDragStop}
        onMoveEnd={handleMoveEnd}
        onSelectionChange={handleSelectionChange}
        onNodesDelete={onDeleteNodes}
        onEdgesDelete={onDeleteEdges}
        defaultViewport={initialViewport}
        autoPanOnNodeFocus={false}
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
        <FlowPanel position="top-left" className="flex gap-1" aria-label="画布布局与镜头">
          <Button size="sm" variant="secondary" onClick={autoLayout}>自动布局</Button>
          <Button size="sm" variant="secondary" onClick={() => moveCamera({ type: "zoom-100" })}>100%</Button>
          <Button size="sm" variant="secondary" onClick={() => moveCamera({ type: "fit-all" })}>适应全部</Button>
        </FlowPanel>
        <Controls showInteractive={false} showFitView={false} />
        <MiniMap pannable zoomable className="task-world-minimap" />
      </ReactFlow>
    </div>
    </Panel>
    </>
  );
}

const NODE_TYPES = { "task-world": TaskNode };
