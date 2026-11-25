import { memoryVersionRadius, type MemorySpaceScene } from "./memorySpaceGeometry";
import {
  sampleMemorySpatialCurve, type MemoryPoint3, type MemorySpatialCurve,
} from "./memorySpatialCurve";

type Color = readonly [number, number, number];
type SolidLink = { id: string; event?: boolean; historical?: boolean; pending?: boolean; directed?: boolean };

const longitudeSegments = 20;
const latitudeSegments = 14;
const tubeSegments = 40;
const tubeSides = 8;
export const MEMORY_MESH_STRIDE = 10;

function add(a: MemoryPoint3, b: MemoryPoint3): MemoryPoint3 {
  return { x: a.x + b.x, y: a.y + b.y, z: a.z + b.z };
}

function subtract(a: MemoryPoint3, b: MemoryPoint3): MemoryPoint3 {
  return { x: a.x - b.x, y: a.y - b.y, z: a.z - b.z };
}

function multiply(a: MemoryPoint3, factor: number): MemoryPoint3 {
  return { x: a.x * factor, y: a.y * factor, z: a.z * factor };
}

function cross(a: MemoryPoint3, b: MemoryPoint3): MemoryPoint3 {
  return { x: a.y * b.z - a.z * b.y, y: a.z * b.x - a.x * b.z, z: a.x * b.y - a.y * b.x };
}

function length(a: MemoryPoint3) {
  return Math.hypot(a.x, a.y, a.z);
}

function unit(a: MemoryPoint3): MemoryPoint3 {
  const lengthOfA = length(a);
  return lengthOfA > 0 ? multiply(a, 1 / lengthOfA) : { x: 0, y: 0, z: 1 };
}

function vertex(data: number[], point: MemoryPoint3, normal: MemoryPoint3, color: Color, anchorZ: number) {
  data.push(point.x, point.y, point.z, normal.x, normal.y, normal.z, ...color, anchorZ);
}

function sphere(data: number[], center: MemoryPoint3, radius: number, color: Color, compact: boolean) {
  const columns = compact ? 12 : longitudeSegments;
  const rows = compact ? 8 : latitudeSegments;
  const ring = (i: number, j: number) => {
    const latitude = (i / rows - 0.5) * Math.PI;
    const longitude = j / columns * Math.PI * 2;
    return {
      x: Math.cos(latitude) * Math.cos(longitude),
      y: Math.cos(latitude) * Math.sin(longitude),
      z: Math.sin(latitude),
    };
  };
  for (let i = 0; i < rows; i++) {
    for (let j = 0; j < columns; j++) {
      const a = ring(i, j), b = ring(i, j + 1), c = ring(i + 1, j), d = ring(i + 1, j + 1);
      for (const normal of [a, b, c, b, d, c]) {
        vertex(data, add(center, multiply(normal, radius)), normal, color, center.z);
      }
    }
  }
}

function curveTangent(curve: MemorySpatialCurve, t: number): MemoryPoint3 {
  const before = sampleMemorySpatialCurve(curve, Math.max(0, t - 0.003));
  const after = sampleMemorySpatialCurve(curve, Math.min(1, t + 0.003));
  return unit(subtract(after, before));
}

function circleNormal(tangent: MemoryPoint3, reference: MemoryPoint3, angle: number): MemoryPoint3 {
  const horizontal = unit(cross(tangent, reference));
  const vertical = unit(cross(tangent, horizontal));
  return add(multiply(horizontal, Math.cos(angle)), multiply(vertical, Math.sin(angle)));
}

function tube(data: number[], curve: MemorySpatialCurve, radius: number, color: Color, pattern?: "dotted" | "dashed" | "event") {
  const chord = unit(subtract(curve[3], curve[0]));
  const reference = Math.abs(chord.z) > 0.72 ? { x: 0, y: 1, z: 0 } : { x: 0, y: 0, z: 1 };
  const rows = Array.from({ length: tubeSegments + 1 }, (_, i) => {
    const t = i / tubeSegments;
    const point = sampleMemorySpatialCurve(curve, t);
    const tangent = curveTangent(curve, t);
    return Array.from({ length: tubeSides }, (_, j) => {
      const normal = circleNormal(tangent, reference, j * Math.PI * 2 / tubeSides);
      return { point: add(point, multiply(normal, radius)), normal, anchorZ: point.z };
    });
  });
  for (let i = 0; i < tubeSegments; i++) {
    if (pattern === "dotted" && i % 5 > 1 || pattern === "dashed" && i % 6 > 3 ||
        pattern === "event" && (i % 10 === 5 || i % 10 > 7)) continue;
    for (let j = 0; j < tubeSides; j++) {
      const next = (j + 1) % tubeSides;
      const a = rows[i][j], b = rows[i][next], c = rows[i + 1][j], d = rows[i + 1][next];
      for (const { point, normal, anchorZ } of [a, b, c, b, d, c]) {
        vertex(data, point, normal, color, anchorZ);
      }
    }
  }
}

