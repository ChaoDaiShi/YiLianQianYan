export interface ExecutionDetailAnchor {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface ExecutionDetailViewport {
  width: number;
  height: number;
}

export interface ExecutionDetailPlacement {
  left: number;
  width: number;
  verticalEdge: "top" | "bottom";
  verticalOffset: number;
}

const VIEWPORT_MARGIN = 12;
const PANEL_GAP = 12;
const PREFERRED_WIDTH = 420;
const MIN_LEFT_PANEL_WIDTH = 280;

export function getExecutionDetailPlacement(
  anchor: ExecutionDetailAnchor,
  viewport: ExecutionDetailViewport,
): ExecutionDetailPlacement {
  const viewportWidth = Math.max(viewport.width, VIEWPORT_MARGIN * 2);
  const availableOnLeft = anchor.left - PANEL_GAP - VIEWPORT_MARGIN;
  const canStayEntirelyLeft = availableOnLeft >= MIN_LEFT_PANEL_WIDTH;
  const width = canStayEntirelyLeft
    ? Math.min(PREFERRED_WIDTH, availableOnLeft)
    : Math.min(PREFERRED_WIDTH, viewportWidth - VIEWPORT_MARGIN * 2);
  const idealLeft = anchor.left - PANEL_GAP - width;
  const maxLeft = viewportWidth - VIEWPORT_MARGIN - width;
  const left = Math.max(VIEWPORT_MARGIN, Math.min(idealLeft, maxLeft));
  const verticalEdge = anchor.top <= viewport.height / 2 ? "top" : "bottom";
  const verticalOffset =
    verticalEdge === "top"
      ? Math.max(VIEWPORT_MARGIN, anchor.top)
      : Math.max(VIEWPORT_MARGIN, viewport.height - anchor.bottom);

  return { left, width, verticalEdge, verticalOffset };
}
