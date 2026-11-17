import type { MemorySimilarity } from "./memoryGraphApi";

export const memoryNodeSize = { width: 144, height: 104 };
const spacingX = memoryNodeSize.width + 12;
const spacingY = memoryNodeSize.height + 10;
type Place = {x: number; y: number};

function overlaps(a: Place, b: Place) {
  return Math.abs(a.x - b.x) < spacingX && Math.abs(a.y - b.y) < spacingY;
}

function insertWithoutOverlap(positions: Map<string, Place>, id: string, point: Place) {
  let { x, y } = point;
  while ([...positions.values()].some(existing => overlaps(existing, {x,y}))) y += spacingY;
  positions.set(id, {x,y});
}

export function keepMemoryAnchors(
  previous: ReadonlyMap<string, {x: number; y: number}>,
  next: ReadonlyMap<string, {x: number; y: number}>,
) {
  const stable = new Map(previous);
  for (const [id, point] of next) {
    if (stable.has(id)) continue;
    insertWithoutOverlap(stable, id, point);
  }
  return stable;
}

/** Stable radial seeds with soft attraction and collision separation. */
export function layoutMemoryWorld(
  ids: readonly string[],
  edges: readonly { source: string; target: string }[],
  affinities: readonly MemorySimilarity[],
) {
  const degree = new Map<string, number>();
  for (const {source,target} of edges) {
    degree.set(source,(degree.get(source) ?? 0) + 1);
    degree.set(target,(degree.get(target) ?? 0) + 1);
  }
  const ordered = [...ids].sort((a,b) => (degree.get(b) ?? 0) - (degree.get(a) ?? 0));
  const coordinates = new Map(ordered.map((id,index) => {
    const angle = index * 2.399963 - 3.3;
    const radius = 156 * Math.sqrt(index);
    return [id, {x:Math.cos(angle) * radius * 1.3, y:Math.sin(angle) * radius * .92}];
  }));
  const springs = [
    ...edges.map(({source,target}) => ({source,target,length:216,strength:.048})),
    ...affinities.map(pair => ({source:`memory:${pair.source_memory_id}`,target:`memory:${pair.target_memory_id}`,
      length:185,strength:pair.score * .055})),
  ];
  const iterations = ordered.length > 64 ? 12 : 32;
  const points = [...coordinates.values()];
  for (let step = 0; step < iterations; step++) {
    for (const {source,target,length,strength} of springs) {
      const a = coordinates.get(source), b = coordinates.get(target);
      if (!a || !b) continue;
      const dx = b.x - a.x, dy = b.y - a.y;
      const distance = Math.hypot(dx,dy);
      if (distance <= length) continue;
      const pull = (1 - length / distance) * strength;
      a.x += dx * pull; a.y += dy * pull;
      b.x -= dx * pull; b.y -= dy * pull;
    }
    for (let i = 0; i < points.length; i++) for (let j = i + 1; j < points.length; j++) {
      const a = points[i], b = points[j];
      if (!overlaps(a,b)) continue;
      const dx = b.x - a.x, dy = b.y - a.y;
      if (Math.abs(dx) / spacingX > Math.abs(dy) / spacingY) {
        const shift = (spacingX - Math.abs(dx)) * .52 * (dx < 0 ? -1 : 1);
        a.x -= shift; b.x += shift;
      } else {
        const shift = (spacingY - Math.abs(dy)) * .52 * (dy < 0 ? -1 : 1);
        a.y -= shift; b.y += shift;
      }
    }
  }
  const positions = new Map<string, Place>();
  for (const [id,point] of coordinates) insertWithoutOverlap(positions,id,{x:Math.round(point.x),y:Math.round(point.y)});
  const settled = [...positions.values()];
  if (settled.length > 1 && settled.length <= 12) {
    const xs = settled.map(point => point.x), ys = settled.map(point => point.y);
    const spanX = Math.max(...xs) - Math.min(...xs);
    const spanY = Math.max(...ys) - Math.min(...ys);
    const scaleX = Math.min(2.5, Math.max(1, 1.65 * spanY / Math.max(spanX,1)));
    const centerX = (Math.min(...xs) + Math.max(...xs)) / 2;
    for (const point of settled) point.x = Math.round(centerX + (point.x - centerX) * scaleX);
  }
  return positions;
}
