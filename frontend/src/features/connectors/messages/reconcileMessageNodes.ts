import type { Node } from "@xyflow/react";

/** Message refreshes own content and saved positions. React Flow owns the
 * measured dimensions and drag state already present on each controlled node. */
export function reconcileMessageNodes<T extends Node>(current: T[], projection: T[]): T[] {
  const existing = new Map(current.map((node) => [node.id, node]));
  return projection.map((node) => {
    const previous = existing.get(node.id);
    return previous ? { ...previous, ...node } : node;
  });
}
