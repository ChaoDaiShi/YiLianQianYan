export const MESSAGE_NODE_WIDTH = 288;
export const MESSAGE_NODE_HEIGHT = 152;

export interface MessagePoint { x: number; y: number }

/** Choose opposing card edges, including nodes the user has dragged across a branch. */
export function messageEdgeHandles(source: MessagePoint, target: MessagePoint) {
  const dx = target.x - source.x;
  const dy = target.y - source.y;
  if (Math.abs(dx) >= MESSAGE_NODE_WIDTH + 16 && Math.abs(dx) >= Math.abs(dy) * 0.75) {
    return dx > 0
      ? { sourceHandle: "out-right", targetHandle: "in-left" }
      : { sourceHandle: "out-left", targetHandle: "in-right" };
  }
  return dy >= 0
    ? { sourceHandle: "out-bottom", targetHandle: "in-top" }
    : { sourceHandle: "out-top", targetHandle: "in-bottom" };
}
