import type { CanvasNodeLayout, CanvasView } from "../../../../api/taskWorld";
import type { TaskGraphCanvasNode, TaskGraphProjection } from "../../taskGraphProjection";

/** Document positions hydrate new nodes only. Explicit local layout commands
 * are deliberately separate from document revisions and save acknowledgements. */
export function reconcileCanvasNodes(
  current: TaskGraphCanvasNode[],
  projection: TaskGraphProjection,
  view: CanvasView | null,
  options: { focusedNodeId?: string | null; layouts?: CanvasNodeLayout[] } = {},
): TaskGraphCanvasNode[] {
  const existing = new Map(current.map((node) => [node.id, node]));
  const saved = new Map(view?.node_layouts.map((layout) => [layout.node_id, layout]));
  const explicit = new Map(options.layouts?.map((layout) => [layout.node_id, layout]));
  const hidden = new Set(view?.groups.filter((group) => group.collapsed).flatMap((group) => group.node_ids));
  const next = projection.nodes.map((task, index): TaskGraphCanvasNode => {
    const previous = existing.get(task.id);
    const layout = explicit.get(task.id) ?? (!previous ? saved.get(task.id) : undefined);
    const position = layout ? { x: layout.x, y: layout.y }
      : previous?.position ?? { x: 40 + (index % 4) * 320, y: 40 + Math.floor(index / 4) * 200 };
    const data = {
      task, role: task.role, status: task.status,
      executionStatus: task.latest_execution?.status || null,
      isRunning: task.isRunning, isFocused: options.focusedNodeId === task.id,
    };
    // Projection fetches deserialize fresh objects; retain stable references for
    // unchanged cards (including nested execution/results) before React Flow.
    const sameData = previous && JSON.stringify(previous.data) === JSON.stringify(data);
    const isHidden = hidden.has(task.id);
    if (previous && sameData && previous.hidden === isHidden && !layout) return previous;
    return {
      ...previous,
      id: task.id, type: "task-world", position,
      width: layout?.width ?? previous?.width ?? 240,
      height: layout?.height ?? previous?.height ?? 128,
      hidden: isHidden,
      selected: previous ? previous.selected : view?.selection.includes(task.id) || false,
      data: sameData ? previous.data : data,
    };
  });
  return next.length === current.length && next.every((node, i) => node === current[i]) ? current : next;
}
