import type { KnowledgeEntry, KnowledgeEntrySummary } from "../../api/knowledge";
import type { MemoryRecord } from "../../api/memory";
import { knowledgeSourceUrl } from "../knowledge/knowledgeAnchor";
import { getMemoryCategoryPresentation, truncateMemoryContent } from "./memoryCenterModel";
import { readMemoryRelationship, type RelationshipKind } from "./relationship";
import type { MemoryGraphSnapshot } from "./memoryGraphApi";

export type MemoryGraphMode = "network" | "relations" | "people" | "event" | "project" | "concept" | "preference";
export type GraphNodeKind = "category" | "memory" | "person" | "conversation" | "event" | "project" | "concept" | "preference" | "knowledge" | "document";
export type GraphNode = {
  id: string; label: string; kind: GraphNodeKind; href?: string; recordId?: string; status?: string;
  preview?: string; caption?: string; changes?: number; revision?: number; category?: string; pending?: boolean;
};
export type GraphEvidence = { type: "memory"; id: string; time: number }
  | { type: "extracted"; id: string; memoryId: string; time: number }
  | { type: "correspondence"; id: string; time: number }
  | { type: "citation"; id: string; time: number; status: string; excerpt: string | null; href?: string };
export type GraphEdge = {
  id: string; source: string; target: string; label: string; directed: boolean; evidence: GraphEvidence;
  kind: RelationshipKind | "category" | "source" | "citation" | "correspondence";
  occurredAt?: string;
  inferred?: boolean;
};
export type MemoryGraph = { nodes: Map<string, GraphNode>; edges: GraphEdge[]; overview: string[] };

export const memoryGraphModes: Record<MemoryGraphMode, string> = {
  network: "记忆网络", relations: "多类关系", people: "人物与亲属", event: "事件链", project: "项目关系", concept: "概念关系", preference: "偏好关系",
};

export function viewForRelationship(kind: RelationshipKind): Exclude<MemoryGraphMode, "network" | "relations"> {
  return kind === "person" || kind === "kinship" ? "people" : kind;
}

export function buildMemoryGraph(memories: readonly MemoryRecord[], mode: MemoryGraphMode): MemoryGraph {
  const nodes = new Map<string, GraphNode>();
  const edges: GraphEdge[] = [];
  const overview = new Set<string>();
  const add = (node: GraphNode) => { if (!nodes.has(node.id)) nodes.set(node.id, node); return node.id; };

  for (const memory of memories) {
    const evidence = { type: "memory" as const, id: memory.id, time: memory.updated_at };
    if (mode !== "network") {
      const relation = readMemoryRelationship(memory);
      if (!relation) continue;
      const view = viewForRelationship(relation.kind);
      if (mode !== "relations" && mode !== view) continue;
      const kind = view === "people" ? "person" : view;
      const source = add({ id: `${kind}:${relation.subject}`, kind, label: relation.subject });
      const target = add({ id: `${kind}:${relation.object}`, kind, label: relation.object });
      overview.add(source);
      overview.add(target);
      edges.push({ id: memory.id, source, target, label: relation.label, directed: relation.directed, kind: relation.kind, occurredAt: relation.occurredAt, inferred: relation.certainty === "inferred", evidence });
      continue;
    }

    const category = add({ id: `category:${memory.category}`, kind: "category", label: getMemoryCategoryPresentation(memory.category).label });
    const label = truncateMemoryContent(memory.content.replace(/\s+/g, " "), 36);
    const item = add({ id: `memory:${memory.id}`, kind: "memory", label, recordId: memory.id });
    overview.add(category);
    edges.push({ id: `category:${memory.id}`, source: category, target: item, label: "归类", directed: false, kind: "category", evidence });
    if (memory.source_removed) {
      const source = add({ id: `conversation:removed:${memory.id}`, kind: "conversation", label: "来源已移除", status: "来源已移除" });
      edges.push({ id: `source:${memory.id}`, source, target: item, label: "形成记忆", directed: true, kind: "source", evidence });
    } else if (memory.source_conversation_id) {
      const id = memory.source_conversation_id;
      const source = add({ id: `conversation:${id}`, kind: "conversation", label: "来源对话", href: `/chat/${encodeURIComponent(id)}` });
      edges.push({ id: `source:${memory.id}`, source, target: item, label: "形成记忆", directed: true, kind: "source", evidence });
    }
  }
  return { nodes, edges, overview: [...overview] };
}

