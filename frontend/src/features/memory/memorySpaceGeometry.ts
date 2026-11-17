import { memorySpatialCurve, splitMemorySpatialCurve, type MemorySpatialCurve } from "./memorySpatialCurve";

type Point3 = { x: number; y: number; z: number };
type ScenePoint = Point3 & { id: string; memoryId: string; layer: number; base: boolean; current: boolean; auxiliary?: boolean; historyOnly?: boolean };
type Line = { id: string; source: string; target: string };
type GeometryItem = {
  id: string; x: number; y: number; versions: { version_id: string; layer: number }[];
  auxiliary?: boolean; cardWidth?: number; cardHeight?: number;
  currentVersionId?: string | null;
  historyOnly?: boolean;
};
type GeometryEdge = Line & { sourceVersionId?: string; targetVersionId?: string; event?: boolean; pending?: boolean; directed?: boolean };
type Size = { width: number; height: number };
export type SpaceCamera = { azimuth: number; elevation: number; zoom: number; panX: number; panY: number; perspective: number };
type Projected = { x: number; y: number; depth: number; scale: number };

export const LAYER_SPACING = 112;
const ORBIT_SPEED = 0.007;
const BASE_REVEAL_START = .17;

export const memoryVersionRadius = (point: ScenePoint) => point.auxiliary ? 6 : point.current ? 17 : point.base ? 12 : 8;

export const defaultSpaceCamera: SpaceCamera = { azimuth: -0.12, elevation: 1.02, zoom: 1, panX: 0, panY: 0, perspective: .42 };
export const frontSpaceCamera: SpaceCamera = { ...defaultSpaceCamera, azimuth: 0, elevation: 0, perspective: 0 };

export function compileMemorySpace(items: GeometryItem[], edges: GeometryEdge[]) {
  const points: ScenePoint[] = [];
  const risers: Line[] = [];
  const links: Line[] = [];
  const heads = new Map<string, string>();
  const cards = new Map<string, Size>();
  const owners = new Map<string, string>();
  const curves = new Map<string, { path: MemorySpatialCurve; atBase: boolean; orbitHull: readonly Point3[] }>();
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity, maxZ = 0;
  for (const item of items) {
    const versions = [...item.versions].sort((a, b) => a.layer - b.layer || a.version_id.localeCompare(b.version_id));
    if (!versions.length && !item.auxiliary) continue;
    const current = item.auxiliary ? item.id
      : versions.find(version => version.version_id === item.currentVersionId)?.version_id;
    if (current) {
      heads.set(item.id, current);
      cards.set(current, { width: item.cardWidth ?? (item.auxiliary ? 180 : 252), height: item.cardHeight ?? (item.auxiliary ? 60 : 88) });
    }
    minX = Math.min(minX, item.x);
    minY = Math.min(minY, item.y);
    maxX = Math.max(maxX, item.x);
    maxY = Math.max(maxY, item.y);
    if (item.auxiliary) {
      points.push({ id: item.id, memoryId: item.id, x: item.x, y: item.y, z: 0, layer: 0, base: false, current: true, auxiliary: true });
      owners.set(item.id, item.id);
      continue;
    }
    for (const [index, { version_id: id, layer }] of versions.entries()) {
      const z = layer * LAYER_SPACING;
      maxZ = Math.max(maxZ, z);
      points.push({ id, memoryId: item.id, x: item.x, y: item.y, z, layer, base: layer === 0, current: id === current, historyOnly:item.historyOnly });
      owners.set(id, item.id);
      if (index && layer === versions[index - 1].layer + 1) {
        const source = versions[index - 1].version_id;
        risers.push({ id: `version:${source}:${id}`, source, target: id });
      }
    }
  }
  const byId = new Map(points.map(point => [point.id, point]));
  for (const edge of edges) {
    const source = edge.sourceVersionId ?? heads.get(edge.source);
    const target = edge.targetVersionId ?? heads.get(edge.target);
    if (source && target && owners.get(source) === edge.source && owners.get(target) === edge.target) {
      links.push({ id: edge.id, source, target });
      const from = byId.get(source)!, to = byId.get(target)!;
      const atBase = from.base && to.base && !edge.event && !edge.pending;
      const path = memorySpatialCurve(from, to, edge.event ? "event" : atBase ? "base" : "relation", edge.id);
      const orbitHull = edge.directed ? path : splitMemorySpatialCurve(path).flatMap(half => splitMemorySpatialCurve(half).flat());
      curves.set(edge.id, { atBase, path, orbitHull });
    }
  }
  if (!points.length) minX = minY = maxX = maxY = 0;
  const floor = { minX: minX - 90, maxX: maxX + 90, minY: minY - 64, maxY: maxY + 64 };
  const center = { x: (minX + maxX) / 2, y: (minY + maxY) / 2, z: maxZ / 2 };
  const focalLength = Math.max(480, Math.hypot(maxX - minX + 180, maxY - minY + 128, maxZ) * 1.9);
  return { points, risers, links, curves, cards, floor, center, focalLength, maxLayer: maxZ / LAYER_SPACING };
}

