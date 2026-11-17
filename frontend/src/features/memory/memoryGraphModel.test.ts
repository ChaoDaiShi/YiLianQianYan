import { describe, expect, it } from "vitest";
import type { KnowledgeEntry } from "../../api/knowledge";
import type { MemoryRecord } from "../../api/memory";
import { buildKnowledgeGraph, buildMemoryGraph } from "./memoryGraphModel";
import { emptyRelationship, relationshipMemory } from "./relationship";

const memory = (id: string, patch: Partial<MemoryRecord> = {}): MemoryRecord => ({
  id, category: "fact", source: "manual", content: "示例正文", created_at: 1, updated_at: 1, ...patch,
});

describe("memory graph projection", () => {
  it("keeps only explicit relationships and separates inferred, directional and temporal evidence", () => {
    const kinship = memory("kinship", relationshipMemory({ ...emptyRelationship, kind: "kinship", subject: "小林", label: "姐妹", object: "小周", certainty: "inferred" }));
    expect(kinship.content).toContain("待确认");
    const event = memory("event", relationshipMemory({ ...emptyRelationship, kind: "event", subject: "启动", label: "随后", object: "验收", directed: true, occurredAt: "2026-11-13" }));
    const project = memory("project", relationshipMemory({ ...emptyRelationship, kind: "project", subject: "遥感项目", label: "依赖", object: "数据准备", directed: true }));
    const plain = memory("plain", { content: "小林和小周一起讨论。" });
    const people = buildMemoryGraph([kinship, event, project, plain], "people");
    expect(people.edges).toHaveLength(1);
    expect(people.edges[0]).toMatchObject({ kind: "kinship", directed: false, label: "姐妹", inferred: true });
    expect(people.edges[0].evidence).toMatchObject({ type: "memory", id: "kinship" });
    const events = buildMemoryGraph([kinship, event, project, plain], "event");
    expect(events.edges).toHaveLength(1);
    expect(events.edges[0]).toMatchObject({ kind: "event", directed: true, occurredAt: "2026-11-13" });
    expect(events.nodes.get(events.edges[0].source)?.kind).toBe("event");
    expect(buildMemoryGraph([kinship, event, project, plain], "project").edges[0].kind).toBe("project");
    expect(buildMemoryGraph([kinship, event, project, plain], "relations").edges.map(edge => edge.kind)).toEqual(["kinship", "event", "project"]);
  });

  it("removes deleted memory evidence and never links an unavailable conversation", () => {
    const source = memory("first", { source_conversation_id: "c1", source_removed: true });
    const other = memory("second", { source_conversation_id: "c2" });
    const before = buildMemoryGraph([source, other], "network");
    expect(before.nodes.get("conversation:removed:first")).toMatchObject({ label: "来源已移除" });
    expect(before.nodes.get("conversation:removed:first")?.href).toBeUndefined();
    expect(before.nodes.get("conversation:c2")?.href).toBe("/chat/c2");
    const after = buildMemoryGraph([other], "network");
    expect(after.nodes.has("memory:first")).toBe(false);
    expect(after.edges.some(edge => edge.evidence.id === "first")).toBe(false);
  });

  it("shows a reviewed AI concept relation in its own view with the pending status and evidence", () => {
    const concept = memory("concept", { category: "fact", source: "manual", content: "向量检索 · 属于 · 语义检索 · 待确认", metadata: JSON.stringify({
      candidate_id: "ai-1", source_kind: "user_message", relationship: {
        kind: "concept", subject: "向量检索", object: "语义检索", label: "属于", directed: true, certainty: "inferred", occurredAt: "",
      },
    }) });
    const graph = buildMemoryGraph([concept], "concept");
    expect(graph.nodes.get("concept:向量检索")).toMatchObject({ kind: "concept", label: "向量检索" });
    expect(graph.edges).toEqual([expect.objectContaining({ source: "concept:向量检索", target: "concept:语义检索", kind: "concept", directed: true, inferred: true, evidence: { type: "memory", id: "concept", time: 1 } })]);
    expect(buildMemoryGraph([concept], "people").edges).toHaveLength(0);
  });

  it("keeps exact source anchors and preserves unavailable status without open actions", () => {
    const entry: KnowledgeEntry = { id: "entry", topic: "水系", title: "径流结论", summary: "水文", updated_at: 1, source_count: 2, sources: [
      { document_id: "a", document_name: "原文 A", version: "v1", start_char: 4, end_char: 8, excerpt: "证据 A", status: "available" },
      { document_id: "b", document_name: "原文 B", version: "v2", start_char: 5, end_char: 9, excerpt: null, status: "removed" },
    ] };
    const graph = buildKnowledgeGraph([entry], entry);
    expect(graph.edges[0].evidence).toMatchObject({ type: "citation", href: "/knowledge?document=a&version=v1&start=4&end=8", status: "可定位原文" });
    expect(graph.edges[1].evidence).toMatchObject({ type: "citation", status: "来源已移除" });
    expect(graph.edges[1].evidence).not.toHaveProperty("href");
  });
});
