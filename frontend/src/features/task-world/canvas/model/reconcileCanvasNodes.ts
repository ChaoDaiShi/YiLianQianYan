import type { CanvasNodeLayout, CanvasView } from "../../../../api/taskWorld";
import { toReactFlowModel, type TaskGraphCanvasNode, type TaskGraphProjection } from "../../taskGraphProjection";

export function reconcileCanvasNodes(
  _current: TaskGraphCanvasNode[],
  projection: TaskGraphProjection,
  view: CanvasView | null,
  options: { focusedNodeId?: string | null; layouts?: CanvasNodeLayout[] } = {},
): TaskGraphCanvasNode[] {
  return toReactFlowModel(projection, view, options.focusedNodeId).nodes;
}
