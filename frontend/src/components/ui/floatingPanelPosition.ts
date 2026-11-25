type Anchor = Pick<DOMRectReadOnly, "top" | "right" | "bottom" | "left">;
type Side = "right" | "left" | "below" | "above";

export function placeFloatingPanel(
  anchor: Anchor, panel: { width: number; height: number }, viewport: { width: number; height: number },
): { left: number; top: number; side: Side } {
  const edge = 12;
  const gap = 8;
  const maxLeft = Math.max(edge, viewport.width - panel.width - edge);
  const maxTop = Math.max(edge, viewport.height - panel.height - edge);
  const right = anchor.right + gap;
  const left = anchor.left - gap - panel.width;
  const below = anchor.bottom + gap;
  const above = anchor.top - gap - panel.height;
  const sides: Side[] = ["right", "left", "above", "below"];
  const fits = { right: right <= maxLeft, left: left >= edge, above: above >= edge, below: below <= maxTop };
  const side = sides.find(candidate => fits[candidate]) ?? (anchor.top > viewport.height - anchor.bottom ? "above" : "below");
  const x = side === "right" ? right : side === "left" ? left : anchor.left;
  const y = side === "above" ? above : side === "below" ? below : anchor.bottom - panel.height;
  return { left: Math.min(maxLeft, Math.max(edge, x)), top: Math.min(maxTop, Math.max(edge, y)), side };
}
