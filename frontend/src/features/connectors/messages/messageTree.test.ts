import { describe, expect, it } from "vitest";
import { ancestorPath, layoutMessageTree, nextAssistantParent } from "./messageTree";

const tree = [
  { id: "q1", parent_id: null },
  { id: "a1", parent_id: "q1" },
  { id: "q2", parent_id: "a1" },
  { id: "a2", parent_id: "q2" },
  { id: "branch", parent_id: "a1" },
  { id: "answer", parent_id: "branch" },
  { id: "nested", parent_id: "q2" },
];

describe("message branch geometry", () => {
  it("keeps the first question and answer chain vertical, branches to separate columns", () => {
    const positions = layoutMessageTree(tree);
    expect(positions.q1.x).toBe(positions.a1.x);
    expect(positions.a1.x).toBe(positions.q2.x);
    expect(positions.q2.x).toBe(positions.a2.x);
    expect(positions.a1.y).toBeGreaterThan(positions.q1.y);
    expect(positions.branch.y).toBe(positions.q2.y);
    expect(positions.branch.x).toBeGreaterThan(positions.nested.x);
    expect(positions.answer.x).toBe(positions.branch.x);
    expect(new Set(Object.values(positions).map(p => `${p.x},${p.y}`)).size).toBe(tree.length);
  });

  it("resolves the exact branch ancestry without sibling messages", () => {
    expect(ancestorPath(tree, "answer")).toEqual(["q1", "a1", "branch", "answer"]);
    expect(ancestorPath(tree, "a2")).toEqual(["q1", "a1", "q2", "a2"]);
    expect(ancestorPath(tree, null)).toEqual([]);
  });

  it("allocates separate root questions and accepts empty graphs", () => {
    const positions = layoutMessageTree([{ id: "one", parent_id: null }, { id: "two", parent_id: null }]);
    expect(positions.two.x).toBeGreaterThan(positions.one.x);
    expect(layoutMessageTree([])).toEqual({});
  });
});

describe("next question context", () => {
  const replies = [
    { id: "a1", parent_id: "q1", role: "assistant", conversation_id: "first", created_at: 1 },
    { id: "q2", parent_id: "a1", role: "user", conversation_id: "second", created_at: 2 },
    { id: "a2", parent_id: "q2", role: "assistant", conversation_id: "second", created_at: 3 },
    { id: "other", parent_id: "a1", role: "assistant", conversation_id: "other", created_at: 4 },
  ];
  it("uses the saved assistant selection or the latest leaf answer on reopen", () => {
    expect(nextAssistantParent(replies, { selectedId: "a1" })).toBe("a1");
    expect(nextAssistantParent(replies, { selectedId: "q2" })).toBe("other");
    expect(nextAssistantParent([], {})).toBeNull();
  });
  it("continues the actual initiating turn rather than a different newer branch", () => {
    expect(nextAssistantParent(replies, { turnConversationId: "second", previousParentId: "a1", selectedId: "other" })).toBe("a2");
  });
  it("retains the prior valid assistant when a failed turn has no answer", () => {
    expect(nextAssistantParent(replies, { turnConversationId: "failed", previousParentId: "a1" })).toBe("a1");
    expect(nextAssistantParent(replies, { turnConversationId: null, previousParentId: "a1", selectedId: "other" })).toBe("a1");
  });
});