export type MemorySpaceScene = ReturnType<typeof compileMemorySpace>;

function wrapAngle(angle: number) {
  return Math.atan2(Math.sin(angle), Math.cos(angle));
}

export function flatMemoryCamera(scene: MemorySpaceScene, size: Size, transform: { x: number; y: number; zoom: number }): SpaceCamera {
  return {
    ...frontSpaceCamera,
    zoom: transform.zoom,
    panX: transform.x + scene.center.x * transform.zoom - size.width / 2,
    panY: transform.y + scene.center.y * transform.zoom - size.height / 2,
  };
}

export function interpolateMemoryCamera(from: SpaceCamera, to: SpaceCamera, progress: number): SpaceCamera {
  if (progress <= 0) return from;
  if (progress >= 1) return to;
  const linear = (start: number, end: number) => start + (end - start) * progress;
  const angle = (start: number, end: number) => start + wrapAngle(end - start) * progress;
  return {
    azimuth: angle(from.azimuth, to.azimuth),
    elevation: angle(from.elevation, to.elevation),
    zoom: linear(from.zoom, to.zoom),
    panX: linear(from.panX, to.panX),
    panY: linear(from.panY, to.panY),
    perspective: linear(from.perspective, to.perspective),
  };
}

export function transitionMemorySpace(start: number, end: number, elapsedMs: number) {
  const duration = 660 * Math.abs(end - start);
  if (duration === 0 || elapsedMs >= duration) return end;
  const t = Math.max(0, elapsedMs / duration);
  return start + (end - start) * t * t * (3 - 2 * t);
}

export function memorySpaceReveal(progress: number) {
  const ramp = (start: number, end: number) => Math.max(0, Math.min(1, (progress - start) / (end - start)));
  const base = ramp(BASE_REVEAL_START, .31);
  return {
    projection: 1 - base,
    base,
    growth: ramp(.34, .92),
    upper: ramp(.34, .55),
  };
}

export function orbitMemorySpace(camera: SpaceCamera, deltaX: number, deltaY: number): SpaceCamera {
  return { ...camera, azimuth: wrapAngle(camera.azimuth + deltaX * ORBIT_SPEED),
    elevation: Math.max(.24, Math.min(1.45, camera.elevation + deltaY * ORBIT_SPEED)) };
}

function planeProjection(scene: MemorySpaceScene, camera: SpaceCamera) {
  const sinAz = Math.sin(camera.azimuth), cosAz = Math.cos(camera.azimuth);
  const sinEl = Math.sin(camera.elevation), cosEl = Math.cos(camera.elevation);
  return (point: Point3): Projected => {
    const x = point.x - scene.center.x;
    const y = point.y - scene.center.y;
    const z = point.z - scene.center.z;
    const horizontal = x * cosAz - y * sinAz;
    const ordinate = x * sinAz + y * cosAz;
    const depth = ordinate * sinEl + z * cosEl;
    const scale = scene.focalLength / (scene.focalLength - depth * camera.perspective);
    return { x: horizontal * scale, y: (ordinate * cosEl - z * sinEl) * scale, scale, depth };
  };
}

function softMinimum(value: number, limit: number, band: number) {
  if (band <= 0) return Math.min(value, limit);
  const overlap = Math.max(0, band - Math.abs(value - limit)) / band;
  return Math.min(value, limit) - band * overlap ** 2 / 4;
}

