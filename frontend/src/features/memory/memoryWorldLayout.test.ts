import { expect, it } from "vitest";
import { keepMemoryAnchors, layoutMemoryWorld, memoryNodeSize } from "./memoryWorldLayout";

it("positions semantically close isolated notes next to each other without inventing edges", () => {
  const ids = ["memory:a", "memory:b", "memory:c", "memory:d"];
  const layout = layoutMemoryWorld(ids, [], [{ source_memory_id: "a", target_memory_id: "d", score: .9 }]);
  const neighbors = [layout.get("memory:a")!, layout.get("memory:d")!];
  const neutral = layoutMemoryWorld(ids, [], []);
  const baseline = [neutral.get("memory:a")!, neutral.get("memory:d")!];
  const distance = ([a,b]: typeof neighbors) => Math.hypot(a.x - b.x, a.y - b.y);
  expect(distance(neighbors)).toBeLessThan(distance(baseline));
  expect(new Set([...layout.values()].map(p => `${p.x}:${p.y}`)).size).toBe(4);
});

it("keeps the original anchor when a refresh reorders or inserts a memory", () => {
  const previous = layoutMemoryWorld(["memory:a", "memory:b"], [], []);
  const reordered = layoutMemoryWorld(["memory:c", "memory:b", "memory:a"], [], []);
  const anchors = keepMemoryAnchors(previous, reordered);
  expect(anchors.get("memory:a")).toEqual(previous.get("memory:a"));
  expect(anchors.get("memory:b")).toEqual(previous.get("memory:b"));
  expect(anchors.get("memory:c")).not.toEqual(previous.get("memory:a"));
});

it("spaces and preserves every displayed node in a dense connected and isolated set", () => {
  const ids = Array.from({length: 28}, (_, i) => `memory:${i}`);
  const edges = ids.slice(1, 12).map(target => ({source: ids[0], target}));
  const layout = layoutMemoryWorld(ids, edges, []);
  expect(layout.size).toBe(28);
  const positions = [...layout.values()];
  for (let i = 0; i < positions.length; i++) for (let j = i + 1; j < positions.length; j++) {
    expect(Math.abs(positions[i].x - positions[j].x) >= memoryNodeSize.width ||
      Math.abs(positions[i].y - positions[j].y) >= memoryNodeSize.height).toBe(true);
  }
});
