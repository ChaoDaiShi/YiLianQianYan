import { useEffect, useState } from "react";
import { subscribeToEvents } from "../../../api/events";
import { isTaskWorldEvent } from "../taskGraphProjection";

/**
 * Streams task-world invalidations into the authoritative projections.
 *
 * A running node is deliberately treated as an invalidation rather than as
 * state to apply: the page re-reads the backend projection instead of
 * reproducing TaskSupervisor in React.
 */
export function useTaskWorldEvents(
  graphId: string,
  refreshDetail: () => Promise<void>,
  refreshView: () => Promise<void>,
) {
  const [eventWarning, setEventWarning] = useState<string | null>(null);

  useEffect(() => {
    if (!graphId) return;
    const controller = subscribeToEvents((event) => {
      if (!isTaskWorldEvent(event, graphId)) return;
      if (event.type === "task.node.running") void refreshDetail();
      else if (event.type === "task.canvas.updated") void refreshView();
      else void refreshDetail();
    }, (streamError) => setEventWarning(streamError.message));
    return () => controller.abort();
  }, [graphId, refreshDetail, refreshView]);

  return eventWarning;
}
