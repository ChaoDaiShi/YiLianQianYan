import { useCallback, useRef, useState } from "react";
import {
  getTaskGraphDetail,
  type ApiError,
  type TaskGraphDetail,
} from "../../../api/taskWorld";

/**
 * The authoritative semantic graph.
 *
 * A sequence ticket drops responses from superseded requests, so a slow load
 * cannot overwrite a newer one. Repairing which node is focused is delegated:
 * the caller owns that selection, and only the loaded node ids are handed over.
 */
export function useTaskGraph(
  graphId: string,
  onNodesLoaded: (nodeIds: string[]) => void,
) {
  const [detail, setDetail] = useState<TaskGraphDetail | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const sequence = useRef(0);

  const refreshDetail = useCallback(async () => {
    if (!graphId) return;
    const ticket = ++sequence.current;
    const result = await getTaskGraphDetail(graphId);
    if (ticket !== sequence.current) return;
    if (result.ok) {
      setDetail(result.data);
      setError(null);
      onNodesLoaded(result.data.nodes.map((node) => node.id));
    } else {
      setError(result.error);
    }
  }, [graphId, onNodesLoaded]);

  return { detail, error, setError, refreshDetail };
}
