import { describe, expect, it } from "vitest";
import type { MemoryRecord } from "../../api/memory";
import { emptyRelationship, readMemoryRelationship, relationshipMemory } from "./relationship";

describe("structured memory relationships", () => {
  it("keeps the graph in sync with the recorded people and action", () => {
    const value = { ...emptyRelationship, subject: "我", label: "姐姐", object: "小林", kind: "kinship" as const, directed: true };
    const memory: MemoryRecord = { id: "relation", category: "fact", source: "manual", created_at: 1, updated_at: 1, ...relationshipMemory(value) };
    expect(readMemoryRelationship(memory)).toEqual(value);
    expect(JSON.parse(memory.metadata!).relationship).not.toHaveProperty("action");
    expect(readMemoryRelationship({ ...memory, content: "关系已由对话纠正" })).toBeNull();
  });

  it("reads a legacy triple and preserves unrelated metadata when editing", () => {
    const legacy: MemoryRecord = { id: "old", category: "fact", source: "manual", created_at: 1, updated_at: 1, content: "我 · 认识 · 小林", metadata: JSON.stringify({ note: "历史来源", relationship: { subject: "我", action: "认识", object: "小林" } }) };
    const relation = readMemoryRelationship(legacy)!;
    expect(relation).toEqual({ ...emptyRelationship, subject: "我", label: "认识", object: "小林", directed: true });
    expect(JSON.parse(relationshipMemory(relation, legacy.metadata).metadata).note).toBe("历史来源");
  });
});
