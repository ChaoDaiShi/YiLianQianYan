import type { CanvasProjection } from "./MemoryGraphCanvas";
import type { MemoryGraphSnapshot } from "./memoryGraphApi";
import type { SpaceEdge, SpaceMemory } from "./MemorySpaceView";
import { truncateMemoryContent } from "./memoryCenterModel";
import { readMemoryRelationship } from "./relationship";
import { memoryNodeSize } from "./memoryWorldLayout";

export function memorySpace(snapshot: MemoryGraphSnapshot, projection: CanvasProjection, showPending: boolean) {
  const positions = new Map<string, { x: number; y: number }>();
  for (const [key, pos] of projection.positions) if (key.startsWith("memory:")) positions.set(key.slice(7), pos);
  // Semantic views represent a saved relationship memory by an edge rather than a card.
  for (const edge of projection.edges) {
    const id = edge.evidence.type === "memory" ? edge.evidence.id : edge.evidence.type === "extracted" ? edge.evidence.memoryId : null;
    if (!id || positions.has(id)) continue;
    const a = projection.positions.get(edge.source), b = projection.positions.get(edge.target);
    if (a && b && edge.kind !== "source" && edge.kind !== "category")
      positions.set(id, { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });
  }

  const items: SpaceMemory[] = snapshot.memories.flatMap(memory => {
    const pos = positions.get(memory.id);
    const versions = snapshot.histories[memory.id];
    if (!pos || !versions?.length || (!showPending && readMemoryRelationship(memory)?.certainty === "inferred")) return [];
    const node = projection.nodes?.find(node => node.recordId === memory.id);
    return [{ id: memory.id, label: node?.label ?? truncateMemoryContent(memory.content, 32), ...pos, versions,
      currentVersionId: snapshot.current_version_ids?.[memory.id] ?? null, category: memory.category,
      historyOnly: projection.shared && !node,
      categoryLabel: node?.pending ? `${node.caption ?? "记忆"} · 待确认` : node?.caption,
      preview: node?.preview, cardWidth: memoryNodeSize.width, cardHeight: memoryNodeSize.height }];
  });
  if (projection.shared) for (const node of projection.nodes ?? []) {
    const position = projection.positions.get(node.id);
    if (node.kind !== "memory" && position) items.push({ id: node.id, label: node.label, ...position, versions: [], auxiliary: true, caption: node.status, cardWidth: 180, cardHeight: 60 });
  }
  const visible = new Map(items.map(item => [item.id, item]));
  const edges: SpaceEdge[] = [];
  for (const relation of snapshot.relations) {
    const source = relation.source_memory_id, target = relation.target_memory_id;
    const a = visible.get(source), b = visible.get(target);
    if (!a || !b) continue;
    if (!projection.shared && (relation.status === "confirmed" || (relation.status === "pending" && showPending))) edges.push({
      id: relation.id, source, target, directed: false, label: relation.label,
      pending: relation.status === "pending",
    });
  }
  const owner = new Map(items.flatMap(item => item.versions.map(version => [version.version_id, item.id] as const)));
  for (const version of snapshot.relation_versions) {
    if (version.status !== "confirmed") continue;
    const source = owner.get(version.source_version_id), target = owner.get(version.target_version_id);
    if (!source || !target) continue;
    const current = snapshot.relations.find(relation => relation.id === version.relation_id);
    if (current?.status === "confirmed" && current.revision === version.revision &&
      version.source_version_id === visible.get(source)?.currentVersionId &&
      version.target_version_id === visible.get(target)?.currentVersionId) continue;
    edges.push({id:`${version.relation_id}:${version.revision}`,source,target,directed:false,label:version.label,historical:true,
      sourceVersionId:version.source_version_id,targetVersionId:version.target_version_id});
  }
  if (projection.shared) for (const edge of projection.edges) {
    const a = edge.source.startsWith("memory:") ? edge.source.slice(7) : edge.source;
    const b = edge.target.startsWith("memory:") ? edge.target.slice(7) : edge.target;
    if (visible.has(a) && visible.has(b)) edges.push({id:edge.id,source:a,target:b,label:edge.label,directed:edge.directed,pending:edge.inferred});
  }
  for (const event of snapshot.change_events ?? []) {
    const source = event.related_memory_id;
    const anchor = event.related_version_id;
    const target = visible.get(event.memory_id);
    if (!source || !anchor || !event.related_event_id ||
        !visible.get(source)?.versions.some(version => version.version_id === anchor) ||
        !target?.versions.some(version => version.version_id === event.version_id)) continue;
    edges.push({
      id: `event:${event.id}`, source, target: event.memory_id, label: "经审阅的关联变更", directed: true, event: true,
      sourceVersionId: anchor, targetVersionId: event.version_id,
    });
  }
  return { items, edges };
}
