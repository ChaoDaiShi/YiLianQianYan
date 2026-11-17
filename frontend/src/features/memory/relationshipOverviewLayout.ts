import type { GraphEdge } from "./memoryGraphModel";

const COLUMN = 420;
const ROW = 112;

export function layoutRelationshipOverview(ids: readonly string[], edges: readonly Pick<GraphEdge, "source" | "target">[], packIsolated = false) {
  const neighbors = new Map(ids.map(id => [id, new Set<string>()]));
  const incoming = new Map(ids.map(id => [id, new Set<string>()]));
  for (const { source, target } of edges) {
    neighbors.get(source)!.add(target);
    neighbors.get(target)!.add(source);
    incoming.get(target)!.add(source);
  }

  const positions = new Map<string, { x: number; y: number }>();
  let y = 0;
  for (const root of ids) {
    if (positions.has(root)) continue;
    if (packIsolated && !neighbors.get(root)!.size) continue;
    const component = new Set([root]);
    for (const id of component) for (const next of neighbors.get(id)!) component.add(next);

    // Place upstream nodes before their targets; separate disconnected groups vertically.
    // For a cycle, use the first remaining node to seed its column.
    const remaining = new Set(component);
    let column = 0;
    let height = 0;
    while (remaining.size) {
      const layer = [...remaining].filter(id => [...incoming.get(id)!].every(parent => !remaining.has(parent)));
      if (!layer.length) layer.push(remaining.values().next().value!);
      layer.forEach((id, index) => {
        positions.set(id, { x: column * COLUMN, y: y + index * ROW });
        remaining.delete(id);
      });
      height = Math.max(height, layer.length);
      column++;
    }
    y += height * ROW;
  }
  if (packIsolated) ids.filter(id => !positions.has(id)).forEach((id, index) => {
    positions.set(id, { x: index % 4 * 235, y: y + Math.floor(index / 4) * ROW });
  });
  return positions;
}
