import { describe, expect, it } from "vitest";
import type { CanvasView } from "../../../../api/taskWorld";
import type { TaskGraphProjection, TaskNodeProjection } from "../../taskGraphProjection";
import { reconcileCanvasNodes } from "./reconcileCanvasNodes";

function task(id: string): TaskNodeProjection {
  return {
    id, kind: "work", title: id, status: "pending", role: "Task", isRunning: false,
    state: { status: "pending", attempts: 0, result_summary: null, error: null, started_at: null, finished_at: null, updated_at: 1 },
    executor_ref: null, instruction_summary: "", acceptance_criteria: [], resources: [],
    validation: { status: "pending", issues: [] }, result_summary: null,
  };
}
const graph: TaskGraphProjection = {
  graphId: "graph", schemaVersion: 1, revision: 1,
  nodes: [task("A"), task("B"), task("C")], edges: [], revisions: [], checkpoints: [],
};
const view: CanvasView = {
  schema_version: 1, graph_id: "graph", view_revision: 1, graph_revision_seen: 1,
  viewport: { x: 340, y: -120, zoom: 1.35 },
  node_layouts: [{ node_id: "A", x: 100, y: 40, width: 240, height: 128 }],
  selection: [], groups: [], updated_at: 1,
};
const initial = () => reconcileCanvasNodes([], graph, view);
const dragged = () => initial().map((node) => node.id === "A"
  ? { ...node, position: { x: 500, y: 50 }, selected: true, dragging: true, measured: { width: 240, height: 128 } }
  : node);

describe("canvas node authority", () => {
  it.each(["running", "succeeded"] as const)("keeps position/dimensions/selection on %s", (status) => {
    const current = dragged();
    const incoming = { ...graph, nodes: graph.nodes.map((node) => node.id === "A" ? { ...node, status } : node) };
    const next = reconcileCanvasNodes(current, incoming, view);
    expect(next[0].position).toBe(current[0].position);
    expect(next[0].measured).toBe(current[0].measured);
    expect(next[0].selected).toBe(true);
    expect(next[0].dragging).toBe(true);
    expect(next[0].data.status).toBe(status);
    expect(next[1]).toBe(current[1]);
  });
  it("updates titles without changing positions", () => {
    const current = dragged();
    const next = reconcileCanvasNodes(current, { ...graph, nodes: [{ ...graph.nodes[0], title: "Edited" }, ...graph.nodes.slice(1)] }, view);
    expect(next[0].data.task.title).toBe("Edited");
    expect(next[0].position.x).toBe(500);
  });
  it("focus changes data, preserving local multi-selection and positions", () => {
    const current = dragged();
    const next = reconcileCanvasNodes(current, graph, view, { focusedNodeId: "B" });
    expect(next[0].position.x).toBe(500);
    expect(next[0].selected).toBe(true);
    expect(next[1].data.isFocused).toBe(true);
  });
  it("places only new nodes and respects their saved layouts", () => {
    const current = dragged();
    const incoming = { ...graph, nodes: [task("new"), ...graph.nodes] };
    const doc = { ...view, node_layouts: [...view.node_layouts, { node_id: "new", x: 900, y: 700, width: 240, height: 128 }] };
    const next = reconcileCanvasNodes(current, incoming, doc);
    expect(next[0].position).toEqual({ x: 900, y: 700 });
    expect(next.slice(1)).toEqual(current);
    const a = reconcileCanvasNodes(current, incoming, view);
    const b = reconcileCanvasNodes(current, incoming, view);
    expect(a[0].position).toEqual(b[0].position);
    expect(a.slice(1)).toEqual(current);
  });
  it("removes deleted nodes without moving surviving nodes", () => {
    const current = dragged();
    expect(reconcileCanvasNodes(current, { ...graph, nodes: graph.nodes.slice(1) }, view)).toEqual(current.slice(1));
  });
  it("keeps optimistic x=500 across a pending save, running refresh and save response", () => {
    let current = dragged();
    const running = { ...graph, nodes: [{ ...graph.nodes[0], status: "running" as const }, ...graph.nodes.slice(1)] };
    current = reconcileCanvasNodes(current, running, view);
    expect(current[0].position.x).toBe(500);
    const saved = { ...view, view_revision: 2, node_layouts: [{ ...view.node_layouts[0], x: 500 }] };
    current = reconcileCanvasNodes(current, running, saved);
    expect(current[0].position).toEqual({ x: 500, y: 50 });
  });
  it("does not apply stale document layouts even after a newer local drag", () => {
    const current = dragged();
    expect(reconcileCanvasNodes(current, graph, { ...view, view_revision: 99 })[0].position.x).toBe(500);
  });
  it("applies an explicit layout once without changing data or selection", () => {
    const current = dragged();
    const next = reconcileCanvasNodes(current, graph, view, { layouts: [{ ...view.node_layouts[0], x: 800 }] });
    expect(next[0].position.x).toBe(800);
    expect(next[0].selected).toBe(true);
    expect(reconcileCanvasNodes(next, graph, view)[0].position.x).toBe(800);
  });
  it("retains hidden group members and their unsaved positions", () => {
    const current = dragged();
    const collapsed = { ...view, groups: [{ id: "g", title: "Group", node_ids: ["A", "B"], collapsed: true }] };
    const hidden = reconcileCanvasNodes(current, graph, collapsed);
    expect(hidden[0].hidden).toBe(true);
    expect(hidden[2]).toBe(current[2]);
    expect(reconcileCanvasNodes(hidden, graph, view)[0].position.x).toBe(500);
  });
  it("reuses all 99 unaffected node objects on a 100-node refresh", () => {
    const large = { ...graph, nodes: Array.from({ length: 100 }, (_, i) => task(String(i))) };
    const current = reconcileCanvasNodes([], large, null);
    const incoming = structuredClone(large);
    incoming.nodes[42].status = "running";
    const next = reconcileCanvasNodes(current, incoming, null);
    expect(next.filter((node, i) => node !== current[i])).toHaveLength(1);
    expect(reconcileCanvasNodes(next, incoming, null)).toBe(next);
  });
});
