import type { FitViewOptions } from "@xyflow/react";
import type { CanvasViewport } from "../../../../api/taskWorld";

export type CanvasCameraEvent =
  | { type: "selection" | "status" | "execution" | "save-response" | "resize" }
  | { type: "fit-all" | "zoom-100" }
  | { type: "locate"; nodeId: string };

/** Pure permission boundary: data/selection events never authorize a camera move. */
export function canvasCameraCommand(event: CanvasCameraEvent):
  { kind: "fit"; options: FitViewOptions } | { kind: "zoom"; zoom: number } | null {
  switch (event.type) {
    case "fit-all": return { kind: "fit", options: { padding: 0.2 } };
    case "locate": return { kind: "fit", options: { nodes: [{ id: event.nodeId }], padding: 0.25, maxZoom: 1.35 } };
    case "zoom-100": return { kind: "zoom", zoom: 1 };
    default: return null;
  }
}

export function initialCanvasViewport(saved: CanvasViewport | null | undefined): CanvasViewport {
  return saved ? { ...saved } : { x: 0, y: 0, zoom: 1 };
}
