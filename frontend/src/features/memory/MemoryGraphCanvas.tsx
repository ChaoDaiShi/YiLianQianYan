import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Background, BaseEdge, Controls, MarkerType, Position, ReactFlow, type Edge, type EdgeProps, type Node, type ReactFlowInstance, type Viewport } from "@xyflow/react";
import { Link } from "react-router-dom";
import { Badge, Button, EmptyState, Modal } from "../../components/ui";
import { formatMemoryTime } from "./memoryCenterModel";
import { relationshipKinds } from "./relationship";
import { layoutRelationshipOverview } from "./relationshipOverviewLayout";
import { keepMemoryAnchors, layoutMemoryWorld, memoryNodeSize } from "./memoryWorldLayout";
import { memoryStarGlyph, versionRings } from "./memoryStar";
import { memorySpatialCurve, sampleMemorySpatialCurve, type MemoryPoint3 } from "./memorySpatialCurve";
import type { MemorySimilarity } from "./memoryGraphApi";
import type { GraphEdge, GraphNode, GraphNodeKind, MemoryGraph } from "./memoryGraphModel";
import "@xyflow/react/dist/style.css";

const PAGE_SIZE = 12;
const arrow = { type: MarkerType.ArrowClosed, color: "var(--accent-purple)" } as const;
const kinds: Record<GraphNodeKind, string> = {
  category: "记忆分类", memory: "记忆", person: "人物", conversation: "对话",
  event: "事件", project: "项目与事项", concept: "概念", preference: "偏好", knowledge: "知识条目", document: "资料原文",
};

function edgeLabel(edge: GraphEdge, grouped: boolean) {
  const label = grouped && [...edge.label].length > 12 ? `${[...edge.label].slice(0, 12).join("")}…` : edge.label;
  return `${label}${edge.inferred ? " · 待确认" : ""}`;
}

type MemoryCurveEdge = Edge<{from: MemoryPoint3; to: MemoryPoint3; fromRadius: number; toRadius: number}>;

function MemoryCurve({ id, data, sourceX, sourceY, targetX, targetY, style, markerEnd, label, labelStyle }: EdgeProps<MemoryCurveEdge>) {
  const a = data?.from ?? {x:sourceX,y:sourceY,z:0};
  const b = data?.to ?? {x:targetX,y:targetY,z:0};
  const d = Math.hypot(b.x - a.x, b.y - a.y) || 1;
  const sourceT = Math.min(.35, (data?.fromRadius ?? 0) / d);
  const targetT = Math.min(.35, (data?.toRadius ?? 0) / d);
  const curve = memorySpatialCurve(a, b, "relation", id);
  const from = sampleMemorySpatialCurve(curve, sourceT);
  const to = sampleMemorySpatialCurve(curve, 1 - targetT);
  const [start, controlA, controlB, end] = memorySpatialCurve(from, to, "relation", id);
  const middle = sampleMemorySpatialCurve([start,controlA,controlB,end], .5);
  const path = "M " + start.x + " " + start.y + " C " + controlA.x + " " + controlA.y + " " +
    controlB.x + " " + controlB.y + " " + end.x + " " + end.y;
  return <BaseEdge id={id} path={path} style={style} markerEnd={markerEnd}
    label={label} labelStyle={labelStyle} labelX={middle.x} labelY={middle.y} />;
}

const memoryEdgeTypes = {memoryCurve: MemoryCurve};

export type CanvasProjection = { positions: Map<string, {x: number; y: number}>; edges: GraphEdge[]; viewport?: Viewport; nodes?: GraphNode[]; shared?: boolean; cameraToolbar?: HTMLElement | null };

