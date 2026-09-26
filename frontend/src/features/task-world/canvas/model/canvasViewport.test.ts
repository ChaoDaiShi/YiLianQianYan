import { describe, expect, it } from "vitest";
import { canvasCameraCommand, initialCanvasViewport } from "./canvasViewport";
import source from "../TaskWorldCanvas.tsx?raw";

describe("user-owned canvas camera", () => {
  it.each(["selection", "status", "execution", "save-response", "resize"] as const)("%s cannot issue a camera command", (type) => {
    const current = { x: 340, y: -120, zoom: 1.35 };
    const command = canvasCameraCommand({ type });
    expect(command).toBeNull();
    expect(current).toEqual({ x: 340, y: -120, zoom: 1.35 });
  });
  it("only explicit Fit All, Locate and 100% issue movement commands", () => {
    expect(canvasCameraCommand({ type: "fit-all" })).toEqual({ kind: "fit", options: { padding: 0.2 } });
    expect(canvasCameraCommand({ type: "locate", nodeId: "B" })).toEqual({ kind: "fit", options: { nodes: [{ id: "B" }], padding: 0.25, maxZoom: 1.35 } });
    expect(canvasCameraCommand({ type: "zoom-100" })).toEqual({ kind: "zoom", zoom: 1 });
  });
  it("hydrates exactly the saved viewport on each mount without fitting", () => {
    const saved = { x: 340, y: -120, zoom: 1.35 };
    const firstMount = initialCanvasViewport(saved);
    expect(firstMount).toEqual(saved);
    expect(firstMount).not.toBe(saved);
    expect(initialCanvasViewport({ ...firstMount })).toEqual(saved);
    expect(initialCanvasViewport(null)).toEqual({ x: 0, y: 0, zoom: 1 });
  });
  it("does not enable ReactFlow implicit fit or keyboard auto-pan", () => {
    expect(source).not.toMatch(/^\s+fitView\s*$/m);
    expect(source).toContain("autoPanOnNodeFocus={false}");
    expect(source).not.toContain("[fitView, focusedNodeId, nodes]");
    expect(source).toContain("defaultViewport={initialViewport}");
  });
});
