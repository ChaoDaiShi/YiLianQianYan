import { describe, expect, it } from "vitest";
import { isTaskWorldEvent } from "./taskGraphProjection";

describe("Task World event refresh policy", () => {
  it.each(["task.paused", "task.resumed", "task.cancelled", "task.goal.changed"])("invalidates shared state on %s only for its own graph", (type) => {
    expect(isTaskWorldEvent({ type, payload: { graph_id: "graph-1" } }, "graph-1")).toBe(true);
    expect(isTaskWorldEvent({ type, payload: { graph_id: "graph-2" } }, "graph-1")).toBe(false);
  });
  it("refreshes the smallest authoritative projection for supported task facts", () => {
    const event = {
      type: "task.node.running",
      payload: { graph_id: "graph-1", node_id: "node-1" },
    };
    expect(isTaskWorldEvent(event, "graph-1")).toBe(true);
    expect(isTaskWorldEvent({ type: "task.canvas.updated", payload: { graph_id: "graph-1" } }, "graph-1")).toBe(true);
    expect(isTaskWorldEvent({ type: "task.node.running", payload: {} }, "graph-1")).toBe(false);
  });
});