function constrainMemorySpace(scene: MemorySpaceScene, camera: SpaceCamera, size: Size, inset: [number, number], growth: number, orbit: boolean): SpaceCamera {
  if (!scene.points.length) return camera;
  const project = planeProjection(scene, camera);
  const depthFactor = camera.perspective / scene.focalLength;
  let left = Infinity, right = -Infinity, top = Infinity, bottom = -Infinity;
  const include = (anchor: Point3, radius: number) => {
    const point = project({ ...anchor, z: anchor.z * growth });
    const x = point.x / point.scale, y = point.y / point.scale;
    // A sphere's lateral and depth offsets are coupled. Solve its projected
    // extrema directly so empty corners of a bounding box don't force zoom.
    const w = 1 / point.scale;
    const depthRadius = radius * depthFactor;
    const denominator = w * w - depthRadius * depthRadius;
    const halfExtent = (axis: number) => radius *
      Math.sqrt(denominator + (depthFactor * axis) ** 2) / denominator;
    const centerX = w * x / denominator, centerY = w * y / denominator;
    const extentX = halfExtent(x), extentY = halfExtent(y);
    left = Math.min(left, centerX - extentX);
    right = Math.max(right, centerX + extentX);
    top = Math.min(top, centerY - extentY);
    bottom = Math.max(bottom, centerY + extentY);
  };
  for (const point of scene.points) include(point, memoryVersionRadius(point));
  // Bend controls bound both the projected cubic and its WebGL tube. Version
  // stems are straight and already bounded by their endpoint spheres.
  for (const { path, orbitHull } of scene.curves.values()) {
    for (const point of orbit ? orbitHull : path) include(point, 4.5);
  }
  const width = size.width, height = Math.max(1, size.height - inset[0] - inset[1]);
  const padding = orbit ? 4 : 10;
  const marginX = Math.min(padding, width * .04), marginY = Math.min(padding, height * .04);
  const spanX = Math.max(1, right - left), spanY = Math.max(1, bottom - top);
  const limit = Math.min((width - 2 * marginX) / spanX, (height - 2 * marginY) / spanY);
  const zoom = softMinimum(camera.zoom, limit, orbit ? 5 / Math.max(spanX, spanY) : 0);
  const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
  const pan = (value: number, min: number, max: number) => {
    const band = orbit ? Math.min(5, Math.max(0, (max - min) / 2)) : 0;
    return clamp(softMinimum(-softMinimum(-value, -min, band), max, band), min, max);
  };
  return { ...camera, zoom,
    panX: pan(camera.panX, marginX - width / 2 - left * zoom, width / 2 - marginX - right * zoom),
    panY: pan(camera.panY, inset[0] + marginY - size.height / 2 - top * zoom,
      size.height / 2 - inset[1] - marginY - bottom * zoom),
  };
}

export function keepMemorySpaceInFrame(scene: MemorySpaceScene, camera: SpaceCamera, size: Size, inset: [number, number], growth: number) {
  return constrainMemorySpace(scene, camera, size, inset, growth, false);
}

export function frameMemorySpaceOrbit(scene: MemorySpaceScene, camera: SpaceCamera, size: Size, inset: [number, number]) {
  return constrainMemorySpace(scene, camera, size, inset, 1, true);
}

export function frameMemorySpaceTransition(scene: MemorySpaceScene, camera: SpaceCamera, size: Size, inset: [number, number], progress: number) {
  if (progress === 0) return camera;
  const framed = keepMemorySpaceInFrame(scene, camera, size, inset, memorySpaceReveal(progress).growth);
  const t = Math.min(1, progress / BASE_REVEAL_START);
  return interpolateMemoryCamera(camera, framed, t * t * (3 - 2 * t));
}

