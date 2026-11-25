import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { createPortal } from "react-dom";
import { Crosshair, Maximize, Mouse, Rotate3D, RotateCcw } from "lucide-react";
import { Button, EmptyState } from "../../components/ui";
import {
  compileMemorySpace, composeMemorySpace, fitMemorySpace, flatMemoryCamera, frameMemorySpaceOrbit, frameMemorySpaceTransition, frontSpaceCamera,
  interpolateMemoryCamera, keepMemorySpaceInFrame, memorySpaceReveal, memoryVersionRadius, orbitMemorySpace, projectMemorySpace,
  transitionMemorySpace, zoomMemorySpace, type SpaceCamera,
} from "./memorySpaceGeometry";
import { memorySpaceLineClearance, placeMemorySpaceEdgeLabel, placeMemorySpaceLabels } from "./memorySpaceLabels";
import { sampleMemorySpatialCurve } from "./memorySpatialCurve";
import MemorySpaceMeshes from "./MemorySpaceMeshes";
import "./memorySpaceView.css";

export type SpaceVersion = {
  version_id: string;
  revision: number;
  layer: number;
  baseline: boolean;
  content: string;
  source: string;
  source_ref?: string | null;
  source_conversation_id?: string | null;
  source_message_id?: string | null;
  source_removed?: boolean;
  status: string;
  recorded_at: number;
  reason: string;
  relationship_known?: boolean;
  relationship?: unknown;
  event_id?: string | null;
};
export type SpaceMemory = {
  id: string; label: string; x: number; y: number; versions: SpaceVersion[];
  caption?: string; categoryLabel?: string; preview?: string; cardWidth?: number; cardHeight?: number;
  auxiliary?: boolean; historyOnly?: boolean;
  currentVersionId?: string | null;
  category?: string;
};
export type SpaceEdge = {
  id: string; source: string; target: string; label: string; directed?: boolean; sourceVersionId?: string;
  targetVersionId?: string; historical?: boolean; pending?: boolean; event?: boolean;
};

type Props = {
  items: SpaceMemory[];
  edges: SpaceEdge[];
  focusedId: string | null;
  onFocus?: (id: string | null) => void;
  onVersion: (memoryId: string, version: SpaceVersion) => void;
  onEdge?: (id: string) => void;
  onEntity?: (id: string) => void;
  initialViewport?: { x: number; y: number; zoom: number };
  cameraToolbar?: HTMLElement | null;
  phase?: "enter" | "active" | "exit";
  onTransitionEnd?: (phase: "enter" | "exit") => void;
};

type CameraOverride = { preferred: SpaceCamera; frameOrbit?: true };

const clipText = (value: string, limit: number) => {
  const characters = [...value.trim().replace(/\s+/g, " ")];
  return characters.slice(0, limit).join("") + (characters.length > limit ? "…" : "");
};
const levelName = (version: SpaceVersion) => version.layer === 0
  ? version.baseline ? "迁移基线 · 此前历史未知" : "原始版本"
  : `第 ${version.layer} 次更改`;
const precision = (value: number) => Number(value.toFixed(2));
const characterWidth = (char: string, fontSize: number) => /[\u0000-\u007f]/.test(char) ? fontSize * .67 : fontSize;
const labelWidth = (value: string, fontSize = 14) => [...value].reduce((width, char) => width + characterWidth(char, fontSize), 0);
const DRAG_THRESHOLD = 4;

