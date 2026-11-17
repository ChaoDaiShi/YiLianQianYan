import { expect, it } from "vitest";
import { layoutRelationshipOverview } from "./relationshipOverviewLayout";

it("separates unrelated graph groups and lays a connected event chain out in order", () => {
  const positions = layoutRelationshipOverview(["second", "third", "concept", "parent", "first"], [
    { source: "second", target: "third" }, { source: "concept", target: "parent" }, { source: "first", target: "second" },
  ]);
  const first = positions.get("first")!;
  const second = positions.get("second")!;
  const third = positions.get("third")!;
  const concept = positions.get("concept")!;
  const parent = positions.get("parent")!;
  expect([first.y, second.y, third.y]).toEqual([first.y, first.y, first.y]);
  expect(parent.y).toBe(concept.y);
  expect(Math.abs(concept.y - first.y)).toBeGreaterThanOrEqual(90);
  expect(second.x - first.x).toBeGreaterThan(300);
  expect(third.x - second.x).toBeGreaterThan(300);
  expect(parent.x - concept.x).toBeGreaterThan(300);
});