export function MemoryGraphCanvas({ graph, focusId, onFocus, onRecord, onCorrespondence, label, emptyMessage, groupedOverview, packIsolated, space, sharedWorld, affinities = [], actions, panel, emptyAction, fitKey }: {
  graph: MemoryGraph;
  focusId: string | null;
  onFocus: (node: GraphNode | null) => void;
  onRecord: (id: string) => void;
  label: string;
  emptyMessage: string;
  groupedOverview?: boolean;
  packIsolated?: boolean;
  sharedWorld?: boolean;
  affinities?: readonly MemorySimilarity[];
  actions?: ReactNode;
  panel?: ReactNode;
  emptyAction?: ReactNode;
  fitKey?: string;
  onCorrespondence?: (id: string) => void;
  space?: (projection: CanvasProjection) => ReactNode;
}) {
  const [visible, setVisible] = useState({ focusId, limit: PAGE_SIZE });
  const [cameraToolbar, setCameraToolbar] = useState<HTMLDivElement | null>(null);
  const [evidenceId, setEvidenceId] = useState<string | null>(null);
  const viewport = useRef<Viewport | undefined>(undefined);
  const flow = useRef<ReactFlowInstance | undefined>(undefined);
  const canvasRef = useRef<HTMLDivElement>(null);
  const viewingSpace = useRef(false);
  viewingSpace.current = Boolean(space);
  const [zoomBand, setZoomBand] = useState<"far" | "middle" | "near">("near");
  function rememberViewport(value: Viewport) {
    viewport.current = value;
    setZoomBand(value.zoom < .48 ? "far" : value.zoom < .8 ? "middle" : "near");
  }
  const [move, setMove] = useState(false);
  const [offsets, setOffsets] = useState(new Map<string, {x: number; y: number}>());
  const [anchors, setAnchors] = useState(new Map<string, {x: number; y: number}>());
  const [shownCount, setShownCount] = useState(PAGE_SIZE);
  const lastFitKey = useRef(fitKey);
  const focus = focusId ? graph.nodes.get(focusId) : undefined;
  const limit = visible.focusId === focusId ? visible.limit : PAGE_SIZE;
  const neighbors = focus ? graph.edges.filter(edge => edge.source === focus.id || edge.target === focus.id) : [];
  const visibleNeighbors = neighbors.slice(0, limit);
  const neighborhood = new Set(focus ? [focus.id, ...visibleNeighbors.flatMap(edge => [edge.source, edge.target])] : []);
  const overview = graph.overview.slice(0, sharedWorld ? shownCount : limit);
  const shown = focus ? [...new Set([...neighborhood, ...(sharedWorld ? overview : [])])] : overview;
  const shownSet = new Set(shown);
  const visibleEdges = graph.edges.filter(edge => shownSet.has(edge.source) && shownSet.has(edge.target));
  const selectedEdge = graph.edges.find(edge => edge.id === evidenceId);
  const evidence = selectedEdge?.evidence.type === "citation" ? selectedEdge.evidence : null;

  function focusNode(node: GraphNode | null) {
    onFocus(node);
    setVisible({ focusId: node?.id ?? null, limit: PAGE_SIZE });
  }

  function openEvidence(edge: GraphEdge) {
    if (edge.evidence.type === "memory") onRecord(edge.evidence.id);
    else if (edge.evidence.type === "correspondence" || edge.evidence.type === "extracted") onCorrespondence?.(edge.evidence.id);
    else setEvidenceId(edge.id);
  }

  const world = useMemo(() => sharedWorld
    ? layoutMemoryWorld([...new Set([...graph.overview, ...graph.edges.flatMap(edge => [edge.source,edge.target])])], graph.edges, affinities)
    : null, [sharedWorld, graph, affinities]);
  useEffect(() => { if (world) setAnchors(previous => keepMemoryAnchors(previous, world)); }, [world]);
  useEffect(() => {
    if (lastFitKey.current === fitKey) return;
    lastFitKey.current = fitKey;
    const frame = requestAnimationFrame(() => {
      void flow.current?.fitView({maxZoom:1.42,padding:.12});
    });
    return () => cancelAnimationFrame(frame);
  }, [fitKey]);
  useEffect(() => {
    if (!focusId || viewingSpace.current || !sharedWorld) return;
    const frame = requestAnimationFrame(() => {
      const instance = flow.current;
      const selected = instance?.getNode(focusId);
      if (!instance || !selected || viewingSpace.current) return;
      const viewport = instance.getViewport();
      const width = selected.measured?.width ?? selected.initialWidth ?? memoryNodeSize.width;
      const height = selected.measured?.height ?? selected.initialHeight ?? memoryNodeSize.height;
      const point = instance.flowToScreenPosition({x:selected.position.x + width / 2,y:selected.position.y + height / 2});
      const canvas = canvasRef.current?.getBoundingClientRect();
      if (canvas && (point.x < canvas.left + 80 || point.x > canvas.right - 80 || point.y < canvas.top + 42 || point.y > canvas.bottom - 42)) {
        void instance.setCenter(selected.position.x + width / 2, selected.position.y + height / 2, {zoom:viewport.zoom});
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [focusId, sharedWorld]);
  const fixed = useMemo(() => world && keepMemoryAnchors(anchors, world), [anchors, world]);
  const canRestore = offsets.size > 0 || !!world && [...world].some(([id, position]) => {
    const retained = fixed?.get(id);
    return retained && (retained.x !== position.x || retained.y !== position.y);
  });
  if (!graph.overview.length) return <EmptyState title={emptyMessage} action={emptyAction} />;
  const groupedPositions = !fixed && groupedOverview && !focus ? layoutRelationshipOverview(shown, visibleEdges, packIsolated) : null;

  const nodes: Node[] = shown.map((id, index) => {
    const node = graph.nodes.get(id)!;
    const size = node.kind === "memory" ? memoryNodeSize : {width:180,height:60};
    return {
      id,
      initialWidth: sharedWorld ? size.width : undefined,
      initialHeight: sharedWorld ? size.height : undefined,
      position: offsets.get(id) ?? fixed?.get(id) ?? groupedPositions?.get(id) ?? (focus
        ? index === 0 ? { x: 0, y: Math.floor((shown.length - 1) / 2) * 84 } : { x: 295, y: (index - 1) * 84 }
        : { x: index % 3 * 235, y: Math.floor(index / 3) * 90 }),
      sourcePosition: Position.Right,
      targetPosition: Position.Left,
      draggable: sharedWorld && node.kind === "memory" && move && !space,
      dragHandle: ".memory-star-core",
      className: sharedWorld ? (node.kind === "memory" ? "memory-world-node" : "memory-world-entity") : node.kind === "category" ? "memory-group-node" : node.kind === "person" ? "memory-person-node" : undefined,
      style: sharedWorld ? { width: size.width, height: size.height, opacity: focus && !neighborhood.has(id) ? .38 : 1 } : { width: 182 },
      data: { label: <button className={`${move ? "" : "nodrag "}nopan memory-network-node-label`} type="button"
        aria-label={`${kinds[node.kind]} ${node.label}${node.preview ? " " + node.preview : ""}`} aria-pressed={id === focus?.id} onClick={() => focusNode(node)}>
        {sharedWorld && node.kind === "memory" ? <>
          <span className="memory-star-core" data-rings={versionRings(node.changes)} data-revision={node.revision} aria-hidden="true">
            {Array.from({length:versionRings(node.changes)}, (_, index) => <i key={index} style={{inset:-3 - index * 4}} />)}
            {node.revision === undefined ? memoryStarGlyph[node.category ?? ""] ?? "✦" : "V" + node.revision}
          </span>
          <span className="memory-star-content">
            <strong>{node.label}</strong>
          </span>
          {node.pending && <small className="memory-orbit-status">待确认</small>}
        </> : <>{node.label}{node.status && node.status !== node.label ? <small>{node.status}</small> : (groupedPositions || sharedWorld) && <small>{kinds[node.kind]}</small>}</>}
      </button> },
    };
  });
  const drawnNodes = new Map(nodes.map(node => [node.id,node]));
  const center = (node: Node) => ({x:node.position.x + (node.initialWidth ?? 0) / 2,y:node.position.y + (node.initialHeight ?? 0) / 2,z:0});
  const radius = (node: Node) => node.className === "memory-world-node" ? 18 : 32;
  const edges: Edge[] = visibleEdges.map(edge => {
    const left = offsets.get(edge.source) ?? fixed?.get(edge.source);
    const right = offsets.get(edge.target) ?? fixed?.get(edge.target);
    const reverse = !!left && !!right && !edge.directed && left.x > right.x;
    const incident = !focus || edge.source === focus.id || edge.target === focus.id;
    const enoughRoom = !left || !right || Math.hypot(left.x - right.x, left.y - right.y) >= 120;
    const sourceNode = drawnNodes.get(reverse ? edge.target : edge.source);
    const targetNode = drawnNodes.get(reverse ? edge.source : edge.target);
    const control = sharedWorld && sourceNode && targetNode ? {
      from:center(sourceNode),to:center(targetNode),fromRadius:radius(sourceNode),toRadius:radius(targetNode),
    } : undefined;
    return {
    id: edge.id,
    type: sharedWorld ? "memoryCurve" : undefined,
    data:control,
    source: reverse ? edge.target : edge.source,
    target: reverse ? edge.source : edge.target,
    label: sharedWorld && (!incident || !enoughRoom || (!focus && visibleEdges.length > 2 && edge.kind === "correspondence") ||
      (!focus && (zoomBand === "far" || visibleEdges.length > 8)))
      ? undefined : edgeLabel(edge, groupedOverview === true && !focus),
    labelStyle: sharedWorld && !enoughRoom ? { fontSize: 10.5 } : undefined,
    ariaLabel: `${graph.nodes.get(edge.source)?.label}，${edge.label}，${graph.nodes.get(edge.target)?.label}`,
    markerEnd: edge.directed ? arrow : undefined,
    className: edge.inferred ? "memory-edge-pending" : undefined,
    style: { strokeWidth: incident ? 1.8 : 1.2, strokeLinecap:"round", opacity: incident ? 1 : .3, strokeDasharray: edge.inferred ? "5 4" : undefined },
  }; });

  return <section className="memory-network-workspace" aria-label={label}>
    <div className="memory-network-toolbar">
      {focus && <Button variant="secondary" size="sm" onClick={() => focusNode(null)}>返回整体</Button>}
      {(focus || !space) && <span role="status">{focus ? `${kinds[focus.kind]} · ${focus.label} · ${neighbors.length} 条直接关系` : `${graph.overview.length} 个节点 · 选择节点查看直接关系`}</span>}
      {focus?.recordId && <Button size="sm" variant="ghost" onClick={() => onRecord(focus.recordId!)}>查看详情</Button>}
      {focus?.href && <Link className="memory-reference-link" to={focus.href}>{focus.kind === "document" ? "打开原文" : "打开来源对话"}</Link>}
      {actions}
      {sharedWorld && !space && <div className="memory-network-actions">
        <Button size="sm" variant="ghost" aria-pressed={move} onClick={() => setMove(!move)}>移动节点</Button>
        <Button size="sm" variant="ghost" onClick={() => {
          const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
          void flow.current?.fitView({duration:reduce ? 0 : 180,maxZoom:1.42,padding:.12});
        }}>适合画面</Button>
        <Button size="sm" variant="ghost" disabled={!canRestore} onClick={() => { setOffsets(new Map()); setAnchors(world ?? new Map()); }}>恢复自动布局</Button>
      </div>}
      {(sharedWorld ? shownCount < graph.overview.length || !!focus && limit < neighbors.length : limit < (focus ? neighbors.length : graph.overview.length))
        && <Button variant="secondary" size="sm" onClick={() => { setVisible({ focusId, limit: limit + PAGE_SIZE }); if (sharedWorld) setShownCount(count => count + PAGE_SIZE); }}>展开更多</Button>}
      <div ref={setCameraToolbar} className="memory-network-camera-toolbar" hidden={!space} />
    </div>
    {move && <small className="memory-semantic-hint" style={space ? {visibility:"hidden"} : undefined}>拖动星核调整位置。布局偏移在当前图谱页面内保留。</small>}
    <div className="memory-inspect-workbench">
    <div ref={canvasRef} className="memory-network" data-memory-world={sharedWorld ? "true" : undefined} data-memory-zoom={zoomBand} data-memory-move={move}>
      <div className={space ? "memory-network-flat memory-network-flat-hidden" : "memory-network-flat"} aria-hidden={space ? true : undefined}
        ref={element => { if (element) element.inert = Boolean(space); }}>
      <ReactFlow key={sharedWorld ? "world" : `${focus?.id ?? "overview"}:${limit}:${shown.length}`} nodes={nodes} edges={edges} fitView edgeTypes={memoryEdgeTypes}
        defaultViewport={sharedWorld ? viewport.current : undefined}
        fitViewOptions={{ maxZoom: sharedWorld ? 1.42 : 1.1, padding: sharedWorld ? .12 : .2 }}
        onInit={instance => { flow.current = instance; rememberViewport(instance.getViewport()); }}
        onMove={(_, value) => rememberViewport(value)}
        nodesDraggable={Boolean(sharedWorld && move && !space)} nodeDragThreshold={5} nodesConnectable={false} nodesFocusable={false} edgesFocusable={false}
        onNodesChange={changes => {
          const positions = changes.flatMap(change => change.type === "position" && change.position ? [[change.id, change.position] as const] : []);
          if (positions.length) setOffsets(previous => new Map([...previous, ...positions]));
        }}
        onEdgeClick={(_, edge) => { const selected = graph.edges.find(candidate => candidate.id === edge.id); if (selected) openEvidence(selected); }}>
        {!sharedWorld && <Background />}
        <Controls showInteractive={false} />
      </ReactFlow>
      </div>
      {space?.({ positions: new Map(nodes.map(node => [node.id, sharedWorld ? {x:node.position.x + (node.initialWidth ?? 0)/2,y:node.position.y + (node.initialHeight ?? 0)/2} : node.position])), edges: visibleEdges,
        nodes:shown.map(id => graph.nodes.get(id)!), shared:sharedWorld, viewport: flow.current?.getViewport() ?? viewport.current, cameraToolbar })}
    </div>
    {panel}
    </div>
    {focus && neighbors.length > 0 && <details className="memory-network-relations" style={space ? {visibility:"hidden"} : undefined}>
      <summary>直接关系列表</summary>
      <ul>{visibleNeighbors.map(edge => <li key={edge.id}>
        <button type="button" onClick={() => focusNode(graph.nodes.get(edge.source === focus.id ? edge.target : edge.source) ?? null)}>
          {graph.nodes.get(edge.source)?.label} {edge.directed ? "→" : "·"} {edge.label} {edge.directed ? "→" : "·"} {graph.nodes.get(edge.target)?.label}
          {edge.kind in relationshipKinds && ` · ${relationshipKinds[edge.kind as keyof typeof relationshipKinds]}`}
          {edge.inferred && " · 待确认"}
        </button>
        <button type="button" onClick={() => openEvidence(edge)}>{edge.occurredAt || formatMemoryTime(edge.evidence.time)} · 查看关系依据</button>
      </li>)}</ul>
    </details>}
    {evidence && <Modal open title="引用依据" size="md" onClose={() => setEvidenceId(null)}>
      <div className="memory-activity-detail"><Badge>{evidence.status}</Badge>
        {evidence.excerpt && <p>{evidence.excerpt}</p>}
        {evidence.href && <Link to={evidence.href} className="memory-reference-link">定位原文</Link>}
      </div>
    </Modal>}
  </section>;
}
