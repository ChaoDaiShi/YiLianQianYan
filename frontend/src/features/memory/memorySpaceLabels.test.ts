import { expect, it } from "vitest";
import { memorySpaceLineClearance, placeMemorySpaceEdgeLabel, placeMemorySpaceLabels } from "./memorySpaceLabels";

const label = (id: string, x: number, y: number, priority = 0, depth = 0) => ({
  id, x, y, width: 120, height: 36, scale: 1, priority, depth,
});

it("reduces labels only where they overlap and prioritizes the selected stack", () => {
  const labels = Array.from({ length: 39 }, (_, index) => label(String(index), 155 + index % 4 * 20, 70 + index));
  const selected = label("selected", 174, 78, 3);
  const distant = label("distant", 120, 242);
  const placements = placeMemorySpaceLabels([...labels, distant, selected], { width: 390, height: 320 });
  expect(placements.has(selected.id)).toBe(true);
  expect(placements.has(distant.id)).toBe(true);
  expect(placements.size).toBeLessThan(labels.length);
  const boxes = [...placements.values()].map(({ bounds }) => bounds);
  for (const [index, first] of boxes.entries()) for (const second of boxes.slice(index + 1)) {
    expect(first.left >= second.right || second.left >= first.right || first.top >= second.bottom || second.top >= first.bottom).toBe(true);
  }
});

it("fits labels back to the left when their point approaches the right edge", () => {
  const placements = placeMemorySpaceLabels([label("edge", 380, 175)], { width: 390, height: 320 });
  const result = placements.get("edge")!;
  expect(result.x).toBeLessThan(0);
  expect(result.bounds.right).toBeLessThan(380);
});

it("prefers the position above a star when the title will clear its rings", () => {
  const placements = placeMemorySpaceLabels([
    { ...label("star", 200, 180), above: true, anchorPadding: 28 },
  ], { width:390, height:320 });
  expect(placements.get("star")?.bounds).toEqual({left:140,right:260,top:116,bottom:152});
});

it("does not lay a full card over another current point or the overlaid controls", () => {
  const center = { ...label("center", 160, 120, 2), centered: true };
  const neighbor = { ...label("neighbor", 202, 128, 2), centered: true };
  const control = { ...label("controls", 130, 25, 2), centered: true };
  const far = { ...label("spaced", 170, 250, 2), centered: true };
  const results = placeMemorySpaceLabels([center, neighbor, control, far], { width: 390, height: 330 }, [], [40, 28]);
  expect([...results.keys()]).toEqual(["spaced"]);
  expect(results.get("spaced")?.bounds).toMatchObject({ left: 110, top: 232 });
});

it("places edge text only where clear of cards, other captions and controls", () => {
  const occupied = [{ left: 100, right: 230, top: 120, bottom: 190 }];
  const size = { width: 390, height: 330 };
  const inset: [number, number] = [40, 28];
  expect(placeMemorySpaceEdgeLabel(190, 129, 100, occupied, size, inset)).toBe(false);
  expect(placeMemorySpaceEdgeLabel(254, 44, 100, occupied, size, inset)).toBe(false);
  expect(placeMemorySpaceEdgeLabel(245, 90, 100, occupied, size, inset)).toBe(true);
  expect(placeMemorySpaceEdgeLabel(230, 95, 100, occupied, size, inset)).toBe(false);
});

it("follows the marker rather than choosing a new slot when the camera depth changes", () => {
  const size = { width:390, height:320 };
  const first = { ...label("a", 95, 135, 3), above: true };
  const second = { ...label("b", 215, 137, 3), above: true };
  const old = placeMemorySpaceLabels([first, second], size, [], [0,0], new Map());
  const next = placeMemorySpaceLabels([{...second, depth:10, y:138}, {...first, depth:-10, x:94}], size, [], [0,0], old);
  for (const id of ["a","b"]) {
    expect(next.get(id)?.x).toBe(old.get(id)?.x);
    expect(next.get(id)?.y).toBe(old.get(id)?.y);
  }
});

it("nudges a retained title out of a passing relationship segment", () => {
  const size = { width:400, height:320 };
  const marker = { ...label("a", 200, 160, 3), above:true };
  const old = placeMemorySpaceLabels([marker], size, [], [0,0], new Map());
  const bounds = old.get("a")!.bounds;
  const line = memorySpaceLineClearance([{ x:135, y:bounds.bottom-1 },{ x:275, y:bounds.bottom-1 }]);
  const next = placeMemorySpaceLabels([marker], size, line, [0,0], old).get("a")!;
  expect(next.x).toBeCloseTo(old.get("a")!.x);
  expect(next.bounds.bottom).toBeLessThan(line[0].top);
  expect(bounds.bottom-next.bounds.bottom).toBeLessThan(12);
});
