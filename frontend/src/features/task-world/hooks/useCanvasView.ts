import { useCallback, useMemo, useRef, useState } from "react";
import {
  getCanvasView,
  saveCanvasView,
  type ApiError,
  type CanvasNodeLayout,
  type CanvasView,
  type CanvasViewport,
} from "../../../api/taskWorld";
import { createCanvasViewWriteQueue } from "../canvasViewWriter";

/**
 * The canvas projection: viewport, layouts, selection and visual groups.
 *
 * This state is *presentation only*. It is written through a queue so writes
 * never interleave, and an incoming view with an older revision is ignored
 * rather than allowed to roll the surface backwards.
 */
export function useCanvasView(
  graphId: string,
  onError: (error: ApiError) => void,
  onSaveError: (error: ApiError | null) => void,
  focusedNodeId: string | null,
  onClearFocus: () => void,
) {
  const [view, setView] = useState<CanvasView | null>(null);
  const viewRef = useRef<CanvasView | null>(null);

  const acceptView = useCallback((incoming: CanvasView) => {
    const current = viewRef.current;
    if (
      current
      && current.graph_id === incoming.graph_id
      && current.view_revision > incoming.view_revision
    ) return;
    viewRef.current = incoming;
    setView(incoming);
  }, []);

  const refreshView = useCallback(async () => {
    if (!graphId) return;
    const result = await getCanvasView(graphId);
    if (result.ok) acceptView(result.data);
    else onError(result.error);
  }, [acceptView, graphId, onError]);

  const viewWriter = useMemo(() => createCanvasViewWriteQueue({
    readCurrent: () => viewRef.current,
    acceptCurrent: acceptView,
    save: (next) => saveCanvasView(graphId, next),
    onError: async (writeError) => {
      onSaveError(writeError);
      if (writeError.code === "stale_view_revision") await refreshView();
    },
  }), [acceptView, graphId, onSaveError, refreshView]);

  const persistView = useCallback((update: (current: CanvasView) => CanvasView) => {
    onSaveError(null);
    void viewWriter.enqueue(update);
  }, [onSaveError, viewWriter]);

  const updateLayouts = useCallback((changed: CanvasNodeLayout[]) => {
    persistView((current) => {
      const changedById = new Map(changed.map((layout) => [layout.node_id, layout]));
      const node_layouts = current.node_layouts.map((layout) => changedById.get(layout.node_id) || layout);
      for (const layout of changed) if (!node_layouts.some((item) => item.node_id === layout.node_id)) node_layouts.push(layout);
      return { ...current, node_layouts };
    });
  }, [persistView]);

  const updateViewport = useCallback((viewport: CanvasViewport) => {
    persistView((current) => (
      current.viewport.x === viewport.x
      && current.viewport.y === viewport.y
      && current.viewport.zoom === viewport.zoom
        ? current
        : { ...current, viewport }
    ));
  }, [persistView]);

  const updateSelection = useCallback((selection: string[]) => {
    persistView((current) => selection.join("\0") === current.selection.join("\0")
      ? current
      : { ...current, selection });
  }, [persistView]);

  const createVisualGroup = useCallback(() => {
    if (!view) return;
    const alreadyGrouped = new Set(view.groups.flatMap((group) => group.node_ids));
    const node_ids = view.selection.filter((nodeId) => !alreadyGrouped.has(nodeId));
    if (node_ids.length < 2) return;
    persistView((current) => ({
      ...current,
      selection: [],
      groups: [...current.groups, {
        id: `group-${crypto.randomUUID()}`,
        title: `分组 ${current.groups.length + 1}`,
        node_ids,
        collapsed: false,
      }],
    }));
  }, [persistView, view]);

  const toggleVisualGroup = useCallback((groupId: string) => {
    const group = view?.groups.find((candidate) => candidate.id === groupId);
    if (group && !group.collapsed && focusedNodeId && group.node_ids.includes(focusedNodeId)) {
      onClearFocus();
    }
    persistView((current) => ({
      ...current,
      groups: current.groups.map((candidate) => candidate.id === groupId
        ? { ...candidate, collapsed: !candidate.collapsed }
        : candidate),
    }));
  }, [focusedNodeId, onClearFocus, persistView, view]);

  const removeVisualGroup = useCallback((groupId: string) => {
    persistView((current) => ({
      ...current,
      groups: current.groups.filter((group) => group.id !== groupId),
    }));
  }, [persistView]);

  return {
    view,
    refreshView,
    updateLayouts,
    updateViewport,
    updateSelection,
    createVisualGroup,
    toggleVisualGroup,
    removeVisualGroup,
  };
}