export default function MemorySpaceView({
  items, edges, focusedId, onFocus, onVersion, onEdge, onEntity, initialViewport, cameraToolbar, phase = "active", onTransitionEnd,
}: Props) {
  const region = useRef<HTMLDivElement>(null);
  const canvas = useRef<SVGSVGElement>(null);
  const drag = useRef<{ pointer: number; x: number; y: number; mask: number; moved: boolean; mode: "orbit" | "pan"; bounds: DOMRect } | null>(null);
  const suppressClick = useRef(false);
  const initial = useRef(initialViewport);
  const [size, setSize] = useState({ width: 1, height: 1 });
  const [hud, setHud] = useState<[number, number]>([0, 0]);
  const controls = useRef<HTMLDivElement>(null);
  const caption = useRef<HTMLDivElement>(null);
  const [progress, setProgress] = useState(phase === "enter" ? 0 : 1);
  const progressRef = useRef(progress);
  const [cameraOverride, setCamera] = useState<CameraOverride | null>(null);
  const [orbiting, setOrbiting] = useState(false);
  const [panning, setPanning] = useState(false);
  const [dragToOrbit, setDragToOrbit] = useState(false);
  const [hoveredVersion, setHoveredVersion] = useState<string | null>(null);
  const [keyFocusedVersion, setKeyFocusedVersion] = useState<string | null>(null);
  const [meshReady, setMeshReady] = useState<boolean | null>(null);
  const previousLabels = useRef<ReturnType<typeof placeMemorySpaceLabels>>(new Map());
  const scene = useMemo(() => compileMemorySpace(items, edges), [items, edges]);
  const byWorldId = useMemo(() => new Map(scene.points.map(point => [point.id, point])), [scene]);
  const itemById = useMemo(() => new Map(items.map(item => [item.id, item])), [items]);
  const edgeById = useMemo(() => new Map(edges.map(edge => [edge.id, edge])), [edges]);
  const versionById = useMemo(() => new Map(items.flatMap(item => item.versions.map(version => [version.version_id, version] as const))), [items]);
  const fittedCamera = useMemo(() => keepMemorySpaceInFrame(scene, composeMemorySpace(scene, size, hud), size, hud, 1), [scene, size, hud]);
  const flatCamera = useMemo(() => initial.current
    ? flatMemoryCamera(scene, size, initial.current)
    : fitMemorySpace(scene, frontSpaceCamera, size), [scene, size]);
  const resolveCamera = useCallback((override: CameraOverride | null) => {
    if (!override) return fittedCamera;
    return override.frameOrbit ? frameMemorySpaceOrbit(scene, override.preferred, size, hud) : override.preferred;
  }, [scene, size, hud, fittedCamera]);
  const spatialCamera = useMemo(() => resolveCamera(cameraOverride), [resolveCamera, cameraOverride]);
  const preferredCamera = interpolateMemoryCamera(flatCamera, spatialCamera, progress);
  const camera = progress < 1 && (phase === "enter" || !cameraOverride || cameraOverride.frameOrbit)
    ? frameMemorySpaceTransition(scene, preferredCamera, size, hud, progress) : preferredCamera;
  const reveal = memorySpaceReveal(progress);
  const { points, project } = useMemo(() => projectMemorySpace(scene, camera, size, reveal.growth), [scene, camera, size, reveal.growth]);
  const markers = useMemo(() => [...scene.points].sort((first, second) => points.get(first.id)!.depth - points.get(second.id)!.depth), [scene.points, points]);
  const stopOrbit = useRef<(() => void) | null>(null);
  const hasPoints = scene.points.length > 0;
  const transitionCallback = useRef(onTransitionEnd);
  transitionCallback.current = onTransitionEnd;
  const canInteract = phase === "active" && meshReady === true;
  const depthOpacity = reveal.base;

  useLayoutEffect(() => {
    const element = region.current;
    if (!element) return;
    const measure = () => {
      const width = element.clientWidth, height = element.clientHeight;
      if (width > 0 && height > 0) setSize(previous => previous.width === width && previous.height === height ? previous : { width, height });
      const top = cameraToolbar ? 0 : controls.current?.offsetHeight ?? 0;
      const bottom = caption.current?.offsetHeight ?? 0;
      setHud(previous => previous[0] === top && previous[1] === bottom ? previous : [top, bottom]);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    if (controls.current) observer.observe(controls.current);
    if (caption.current) observer.observe(caption.current);
    return () => observer.disconnect();
  }, [hasPoints, cameraToolbar]);

  useLayoutEffect(() => {
    if (phase === "active") {
      setProgress(1);
      progressRef.current = 1;
      return;
    }
    stopOrbit.current?.();
    const finalProgress = phase === "enter" ? 1 : 0;
    const fromProgress = progressRef.current;
    if (!hasPoints || window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      progressRef.current = finalProgress;
      setProgress(finalProgress);
      const frame = requestAnimationFrame(() => transitionCallback.current?.(phase));
      return () => cancelAnimationFrame(frame);
    }
    let start: number | undefined;
    let frame: number;
    const animate = (now: number) => {
      start ??= now;
      const next = transitionMemorySpace(fromProgress, finalProgress, now - start);
      progressRef.current = next;
      setProgress(next);
      if (next !== finalProgress) frame = requestAnimationFrame(animate);
      else frame = requestAnimationFrame(() => transitionCallback.current?.(phase));
    };
    frame = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(frame);
  }, [phase, hasPoints]);

  useEffect(() => {
    const svg = canvas.current;
    if (!svg) return;
    const finish = () => {
      const active = drag.current;
      if (!active) return;
      drag.current = null;
      if (active.moved && active.mask === 1) suppressClick.current = true;
      if (svg.hasPointerCapture(active.pointer)) svg.releasePointerCapture(active.pointer);
      setOrbiting(false);
      setPanning(false);
    };
    const beforeWindowHidden = () => { if (document.hidden) finish(); };
    stopOrbit.current = finish;
    window.addEventListener("blur", finish);
    document.addEventListener("visibilitychange", beforeWindowHidden);
    return () => {
      stopOrbit.current = null;
      window.removeEventListener("blur", finish);
      document.removeEventListener("visibilitychange", beforeWindowHidden);
      finish();
    };
  }, [hasPoints]);

  useEffect(() => {
    const svg = canvas.current;
    if (!svg || !canInteract) return;
    const wheel = (event: WheelEvent) => {
      event.preventDefault();
      const box = svg.getBoundingClientRect();
      const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? box.height : 1);
      setCamera(current => ({ preferred: zoomMemorySpace(resolveCamera(current), delta,
        event.clientX - box.left - box.width / 2, event.clientY - box.top - box.height / 2) }));
    };
    svg.addEventListener("wheel", wheel, { passive: false });
    return () => svg.removeEventListener("wheel", wheel);
  }, [resolveCamera, canInteract]);

  function rotateCamera(dx: number, dy: number) {
    // Keep the preferred scale so framing one angle never shrinks later angles.
    setCamera(current => ({ preferred: orbitMemorySpace(current?.preferred ?? fittedCamera, dx, dy), frameOrbit: true }));
  }

  function pointerDown(event: ReactPointerEvent<SVGSVGElement>) {
    if (!canInteract) return;
    suppressClick.current = false;
    const object = event.target instanceof Element && event.target.closest(".memory-space-marker,.memory-space-link");
    const mode = event.button === 1 || (event.button === 0 && dragToOrbit)
      ? "orbit" : event.button === 0 && !object ? "pan" : null;
    if (!mode) return;
    if (event.button === 1) {
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
    }
    event.stopPropagation();
    drag.current = {
      pointer: event.pointerId, x: event.clientX, y: event.clientY,
      mask: event.button === 1 ? 4 : 1, mode, moved: false, bounds: event.currentTarget.getBoundingClientRect(),
    };
  }

  function pointerMove(event: ReactPointerEvent<SVGSVGElement>) {
    const active = drag.current;
    if (!active || active.pointer !== event.pointerId) return;
    const { left, right, top, bottom } = active.bounds;
    if (!(event.buttons & active.mask) || event.clientX < left || event.clientX > right || event.clientY < top || event.clientY > bottom) {
      stopOrbit.current?.();
      return;
    }
    const dx = event.clientX - active.x, dy = event.clientY - active.y;
    if (!active.moved && Math.hypot(dx, dy) < DRAG_THRESHOLD) return;
    if (!active.moved) {
      active.moved = true;
      if (!event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.setPointerCapture(event.pointerId);
      if (active.mode === "orbit") setOrbiting(true);
      else setPanning(true);
    }
    event.preventDefault();
    active.x = event.clientX;
    active.y = event.clientY;
    if (!dx && !dy) return;
    if (active.mode === "orbit") rotateCamera(dx, dy);
    else setCamera(current => {
      const base = resolveCamera(current);
      return { preferred: { ...base, panX: base.panX + dx, panY: base.panY + dy } };
    });
  }

  function pointerFinish(event: ReactPointerEvent<SVGSVGElement>) {
    if (drag.current?.pointer === event.pointerId) stopOrbit.current?.();
  }

  const relationLines = scene.links.map(line => {
    const from = points.get(line.source)!, to = points.get(line.target)!;
    return { line, from, to, edge: edgeById.get(line.id)! };
  }).sort((a, b) => a.from.depth + a.to.depth - b.from.depth - b.to.depth);
  const markerLabels = markers.map(point => {
    const item = itemById.get(point.memoryId)!;
    const version = versionById.get(point.id);
    const projected = points.get(point.id)!;
    const isFocused = focusedId === item.id;
    const name = version ? levelName(version) : (item.caption || item.categoryLabel || "");
    const revealed = keyFocusedVersion === point.id || (!orbiting && !panning && hoveredVersion === point.id);
    const priority = point.current ? (isFocused ? 4 : 3) : 2;
    const title = clipText(point.current ? item.label : version ? `${item.label} · V${version.revision}` : name, size.width < 560 ? 14 : 22);
    const changes = point.current && version ? version.layer : 0;
    const width = labelWidth(title) + 14;
    const height = 22;
    const scale = Math.min(1.22, Math.max(.70, Math.sqrt(projected.scale)));
    return { point, item, version, projected, isFocused, name, title, priority, revealed, width, height, scale,
      changes, opacity: Math.max(point.base ? reveal.base : reveal.upper,
        point.current && !item.historyOnly ? reveal.projection : 0) };
  });
  const visibleMarkers = markerLabels.filter(item => item.opacity > .05);
  const titleScale = Math.min(1.16, Math.max(.74, Math.sqrt(camera.zoom)));
  const markerRadius = (point: typeof scene.points[number], scale: number, projectedScale: number) =>
    memoryVersionRadius(point) * projectedScale + (point.current && !point.auxiliary ? 5 * scale : 0);
  const candidates = visibleMarkers.filter(item => item.point.current || item.revealed)
    .map(({ point, projected, width, height, scale, priority, revealed }) =>
      ({ id: point.id, ...projected, width, height, scale:titleScale, priority, revealed, above: point.current,
        anchorPadding: (markerRadius(point, scale, projected.scale) + 7) / titleScale }));
  const occupiedMarkers = visibleMarkers.map(({ point, projected, scale }) => {
    const radius = markerRadius(point, scale, projected.scale);
    return { left:projected.x - radius, top:projected.y - radius, right:projected.x + radius, bottom:projected.y + radius };
  });
  const baseLineClearance = reveal.base > .015 ? [...scene.curves.values()].flatMap(({ atBase, path }) => atBase
    ? memorySpaceLineClearance(Array.from({length:33}, (_, step) => project(sampleMemorySpatialCurve(path, step / 32))))
    : []) : [];
  const labelObstacles = [...occupiedMarkers, ...baseLineClearance];
  const placedLabels = placeMemorySpaceLabels(candidates, size, labelObstacles, [hud[0] + 4, hud[1] + 4], previousLabels.current);
  useLayoutEffect(() => { previousLabels.current = placedLabels; }, [placedLabels]);
  const occupiedLabels = [...labelObstacles, ...Array.from(placedLabels.values(), item => item.bounds)];
  const memoryCount = items.filter(item => !item.auxiliary).length;

  if (!hasPoints) return <EmptyState title="暂无可查看的记忆版本" description="保存记忆后，可以在立体视图中查看它的版本层。" />;

  const cameraControls = <div className={`memory-space-controls${cameraToolbar ? " is-docked" : ""}`} ref={controls}
    style={{ opacity: depthOpacity }} aria-hidden={!canInteract}>
      <span><Mouse size={16} aria-hidden="true" /> {dragToOrbit ? "拖动旋转 · 滚轮缩放" : "中键旋转 · 空白拖动平移 · 滚轮缩放"}</span>
      <div>
        <Button size="sm" variant={dragToOrbit ? "secondary" : "ghost"} disabled={!canInteract} aria-pressed={dragToOrbit}
          title="没有鼠标中键时，打开后可用左键或触控拖动；画布聚焦后也可用方向键旋转"
          onClick={() => setDragToOrbit(value => !value)}><Rotate3D size={14} aria-hidden="true" />拖动旋转</Button>
        <Button size="sm" variant="ghost" disabled={!canInteract} onClick={() => setCamera({ preferred: fitMemorySpace(scene, frontSpaceCamera, size, hud) })}><Crosshair size={14} aria-hidden="true" />俯视</Button>
        <Button size="sm" variant="ghost" disabled={!canInteract} onClick={() => setCamera({ preferred: keepMemorySpaceInFrame(scene, fitMemorySpace(scene, camera, size, hud), size, hud, 1) })}><Maximize size={14} aria-hidden="true" />适合画面</Button>
        <Button size="sm" variant="ghost" disabled={!canInteract} onClick={() => setCamera(null)}><RotateCcw size={14} aria-hidden="true" />恢复倾角</Button>
      </div>
    </div>;

  return <section className="memory-space-workspace" aria-label="记忆网络立体视图"
    data-camera-azimuth={camera.azimuth.toFixed(4)} data-camera-elevation={camera.elevation.toFixed(4)}
    data-camera-zoom={camera.zoom.toFixed(4)} data-camera-pan-x={camera.panX.toFixed(2)} data-camera-pan-y={camera.panY.toFixed(2)}
    data-orbiting={orbiting} data-panning={panning} data-space-phase={phase}
    data-space-progress={progress.toFixed(4)} data-space-base={reveal.base.toFixed(4)} data-space-growth={reveal.growth.toFixed(4)}>
    {cameraToolbar ? createPortal(cameraControls, cameraToolbar) : cameraControls}
    <div className="memory-space-stage" ref={region}>
      <MemorySpaceMeshes scene={scene} camera={camera} size={size} edges={edges} progress={progress}
        focusedId={focusedId} onStatus={setMeshReady} />
      {meshReady === false && <div className="memory-space-unavailable" role="status">当前环境无法绘制三维模型，可返回二维查看记忆。</div>}
      <svg className="memory-space-canvas" ref={canvas} viewBox={`0 0 ${size.width} ${size.height}`} aria-label="三维版本画布：方向键旋转"
        style={meshReady === false ? {visibility:"hidden"} : undefined}
        tabIndex={canInteract ? 0 : -1}
        onKeyDown={event => {
          if (!canInteract || event.target !== event.currentTarget) return;
          const arrows: Record<string, [number, number]> = { ArrowLeft: [-23, 0], ArrowRight: [23, 0], ArrowUp: [0, -23], ArrowDown: [0, 23] };
          const delta = arrows[event.key];
          if (delta) { event.preventDefault(); rotateCamera(...delta); }
        }}
        onClickCapture={event => {
          if (!suppressClick.current || event.detail === 0) return;
          suppressClick.current = false;
          event.stopPropagation();
          event.preventDefault();
        }}
        onPointerDown={pointerDown} onPointerMove={pointerMove} onPointerUp={pointerFinish} onPointerCancel={pointerFinish}
        onLostPointerCapture={pointerFinish} onPointerLeave={pointerFinish}
        onAuxClick={event => { if (event.button === 1) event.preventDefault(); }}>
        {relationLines.map(({ line, from, to, edge }) => {
          const distance = Math.hypot(to.x - from.x, to.y - from.y);
          const fromWorld = byWorldId.get(line.source), toWorld = byWorldId.get(line.target);
          if (!fromWorld || !toWorld || !edge) return null;
          const {atBase, path: curve} = scene.curves.get(line.id)!;
          const projectedRelationship = fromWorld.current && toWorld.current && !fromWorld.historyOnly &&
            !toWorld.historyOnly && !edge.historical && !edge.event;
          const [a,b,c,d] = curve.map(point => {
            const screen = project(point);
            return precision(screen.x) + " " + precision(screen.y);
          });
          const path = `M ${a} C ${b} ${c} ${d}`;
          const lineClass = edge.event ? "memory-space-link is-event" : edge.pending ? "memory-space-link is-pending" : edge.historical ? "memory-space-link is-historical" : "memory-space-link";
          const readableLabel = edge.event ? `${edge.label} · 已确认联动`
            : `${edge.label || "对应关系"}${edge.historical ? " · 历史关系" : edge.pending ? " · 待确认" : ""}`;
          const related = !focusedId || edge.source === focusedId || edge.target === focusedId;
          const edgeOpacity = Math.max(atBase ? reveal.base : reveal.upper, projectedRelationship ? reveal.projection : 0) * (related ? 1 : .42);
          const label = `${clipText(edge.label, 12)}${edge.pending ? " · 待确认" : ""}`;
          const middle = project(sampleMemorySpatialCurve(curve,.5));
          const labelX = middle.x, labelY = middle.y - 8;
          const labelWidthOnScreen = labelWidth(label, 12) + 4;
          const showLabel = !edge.historical && edge.label &&
            (focusedId ? related && camera.zoom >= .68 : relationLines.length < 8) &&
            distance >= Math.max(110, labelWidthOnScreen + 24) &&
            placeMemorySpaceEdgeLabel(labelX, labelY, labelWidthOnScreen, occupiedLabels, size, hud);
          return <g key={line.id} data-edge-id={edge.id} data-event-edge={edge.event ? "true" : undefined}
            data-base-relationship={atBase ? "true" : undefined} className={lineClass}
            opacity={edgeOpacity} pointerEvents={canInteract && edgeOpacity > .1 ? undefined : "none"}
            role={canInteract && onEdge ? "button" : undefined} tabIndex={canInteract && onEdge ? 0 : undefined}
            aria-label={readableLabel}
            onClick={onEdge ? () => onEdge(edge.id) : undefined}
            onKeyDown={onEdge ? event => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); onEdge(edge.id); } } : undefined}>
            <title>{readableLabel}</title>
            {onEdge && <path className="memory-space-link-hit" d={path} fill="none" />}
            {showLabel && <text x={precision(labelX)} y={precision(labelY)} textAnchor="middle">{label}</text>}
          </g>;
        })}
        {markerLabels.map(({ point, item, version, projected, isFocused, name, scale, opacity, changes }) => {
          const className = `memory-space-marker${point.current ? " is-current" : ""}${isFocused ? " is-focused" : ""}${version && version.status !== "active" ? " is-inactive" : ""}${point.auxiliary ? " is-auxiliary" : ""}`;
          const open = () => {
            if (!canInteract) return;
            if (version) { onFocus?.(item.id); onVersion(item.id, version); }
            else onEntity?.(item.id);
          };
          const clickable = canInteract && opacity > .04 && Boolean(version || onEntity);
          const radius = memoryVersionRadius(point);
          const screenRadius = radius * projected.scale / scale;
          return <g key={point.id} transform={`translate(${precision(projected.x)},${precision(projected.y)}) scale(${precision(scale)})`}
            opacity={opacity} pointerEvents={clickable ? undefined : "none"}
            className={className} role={clickable ? "button" : undefined} tabIndex={clickable ? 0 : undefined}
            aria-label={version ? `${item.label} · V${version.revision} · ${name}` : item.label}
            data-version-id={version ? point.id : undefined} data-memory-id={version ? item.id : undefined} data-layer={version?.layer} data-depth={point.z} data-current={point.current}
            data-entity-id={point.auxiliary ? item.id : undefined} data-changes={point.current && !point.auxiliary ? changes : undefined}
            data-memory-base={point.base ? "true" : undefined} data-world-x={point.x} data-world-y={point.y}
            data-screen-x={precision(projected.x)} data-screen-y={precision(projected.y)}
            onPointerEnter={() => setHoveredVersion(point.id)} onPointerLeave={() => setHoveredVersion(null)}
            onFocus={() => setKeyFocusedVersion(point.id)} onBlur={() => setKeyFocusedVersion(null)}
            onClick={open} onKeyDown={event => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); open(); } }}>
            <title>{item.label}{name && ` · ${name}`}{version?.source_removed ? " · 来源已移除" : ""}</title>
            {point.current && !point.auxiliary && <circle className="memory-space-current-ring" r={screenRadius + 4} />}
            <circle className="memory-space-marker-dot" r={screenRadius} />
            <circle className="memory-space-marker-hit" r={Math.max(10, radius * projected.scale + 4) / scale} />
            {point.current && version && <text className="memory-space-core-revision"
              dominantBaseline="central" textAnchor="middle" aria-hidden="true"
              opacity={Math.max(0, Math.min(1, (projected.scale - .26) / .14))}
              style={{fontSize:Math.min(14, Math.max(8, (version.revision >= 100 ? 9 : 11) * projected.scale / scale))}}>V{version.revision}</text>}
          </g>;
        })}
        <g className="memory-space-labels" aria-hidden="true" pointerEvents="none">
          {markerLabels.map(({ point, title, projected, width, height, opacity }) => {
            const place = placedLabels.get(point.id);
            if (!place) return null;
            const connectorX = Math.max(place.x-6, Math.min(0, place.x-6+width));
            const connectorY = Math.max(place.y-14, Math.min(0, place.y-14+height));
            const connectorLength = Math.hypot(connectorX, connectorY);
            return <g key={point.id} data-label-for={point.id} opacity={opacity} transform={`translate(${precision(projected.x)},${precision(projected.y)}) scale(${titleScale})`}>
              {(!place.above || Math.abs(connectorX) > 2) && connectorLength > 6 && <path
                d={`M ${6*connectorX/connectorLength} ${6*connectorY/connectorLength} L ${connectorX} ${connectorY}`} className="memory-space-label-leader" />}
              <g className={`memory-space-marker-label${point.current ? " is-current" : ""}${place.above ? " is-above" : ""}`} transform={`translate(${place.x},${place.y})`}>
                <rect x="-6" y="-14" width={width} height={height} rx="7" />
                <text className="memory-space-marker-title" y="1">{title}</text>
              </g>
            </g>;
          })}
        </g>
      </svg>
    </div>
    <div className="memory-space-caption" ref={caption} style={{ opacity: depthOpacity }} aria-hidden={!canInteract}>
      <span>{memoryCount} 条记忆 · {scene.points.filter(point => !point.auxiliary).length} 个版本</span>
      <span className="memory-space-legend">
        <span><i className="memory-space-key-dot" /> 当前有效版本</span>
        <span><i className="memory-space-key-dot is-base" /> 基准层记录</span>
        <span><i className="memory-space-key-dot is-older" /> 历史版本</span>
        <span><i className="memory-space-key-riser" /> 版本轨迹</span>
        <span><i className="memory-space-key-link" /> 对应关系</span>
        {edges.some(edge => edge.pending) && <span><i className="memory-space-key-pending" /> 待确认</span>}
        {edges.some(edge => edge.event) && <span><i className="memory-space-key-event" /> 已确认联动</span>}
      </span>
    </div>
  </section>;
}