function directionalCone(data: number[], curve: MemorySpatialCurve, clearance: number, color: Color) {
  let t = 1;
  while (t > 0.5 && length(subtract(curve[3], sampleMemorySpatialCurve(curve, t))) < clearance + 2) t -= 0.005;
  if (t <= 0.5) return;
  const tip = sampleMemorySpatialCurve(curve, t);
  const tangent = curveTangent(curve, t);
  const base = subtract(tip, multiply(tangent, 11));
  const reference = Math.abs(tangent.z) > 0.72 ? { x: 0, y: 1, z: 0 } : { x: 0, y: 0, z: 1 };
  for (let i = 0; i < 12; i++) {
    const a = circleNormal(tangent, reference, i * Math.PI / 6);
    const b = circleNormal(tangent, reference, (i + 1) * Math.PI / 6);
    vertex(data, add(base, multiply(a, 4.4)), unit(add(a, multiply(tangent, 0.4))), color, base.z);
    vertex(data, add(base, multiply(b, 4.4)), unit(add(b, multiply(tangent, 0.4))), color, base.z);
    vertex(data, tip, unit(add(add(a, b), multiply(tangent, 0.4))), color, tip.z);
  }
}

function palette(dark: boolean) {
  return {
    current: (dark ? [.96,.57,.78] : [.69,.30,.54]) as Color,
    selected: (dark ? [1,.73,.89] : [.78,.33,.58]) as Color,
    historic: (dark ? [.68,.59,.87] : [.48,.39,.68]) as Color,
    auxiliary: (dark ? [.66,.65,.79] : [.45,.45,.62]) as Color,
    relation: (dark ? [.70,.63,.94] : [.51,.39,.74]) as Color,
    pastRelation: (dark ? [.63,.78,.98] : [.30,.48,.71]) as Color,
    event: (dark ? [1,.58,.78] : [.68,.29,.53]) as Color,
    base: (dark ? [.47,.72,.97] : [.26,.48,.74]) as Color,
    grid: (dark ? [.20,.23,.35] : [.72,.70,.85]) as Color,
    subdued: (dark ? [.25,.28,.39] : [.73,.70,.83]) as Color,
  };
}

export function buildMemorySolidGeometry(
  scene: MemorySpaceScene,
  edges: readonly SolidLink[],
  focusedId: string | null,
  dark: boolean,
) {
  const colors = palette(dark);
  const base: number[] = [];
  const upper: number[] = [];
  const projection: number[] = [];
  const points = new Map(scene.points.map(point => [point.id, point]));
  const edgeById = new Map(edges.map(edge => [edge.id, edge]));
  let spheres = 0;
  let tubes = 0;
  const dim = (color: Color): Color => [
    color[0] * .38 + colors.subdued[0] * .62,
    color[1] * .38 + colors.subdued[1] * .62,
    color[2] * .38 + colors.subdued[2] * .62,
  ];

  for (const point of scene.points) {
    const radius = memoryVersionRadius(point);
    const color = point.auxiliary ? colors.auxiliary : point.current
      ? (point.memoryId === focusedId ? colors.selected : colors.current) : point.base ? colors.base : colors.historic;
    sphere(point.base ? base : upper, point, radius, color, !point.current || Boolean(point.auxiliary));
    if (point.current && !point.historyOnly) sphere(projection, point, radius, color, Boolean(point.auxiliary));
    spheres++;
  }

  for (const link of scene.links) {
    const from = points.get(link.source), to = points.get(link.target), edge = edgeById.get(link.id);
    if (!from || !to || !edge || length(subtract(to, from)) < 1) continue;
    const {atBase, path:curve} = scene.curves.get(link.id)!;
    const color = edge.event ? colors.event : atBase ? colors.base : edge.historical ? colors.pastRelation : colors.relation;
    const incident = focusedId && (from.memoryId === focusedId || to.memoryId === focusedId);
    const lineColor = incident || !focusedId && (atBase || edge.historical) ? color : dim(color);
    const data = atBase ? base : upper;
    const pattern = edge.pending ? "dashed" : edge.event ? "event" : undefined;
    tube(data, curve, incident || atBase ? 2.5 : edge.historical ? 2 : 1.45, lineColor, pattern);
    if (edge.directed) directionalCone(data, curve, memoryVersionRadius(to), color);
    if (from.current && to.current && !from.historyOnly && !to.historyOnly && !edge.historical && !edge.event) {
      tube(projection, curve, 1.6, lineColor, pattern);
      if (edge.directed) directionalCone(projection, curve, memoryVersionRadius(to), color);
    }
    tubes++;
  }
  for (const riser of scene.risers) {
    const from = points.get(riser.source), to = points.get(riser.target);
    if (!from || !to || length(subtract(to, from)) < 1) continue;
    tube(upper, [from, from, to, to], focusedId === from.memoryId ? 2.15 : 1.65,
      focusedId && focusedId !== from.memoryId ? dim(colors.historic) : colors.historic, "dotted");
    tubes++;
  }
  // Open grid at z=0, with no perimeter and no nodes below the reference plane.
  const {minX,maxX,minY,maxY} = scene.floor;
  const normal = {x:0,y:0,z:1};
  const gridLine = (x1: number,y1: number,x2: number,y2: number,w: number,h: number) => {
    const a = {x:x1,y:y1,z:-.8}, b = {x:x2,y:y2,z:-.8};
    const c = {x:x1+w,y:y1+h,z:-.8}, d = {x:x2+w,y:y2+h,z:-.8};
    for (const point of [a,b,c,b,d,c]) vertex(base,point,normal,colors.grid,0);
  };
  for (let i=1;i<9;i++) {
    const x = minX + (maxX-minX)*i/9, y = minY + (maxY-minY)*i/9;
    gridLine(x,minY,x,maxY,1,0);
    gridLine(minX,y,maxX,y,0,1);
  }
  return { base: new Float32Array(base), upper: new Float32Array(upper), projection: new Float32Array(projection), spheres, tubes };
}