export function projectMemoryGraph(snapshot: MemoryGraphSnapshot, mode: MemoryGraphMode, showPending = false): MemoryGraph {
  const records = snapshot.memories.filter(memory =>
    showPending || readMemoryRelationship(memory)?.certainty !== "inferred");
  if (mode !== "network") {
    const graph = buildMemoryGraph(records, mode);
    for (const entry of snapshot.extracted_relations ?? []) {
      const relation = entry.relationship;
      const view = viewForRelationship(relation.kind);
      if ((mode !== "relations" && mode !== view) || (relation.certainty === "inferred" && !showPending)) continue;
      const kind = view === "people" ? "person" : view;
      const endpoint = (label: string) => {
        const id = `${kind}:${label}`;
        if (!graph.nodes.has(id)) {
          graph.nodes.set(id, { id, kind, label });
          graph.overview.push(id);
        }
        return id;
      };
      graph.edges.push({id:entry.id,source:endpoint(relation.subject),target:endpoint(relation.object),label:relation.label,directed:relation.directed,kind:relation.kind,
        occurredAt:relation.occurredAt,inferred:relation.certainty === "inferred",evidence:{type:"extracted",id:entry.id,memoryId:entry.source_memory_id,time:entry.recorded_at ?? 0}});
    }
    return graph;
  }
  const base = buildMemoryGraph(snapshot.memories, mode);
  const overview = [...records].sort((a, b) => a.created_at - b.created_at || a.id.localeCompare(b.id))
    .map(memory => `memory:${memory.id}`);
  for (const memory of records) {
    const node = base.nodes.get(`memory:${memory.id}`)!;
    const text = memory.content.trim();
    const split = text.match(/^(.{2,24}?)(?:\s+·\s+|[：:]|\r?\n)([\s\S]+)/u);
    node.label = split ? split[1] : truncateMemoryContent(text.replace(/\s+/g, " "), 52);
    node.preview = split ? truncateMemoryContent(split[2].trim().replace(/\s+/g, " "), 96) : undefined;
    node.caption = getMemoryCategoryPresentation(memory.category).label;
    node.category = memory.category;
    const effective = snapshot.current_version_ids?.[memory.id];
    const version = snapshot.histories[memory.id]?.find(item => item.version_id === effective);
    node.changes = version?.layer;
    node.revision = version?.revision;
    node.pending = readMemoryRelationship(memory)?.certainty === "inferred";
    node.status = memory.source_removed ? "来源已移除" : undefined;
  }
  const ids = new Set(overview);
  for (const id of [...base.nodes.keys()]) {
    if (id.startsWith("memory:") && !ids.has(id)) base.nodes.delete(id);
  }
  const edges = base.edges.filter(edge => edge.kind === "source" && ids.has(edge.target));
  for (const relation of snapshot.relations) {
    const source = `memory:${relation.source_memory_id}`, target = `memory:${relation.target_memory_id}`;
    if (!ids.has(source) || !ids.has(target) ||
      (relation.status !== "confirmed" && !(showPending && relation.status === "pending"))) continue;
    edges.push({
      id: relation.id, source, target, label: relation.label, directed: false, kind: "correspondence",
      inferred: relation.status === "pending", evidence: { type: "correspondence", id: relation.id, time: relation.updated_at },
    });
  }
  return { ...base, edges, overview };
}

const sourceStatus = { available: "可定位原文", changed: "正文版本已变化", removed: "来源已移除" };

export function buildKnowledgeGraph(entries: readonly KnowledgeEntrySummary[], focus?: KnowledgeEntry): MemoryGraph {
  const nodes = new Map<string, GraphNode>();
  const edges: GraphEdge[] = [];
  for (const entry of entries) {
    const id = `knowledge:${entry.id}`;
    nodes.set(id, { id, kind: "knowledge", label: entry.title, recordId: entry.id });
  }
  const overview = [...nodes.keys()];
  const entryNode = `knowledge:${focus?.id}`;
  if (!focus || !nodes.has(entryNode)) return { nodes, edges, overview };
  for (const source of focus.sources) {
    const id = `document:${source.document_id}:${source.version}:${source.start_char}:${source.end_char}`;
    const status = sourceStatus[source.status];
    const link = source.status === "available" ? { href: knowledgeSourceUrl(source) } : {};
    nodes.set(id, { id, kind: "document", label: source.document_name, status, ...link });
    edges.push({ id, source: entryNode, target: id, label: "引用原文", directed: true, kind: "citation", evidence: { type: "citation", id, time: focus.updated_at, status, excerpt: source.excerpt, ...link } });
  }
  return { nodes, edges, overview };
}