export function fitMemorySpace(scene: MemorySpaceScene, camera: SpaceCamera, size: Size, inset: [number, number] = [0, 0]): SpaceCamera {
  if (!scene.points.length) return { ...camera, zoom: 1, panX: 0, panY: 0 };
  const project = planeProjection(scene, camera);
  let left = Infinity, top = Infinity, right = -Infinity, bottom = -Infinity;
  const reserveCards = camera.perspective === 0 && scene.cards.size <= 8;
  for (const point of scene.points) {
    const { x, y, scale } = project(point);
    const card = reserveCards ? scene.cards.get(point.id) : undefined;
    const radius = memoryVersionRadius(point);
    const w = (card ? card.width / 2 : radius) * scale;
    const h = (card ? card.height / 2 : radius) * scale;
    left = Math.min(left, x - w);
    top = Math.min(top, y - h);
    right = Math.max(right, x + w);
    bottom = Math.max(bottom, y + h);
  }
  const roomX = size.width - Math.min(80, size.width * .16);
  const usableHeight = Math.max(size.height - inset[0] - inset[1], size.height / 2);
  const topPadding = Math.min(26, usableHeight * .12), bottomPadding = Math.min(8, usableHeight * .04);
  const roomY = usableHeight - topPadding - bottomPadding;
  const zoom = Math.min(1.62, roomX / Math.max(right - left, 1), roomY / Math.max(bottom - top, 1));
  return { ...camera, zoom, panX: -(left + right) * zoom / 2, panY: -(top + bottom) * zoom / 2 + (inset[0] - inset[1] + topPadding - bottomPadding) / 2 };
}

function closestProjectedPoints(points: Projected[], limit: number) {
  let distance = limit;
  for (let i = 0; i < points.length; i++) {
    for (let j = i + 1; j < points.length; j++) {
      distance = Math.min(distance, Math.hypot(points[i].x - points[j].x, points[i].y - points[j].y));
    }
  }
  return distance;
}

export function composeMemorySpace(scene: MemorySpaceScene, size: Size, inset: [number, number]) {
  const base = scene.points.filter(point => point.base);
  let camera = fitMemorySpace(scene, defaultSpaceCamera, size, inset);
  if (base.length < 3 || base.length > 8 || size.width >= 1000) return camera;

  // Choose a viewing direction that separates the existing anchors on screen.
  const highest = scene.points.reduce((a, b) => a.z < b.z ? b : a);
  const foot = base.find(point => point.memoryId === highest.memoryId);
  const heads = scene.points.filter(point => point.current && !point.auxiliary);
  let best = -Infinity;
  for (const elevation of [.84, 1.02, 1.16]) {
    for (let step = 0; step < 16; step++) {
      const azimuth = wrapAngle(step * Math.PI / 8);
      const candidate = fitMemorySpace(scene, { ...defaultSpaceCamera, azimuth, elevation }, size, inset);
      const projected = projectMemorySpace(scene, candidate, size).points;
      const floor = base.map(point => projected.get(point.id)!);
      const spanX = Math.max(...floor.map(point => point.x)) - Math.min(...floor.map(point => point.x));
      const spanY = Math.max(...floor.map(point => point.y)) - Math.min(...floor.map(point => point.y));
      const separation = closestProjectedPoints(floor, 56);
      const headSeparation = closestProjectedPoints(heads.map(point => projected.get(point.id)!), 36);
      const rise = foot ? projected.get(foot.id)!.y - projected.get(highest.id)!.y : 0;
      const score = spanX + spanY * .18 + separation + rise * .55 + headSeparation * 5;
      if (score > best) {
        best = score;
        camera = candidate;
      }
    }
  }
  return camera;
}

export function zoomMemorySpace(camera: SpaceCamera, delta: number, anchorX: number, anchorY: number): SpaceCamera {
  const zoom = Math.max(0.025, Math.min(camera.zoom * Math.exp(-delta * 0.0015), 5));
  const ratio = zoom / camera.zoom;
  return { ...camera, zoom, panX: anchorX - (anchorX - camera.panX) * ratio, panY: anchorY - (anchorY - camera.panY) * ratio };
}

export function projectMemorySpace(scene: MemorySpaceScene, camera: SpaceCamera, size: Size, growth = 1) {
  const plane = planeProjection(scene, camera);
  const project = (point: Point3): Projected => {
    const p = plane({ ...point, z: point.z * growth });
    return {
      x: size.width / 2 + camera.panX + p.x * camera.zoom,
      y: size.height / 2 + camera.panY + p.y * camera.zoom,
      scale: p.scale * camera.zoom,
      depth: p.depth,
    };
  };
  return { points: new Map(scene.points.map(point => [point.id, project(point)])), project };
}
